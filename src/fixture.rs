use crate::linux::{self, Result};
use nix::sys::signal::SigSet;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

/// Each event is acknowledged; cleanup_finished leaves the worker alive until
/// the harness has independently checked root liveness, making the wait witness reliable.
fn event(socket: &mut UnixStream, kind: &str, signal: Option<i32>) -> Result<()> {
    let mut frame = serde_json::json!({"kind":kind});
    if let Some(signal) = signal {
        frame["signal"] = signal.into();
    }
    writeln!(socket, "{frame}")?;
    let mut line = String::new();
    BufReader::new(socket).read_line(&mut line)?;
    if line != "ack\n" {
        return Err("fixture acknowledgement missing".into());
    }
    Ok(())
}
pub fn run(cleanup_ms: u64, behavior: &str) -> Result<()> {
    if cleanup_ms > 3_600_000
        || ![
            "normal",
            "premature",
            "no_ready",
            "silent",
            "new_session",
            "new_group",
            "close_output",
            "flood",
        ]
        .contains(&behavior)
    {
        return Err("invalid fixture options".into());
    }
    SigSet::empty().thread_set_mask()?;
    if behavior == "new_session" {
        nix::unistd::setsid()?;
    }
    if behavior == "new_group" {
        nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))?;
    }
    let signals = linux::signals()?;
    let mut socket = UnixStream::connect(std::env::var("EXIT_SCOPE_SOCKET")?)?;
    socket.set_read_timeout(Some(Duration::from_secs(10)))?;
    socket.set_write_timeout(Some(Duration::from_secs(10)))?;
    if behavior != "silent" {
        event(&mut socket, "handler_installed", None)?;
        if behavior != "no_ready" {
            event(&mut socket, "ready", None)?;
        }
    }
    if behavior == "close_output" {
        nix::unistd::close(1)?;
        nix::unistd::close(2)?;
    }
    let received = loop {
        if let Some(info) = signals.read_signal()? {
            break info.ssi_signo as i32;
        }
        if behavior == "flood" {
            let _ = std::io::stdout().write_all(&[b'x'; 4096]);
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    if behavior == "silent" {
        return Ok(());
    }
    event(&mut socket, "signal_received", Some(received))?;
    event(&mut socket, "cleanup_started", None)?;
    if behavior == "premature" {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_millis(cleanup_ms);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    event(&mut socket, "cleanup_finished", None)?;
    Ok(())
}
