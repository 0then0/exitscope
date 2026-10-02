use crate::linux::{self, Result, Scope};
use crate::model::*;
use nix::sys::signal::{SigSet, Signal};
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

struct Resources {
    scope: Option<Scope>,
    temp: PathBuf,
    child: Option<Child>,
    root_fd: Option<OwnedFd>,
    target_started: bool,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
}
impl Resources {
    fn cleanup(&mut self, report: &mut Report) {
        if let Some(scope) = &mut self.scope {
            match scope.cleanup() {
                Ok(()) => report
                    .cleanup
                    .push("cgroup.kill; empty confirmed; run cgroup removed".into()),
                Err(e) => {
                    report.cleanup.push(format!("cgroup cleanup failed: {e}"));
                    report.finding("CLEANUP_ERROR", e.to_string());
                    report.outcome = Outcome::InfrastructureError;
                }
            }
        }
        if !self.target_started {
            if let Some(fd) = &self.root_fd {
                let _ = linux::pidfd_signal(fd, Signal::SIGKILL);
            } else if let Some(child) = &mut self.child {
                let _ = child.kill();
            } // Own blocked launcher; never authorized to exec.
        }
        if let Some(child) = &mut self.child {
            let end = Instant::now() + Duration::from_secs(2);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Err(e) => {
                        report.cleanup.push(format!("root reap error: {e}"));
                        report.finding("CLEANUP_ERROR", "root reaping failed");
                        report.outcome = Outcome::InfrastructureError;
                        break;
                    }
                    _ if Instant::now() >= end => {
                        report.cleanup.push("root reap deadline exceeded".into());
                        report.finding("CLEANUP_ERROR", "root reap deadline exceeded");
                        report.outcome = Outcome::InfrastructureError;
                        break;
                    }
                    _ => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        }
        match fs::remove_dir_all(&self.temp) {
            Ok(()) => report.cleanup.push("private IPC directory removed".into()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                report.cleanup.push(format!("IPC removal failed: {e}"));
                report.finding("CLEANUP_ERROR", "IPC directory removal failed");
                report.outcome = Outcome::InfrastructureError;
            }
        }
    }
}
impl Drop for Resources {
    fn drop(&mut self) {
        if !self.target_started {
            if let Some(fd) = &self.root_fd {
                let _ = linux::pidfd_signal(fd, Signal::SIGKILL);
            }
        }
        let _ = fs::remove_dir_all(&self.temp);
    }
}

pub fn launch(
    cgroup: &Path,
    socket: &Path,
    cwd: &Path,
    executable: &str,
    args: &[String],
) -> Result<()> {
    // Connect before joining so even delegation/setup errors have a bounded diagnostic channel.
    let mut connection = UnixStream::connect(socket)?;
    let setup = |connection: &mut UnixStream, stage: &str, error: io::Error| -> io::Error {
        let _ = writeln!(
            connection,
            "{}",
            serde_json::json!({
                "kind":"launch_error", "stage":stage, "errno":error.raw_os_error()
            })
        );
        error
    };
    SigSet::empty().thread_set_mask().map_err(|e| {
        setup(
            &mut connection,
            "signal_mask",
            io::Error::from_raw_os_error(e as i32),
        )
    })?;
    // This runs in a fresh exec of ExitScope, not in a multithreaded pre_exec callback.
    fs::write(cgroup.join("cgroup.procs"), b"0")
        .map_err(|e| setup(&mut connection, "cgroup_join", e))?;
    nix::unistd::setsid().map_err(|e| {
        setup(
            &mut connection,
            "setsid",
            io::Error::from_raw_os_error(e as i32),
        )
    })?;
    connection.set_read_timeout(Some(Duration::from_secs(10)))?;
    writeln!(connection, "{}", serde_json::json!({"kind":"gated"}))?;
    let mut ack = [0; 4];
    connection.read_exact(&mut ack)?;
    if &ack != b"ack\n" {
        return Err("launch gate denied".into());
    }
    // Resolve cwd separately so its failure is distinguishable from executable failure.
    std::env::set_current_dir(cwd).map_err(|e| setup(&mut connection, "working_directory", e))?;
    // UnixStream is CLOEXEC. EOF proves successful exec; an error is reported before closing.
    let err = Command::new(executable).args(args).exec();
    Err(setup(&mut connection, "exec", err).into())
}

struct Peer {
    stream: UnixStream,
    pid: i32,
    fd: OwnedFd,
    buffer: Vec<u8>,
    closed: bool,
    gated: bool,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    kind: String,
    #[serde(default)]
    signal: Option<i32>,
    #[serde(default)]
    stage: Option<String>,
    #[serde(default)]
    errno: Option<i32>,
}

struct Observation {
    peers: Vec<Peer>,
    worker: Option<i32>,
    worker_stage: usize,
    exec: bool,
    ready: bool,
    root_waited: Option<bool>,
}
impl Observation {
    fn tick(
        &mut self,
        listener: &UnixListener,
        scope: &Scope,
        root_pid: i32,
        root_fd: &OwnedFd,
        r: &mut Report,
        start: Instant,
    ) -> Result<()> {
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    if self.peers.len() >= 16 {
                        return Err("IPC peer limit exceeded".into());
                    }
                    let cred = nix::sys::socket::getsockopt(
                        &stream,
                        nix::sys::socket::sockopt::PeerCredentials,
                    )?;
                    let pid = cred.pid();
                    let fd = linux::pidfd(pid)?;
                    if pid != root_pid && (!scope.pids()?.contains(&pid) || linux::exited(&fd)?) {
                        return Err("IPC peer outside run cgroup".into());
                    }
                    stream.set_nonblocking(true)?;
                    self.peers.push(Peer {
                        stream,
                        pid,
                        fd,
                        buffer: vec![],
                        closed: false,
                        gated: false,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        for p in &mut self.peers {
            if p.closed {
                continue;
            }
            let mut buf = [0; 1024];
            // Bounded draining prevents a flooding peer from starving deadlines.
            for _ in 0..8 {
                match p.stream.read(&mut buf) {
                    Ok(0) => {
                        p.closed = true;
                        if p.gated {
                            self.exec = true;
                        }
                        break;
                    }
                    Ok(n) => p.buffer.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == io::ErrorKind::ConnectionReset => {
                        p.closed = true;
                        break;
                    }
                    Err(e) => return Err(e.into()),
                }
                if p.buffer.len() > 4096 {
                    return Err("IPC event buffer limit exceeded".into());
                }
            }
            while let Some(pos) = p.buffer.iter().position(|b| *b == b'\n') {
                if r.events.len() >= 64 {
                    return Err("fixture event limit exceeded".into());
                }
                let line: Vec<_> = p.buffer.drain(..=pos).collect();
                let wire: Wire = serde_json::from_slice(&line)?;
                if (wire.kind == "signal_received") != wire.signal.is_some() {
                    return Err("signal field is required only on signal_received".into());
                }
                if wire.kind == "launch_error" {
                    if p.pid != root_pid
                        || !matches!(
                            wire.stage.as_deref(),
                            Some(
                                "signal_mask"
                                    | "cgroup_join"
                                    | "setsid"
                                    | "working_directory"
                                    | "exec"
                            )
                        )
                    {
                        return Err("invalid launcher diagnostic".into());
                    }
                    r.launch_error = Some(LaunchError {
                        stage: wire.stage.unwrap(),
                        errno: wire.errno,
                    });
                    let error = r.launch_error.as_ref().unwrap();
                    return Err(format!(
                        "launcher {} failed (errno {:?})",
                        error.stage, error.errno
                    )
                    .into());
                }
                if wire.stage.is_some() || wire.errno.is_some() {
                    return Err("diagnostic fields are only valid on launch_error".into());
                }
                if wire.kind == "gated" {
                    if p.pid != root_pid || p.gated {
                        return Err("invalid launch gate identity".into());
                    }
                    let root = linux::identity(root_pid)?;
                    if root.pgid != root_pid
                        || root.sid != root_pid
                        || !scope.pids()?.contains(&root_pid)
                    {
                        return Err("root containment/topology failed".into());
                    }
                    p.gated = true;
                    r.root = Some(root);
                } else {
                    if self.worker.is_none() {
                        self.worker = Some(p.pid);
                    }
                    if self.worker != Some(p.pid) {
                        return Err("multiple workers are unsupported in v0.1".into());
                    }
                    let dead = linux::exited(&p.fd)?;
                    // Pinned, authenticated peers may leave queued events when killed by the wrapper.
                    if !dead && !scope.pids()?.contains(&p.pid) {
                        return Err("worker left run cgroup during handshake".into());
                    }
                    if dead && self.worker_stage < 2 {
                        return Err("worker exited before readiness handshake".into());
                    }
                    let expected = [
                        "handler_installed",
                        "ready",
                        "signal_received",
                        "cleanup_started",
                        "cleanup_finished",
                    ];
                    if expected.get(self.worker_stage) != Some(&wire.kind.as_str()) {
                        return Err("invalid fixture event sequence".into());
                    }
                    self.worker_stage += 1;
                    if wire.kind == "ready" {
                        self.ready = true;
                    }
                    if wire.kind == "cleanup_finished" {
                        self.root_waited = Some(!linux::exited(root_fd)?);
                    }
                }
                r.events.push(Event {
                    at_ms: millis(start),
                    pid: p.pid,
                    kind: wire.kind,
                    signal: wire.signal,
                });
                // Tiny acknowledgements on a local socket cannot block the observer.
                if let Err(e) = p.stream.write_all(b"ack\n") {
                    if self.worker == Some(p.pid)
                        && matches!(
                            e.kind(),
                            io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                        )
                    {
                        p.closed = true;
                    } else {
                        return Err(e.into());
                    }
                }
            }
            if p.closed && !p.buffer.is_empty() {
                if self.worker == Some(p.pid) && self.worker_stage >= 2 && linux::exited(&p.fd)? {
                    p.buffer.clear(); // Target died mid-frame; missing completion is a contract failure.
                } else {
                    return Err("truncated fixture frame".into());
                }
            }
        }
        Ok(())
    }
}

fn drain(pipe: &mut impl Read, state: &mut StreamState, start: Instant) -> Result<()> {
    if state.eof_at_ms.is_some() {
        return Ok(());
    }
    let mut buf = [0; 8192];
    for _ in 0..16 {
        match pipe.read(&mut buf) {
            Ok(0) => {
                state.eof_at_ms = Some(millis(start));
                break;
            }
            Ok(n) => state.bytes = state.bytes.saturating_add(n as u64),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn millis(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

pub fn run(config_path: &Path, parent: &Path) -> Report {
    let start = Instant::now();
    let mut r = Report::default();
    r.environment.push(format!(
        "os={}; arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    if let Ok(release) = fs::read_to_string("/proc/sys/kernel/osrelease") {
        r.environment.push(format!("kernel={}", release.trim()));
    }
    let temp = std::env::temp_dir().join(format!(
        "exitscope-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut resources = Resources {
        scope: None,
        temp,
        child: None,
        root_fd: None,
        target_started: false,
        stdout: None,
        stderr: None,
    };
    let mut obs = Observation {
        peers: vec![],
        worker: None,
        worker_stage: 0,
        exec: false,
        ready: false,
        root_waited: None,
    };
    let result = (|| -> Result<()> {
        let mut bytes = Vec::new();
        fs::File::open(config_path)?
            .take(65537)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 65536 {
            return Err("config exceeds 64 KiB".into());
        }
        let config: Config = serde_json::from_slice(&bytes)?;
        config.validate()?;
        r.config = Some(config.clone());
        let signals = linux::signals()?;
        fs::create_dir(&resources.temp)?;
        fs::set_permissions(&resources.temp, fs::Permissions::from_mode(0o700))?;
        let name = resources.temp.file_name().unwrap().to_str().unwrap();
        resources.scope = Some(Scope::create(parent, name)?);
        let scope = resources.scope.as_ref().unwrap();
        r.cgroup = Some(scope.path.clone());
        r.environment
            .push("cgroup_v2=true; cgroup.kill writable; private run directory created".into());
        let socket = resources.temp.join("control.sock");
        let listener = UnixListener::bind(&socket)?;
        listener.set_nonblocking(true)?;
        let binary = std::env::current_exe()?;
        let replace = |s: &str| s.replace("{fixture}", binary.to_str().unwrap());
        let executable = replace(&config.executable);
        let args: Vec<_> = config.args.iter().map(|s| replace(s)).collect();
        resources.child = Some(
            Command::new(&binary)
                .arg("launch")
                .arg(&scope.path)
                .arg(&socket)
                .arg(&config.cwd)
                .arg(&executable)
                .arg("--")
                .args(&args)
                .env("EXIT_SCOPE_SOCKET", &socket)
                .env("EXIT_SCOPE_BIN", &binary)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        let child = resources.child.as_mut().unwrap();
        let pid = child.id() as i32;
        resources.root_fd = Some(linux::pidfd(pid)?);
        let root_fd = resources.root_fd.as_ref().unwrap();
        r.environment
            .push("pidfd_open and pidfd polling available".into());
        resources.stdout = child.stdout.take();
        resources.stderr = child.stderr.take();
        let stdout = resources.stdout.as_mut().unwrap();
        let stderr = resources.stderr.as_mut().unwrap();
        linux::nonblocking(&stdout)?;
        linux::nonblocking(&stderr)?;
        let mut snapshot = false;
        loop {
            if signals.read_signal()?.is_some() {
                r.finding("RUNNER_INTERRUPTED", "runner received SIGINT/SIGTERM");
                return Err("runner interrupted".into());
            }
            let tick = obs.tick(&listener, scope, pid, root_fd, &mut r, start);
            resources.target_started |= r.root.is_some();
            tick?;
            drain(stdout, &mut r.stdout, start)?;
            drain(stderr, &mut r.stderr, start)?;
            if r.root_exit.is_none() {
                if let Some(status) = child.try_wait()? {
                    r.root_exit = Some(RootExit {
                        at_ms: millis(start),
                        code: status.code(),
                        signal: status.signal(),
                    });
                }
            }
            if r.impact_at_ms.is_none() {
                if millis(start) >= config.readiness_ms {
                    r.finding(
                        "READINESS_TIMEOUT",
                        "readiness handshake did not finish before deadline",
                    );
                    return Err("readiness timeout".into());
                }
                if obs.exec && (config.readiness == "exec" || obs.ready) {
                    if linux::exited(root_fd)? {
                        return Err("root exited before test impact".into());
                    }
                    if !scope.pids()?.contains(&pid) {
                        return Err("root left run cgroup before impact".into());
                    }
                    match config.scenario {
                        Scenario::ParentSigterm => linux::pidfd_signal(root_fd, Signal::SIGTERM)?,
                        Scenario::GroupSigkill => {
                            // New private session; live root pidfd prevents PGID reuse here.
                            let ident = linux::identity(pid)?;
                            if ident.pgid != pid || ident.sid != pid {
                                return Err("root changed group before impact".into());
                            }
                            nix::sys::signal::killpg(
                                nix::unistd::Pid::from_raw(pid),
                                Signal::SIGKILL,
                            )?;
                        }
                    }
                    r.impact_at_ms = Some(millis(start));
                } else if r.root_exit.is_some() {
                    return Err("root exited before readiness".into());
                }
            } else {
                let impact = r.impact_at_ms.unwrap();
                if r.cgroup_empty_at_ms.is_none() && !scope.populated()? {
                    r.cgroup_empty_at_ms = Some(millis(start));
                }
                if !snapshot && millis(start) >= impact + config.shutdown_ms {
                    r.survivors = scope.members()?;
                    r.live_members_present = Some(scope.populated()?);
                    r.shutdown_snapshot_at_ms = Some(millis(start));
                    snapshot = true;
                    // events arriving after this snapshot cannot satisfy graceful deadlines.
                    r.stdout.eof_before_deadline = r
                        .stdout
                        .eof_at_ms
                        .is_some_and(|t| t <= impact + config.output_ms);
                    r.stderr.eof_before_deadline = r
                        .stderr
                        .eof_at_ms
                        .is_some_and(|t| t <= impact + config.output_ms);
                }
                if snapshot && millis(start) >= impact + config.shutdown_ms.max(config.output_ms) {
                    r.stdout.eof_before_deadline = r
                        .stdout
                        .eof_at_ms
                        .is_some_and(|t| t <= impact + config.output_ms);
                    r.stderr.eof_before_deadline = r
                        .stderr
                        .eof_at_ms
                        .is_some_and(|t| t <= impact + config.output_ms);
                    // Freeze deadline evidence; late events remain in the report but never turn a failure into PASS.
                    let late: Vec<_> = r
                        .events
                        .iter()
                        .filter(|e| e.at_ms > impact + config.shutdown_ms)
                        .map(|e| e.kind.clone())
                        .collect();
                    if config.contract.cleanup_finished
                        && late.iter().any(|s| s == "cleanup_finished")
                    {
                        r.finding(
                            "CLEANUP_DEADLINE",
                            "cleanup finished after shutdown deadline",
                        );
                    }
                    evaluate(&mut r, obs.ready, obs.root_waited);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    })();
    if let Err(e) = result {
        r.outcome = Outcome::InfrastructureError;
        r.finding("INFRASTRUCTURE", e.to_string());
    }
    if r.shutdown_snapshot_at_ms.is_none() {
        if let Some(scope) = &resources.scope {
            match scope.members() {
                Ok(members) => {
                    r.survivors = members;
                    r.live_members_present = scope.populated().ok();
                    r.shutdown_snapshot_at_ms = Some(millis(start));
                }
                Err(e) => r.finding("OBSERVATION_ERROR", e.to_string()),
            }
        }
    }
    r.verdict_at_ms = millis(start);
    r.observed_outcome = Some(r.outcome);
    // Verdict/evidence freeze happens before any harness descendant signal.
    r.cleanup_started_at_ms = Some(millis(start));
    resources.cleanup(&mut r);
    r.cleanup_finished_at_ms = Some(millis(start));
    r
}
