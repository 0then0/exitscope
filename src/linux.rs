//! The only raw syscall boundary: nix lacks pidfd wrappers. No PID-based fallback.
use crate::model::Identity;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::sys::signal::{SigSet, Signal};
use nix::sys::signalfd::{SfdFlags, SignalFd};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn signals() -> Result<SignalFd> {
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGTERM);
    mask.add(Signal::SIGINT);
    mask.thread_block()?;
    Ok(SignalFd::with_flags(
        &mask,
        SfdFlags::SFD_CLOEXEC | SfdFlags::SFD_NONBLOCK,
    )?)
}
pub fn nonblocking(fd: impl AsFd) -> Result<()> {
    let old = OFlag::from_bits_truncate(fcntl(&fd, FcntlArg::F_GETFL)?);
    fcntl(fd, FcntlArg::F_SETFL(old | OFlag::O_NONBLOCK))?;
    Ok(())
}
pub fn pidfd(pid: i32) -> io::Result<OwnedFd> {
    // SAFETY: pidfd_open has only scalar arguments and returns a fresh descriptor.
    let fd = unsafe { nix::libc::syscall(nix::libc::SYS_pidfd_open, pid, 0) };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: ownership of the newly returned descriptor is transferred exactly once.
        Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
    }
}
pub fn pidfd_signal(fd: &OwnedFd, signal: Signal) -> io::Result<()> {
    // SAFETY: valid borrowed descriptor, scalar signal, null optional siginfo, zero flags.
    let rc = unsafe {
        nix::libc::syscall(
            nix::libc::SYS_pidfd_send_signal,
            fd.as_raw_fd(),
            signal as i32,
            std::ptr::null::<nix::libc::siginfo_t>(),
            0,
        )
    };
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
pub fn exited(fd: &OwnedFd) -> Result<bool> {
    use nix::poll::{PollFd, PollFlags, poll};
    let mut fds = [PollFd::new(fd.as_fd(), PollFlags::POLLIN)];
    poll(&mut fds, 0u8)?;
    Ok(fds[0]
        .revents()
        .is_some_and(|f| f.contains(PollFlags::POLLIN)))
}
pub fn identity(pid: i32) -> io::Result<Identity> {
    let s = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let tail = s
        .rsplit_once(") ")
        .ok_or_else(|| io::Error::other("invalid proc stat"))?
        .1;
    let f: Vec<_> = tail.split_whitespace().collect();
    if f.len() < 20 {
        return Err(io::Error::other("short proc stat"));
    }
    let parse = |i: usize| f[i].parse::<i32>().map_err(io::Error::other);
    Ok(Identity {
        pid,
        start_ticks: f[19].parse().map_err(io::Error::other)?,
        ppid: parse(1)?,
        pgid: parse(2)?,
        sid: parse(3)?,
        state: f[0].into(),
    })
}

pub struct Scope {
    pub path: PathBuf,
    kill: File,
}
impl Scope {
    pub fn create(parent: &Path, name: &str) -> Result<Self> {
        use nix::sys::statfs::{CGROUP2_SUPER_MAGIC, statfs};
        if statfs(parent)?.filesystem_type() != CGROUP2_SUPER_MAGIC {
            return Err("parent is not cgroup v2".into());
        }
        let parent = fs::canonicalize(parent)?;
        fs::create_dir(parent.join(name))?;
        let path = parent.join(name);
        match OpenOptions::new()
            .write(true)
            .open(path.join("cgroup.kill"))
        {
            Ok(kill) => Ok(Self { path, kill }),
            Err(e) => {
                let _ = fs::remove_dir(&path);
                Err(e.into())
            }
        }
    }
    pub fn pids(&self) -> Result<Vec<i32>> {
        fn visit(path: &Path, pids: &mut Vec<i32>, dirs: &mut usize) -> Result<()> {
            *dirs += 1;
            if *dirs > 256 {
                return Err("cgroup directory limit exceeded".into());
            }
            for line in fs::read_to_string(path.join("cgroup.procs"))?.lines() {
                pids.push(line.parse()?);
                if pids.len() > 4096 {
                    return Err("cgroup member limit exceeded".into());
                }
            }
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    visit(&entry.path(), pids, dirs)?;
                }
            }
            Ok(())
        }
        let mut pids = vec![];
        visit(&self.path, &mut pids, &mut 0)?;
        pids.sort_unstable();
        pids.dedup();
        Ok(pids)
    }
    pub fn members(&self) -> Result<Vec<Identity>> {
        let mut result = vec![];
        for pid in self.pids()? {
            // Bind the proc identity to an open pidfd and recheck membership after opening.
            match pidfd(pid) {
                Ok(fd) => {
                    let ident = match identity(pid) {
                        Ok(i) => i,
                        Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                        Err(e) => return Err(e.into()),
                    };
                    if !exited(&fd)? && self.pids()?.contains(&pid) {
                        result.push(ident);
                    }
                }
                Err(e) if e.raw_os_error() == Some(nix::libc::ESRCH) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(result)
    }
    pub fn populated(&self) -> Result<bool> {
        Ok(fs::read_to_string(self.path.join("cgroup.events"))?
            .lines()
            .any(|l| l == "populated 1"))
    }
    pub fn cleanup(&mut self) -> Result<()> {
        self.kill.write_all(b"1")?;
        let end = Instant::now() + Duration::from_secs(2);
        while self.populated()? && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
        if self.populated()? {
            return Err("cgroup still populated after cleanup deadline".into());
        }
        fn remove(path: &Path) -> io::Result<()> {
            for e in fs::read_dir(path)? {
                let e = e?;
                if e.file_type()?.is_dir() {
                    remove(&e.path())?;
                }
            }
            fs::remove_dir(path)
        }
        remove(&self.path)?;
        Ok(())
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = self.cleanup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proc_identity_is_own_process() {
        let i = identity(std::process::id() as i32).unwrap();
        assert!(i.start_ticks > 0);
        assert_eq!(i.pid, std::process::id() as i32);
    }
    #[test]
    fn pidfd_reports_exit_without_reaping() {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 7"])
            .spawn()
            .unwrap();
        let fd = pidfd(child.id() as i32).unwrap();
        let end = Instant::now() + Duration::from_secs(2);
        while !exited(&fd).unwrap() && Instant::now() < end {
            std::thread::yield_now();
        }
        assert!(exited(&fd).unwrap());
        assert_eq!(child.wait().unwrap().code(), Some(7));
        assert!(pidfd_signal(&fd, Signal::SIGTERM).is_err());
    }
}
