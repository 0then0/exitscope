use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    ParentSigterm,
    GroupSigkill,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub signal_receipt: bool,
    pub cleanup_finished: bool,
    pub root_waits_cleanup: bool,
    pub no_survivors: bool,
    pub output_eof: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(skip_serializing)]
    pub executable: String,
    #[serde(skip_serializing)]
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub scenario: Scenario,
    /// fixture: readiness from the instrumented worker; exec: successful exec handshake.
    pub readiness: String,
    pub readiness_ms: u64,
    pub shutdown_ms: u64,
    pub output_ms: u64,
    pub contract: Contract,
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.executable.is_empty() || !self.cwd.is_absolute() {
            return Err("executable must be nonempty and cwd absolute".into());
        }
        if !["fixture", "exec"].contains(&self.readiness.as_str()) {
            return Err("readiness must be fixture or exec".into());
        }
        for ms in [self.readiness_ms, self.shutdown_ms, self.output_ms] {
            if ms == 0 || ms > 3_600_000 {
                return Err("deadlines must be 1..3600000 ms".into());
            }
        }
        if self.scenario == Scenario::GroupSigkill
            && (self.contract.signal_receipt
                || self.contract.cleanup_finished
                || self.contract.root_waits_cleanup)
        {
            return Err("SIGKILL cannot require graceful signal or cleanup events".into());
        }
        if self.contract.root_waits_cleanup && !self.contract.cleanup_finished {
            return Err("root_waits_cleanup requires cleanup_finished".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    Pass,
    Fail,
    Unresolved,
    InfrastructureError,
}
impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Unresolved => "UNRESOLVED",
            Self::InfrastructureError => "INFRASTRUCTURE_ERROR",
        }
    }
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Pass => 0,
            Self::Fail => 1,
            Self::Unresolved => 2,
            Self::InfrastructureError => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Identity {
    pub pid: i32,
    pub start_ticks: u64,
    pub ppid: i32,
    pub pgid: i32,
    pub sid: i32,
    pub state: String,
}
#[derive(Debug, Serialize)]
pub struct Event {
    pub at_ms: u64,
    pub pid: i32,
    pub kind: String,
    pub signal: Option<i32>,
}
#[derive(Debug, Serialize)]
pub struct Finding {
    pub id: String,
    pub detail: String,
}
#[derive(Debug, Default, Serialize)]
pub struct StreamState {
    pub eof_at_ms: Option<u64>,
    pub bytes: u64,
    /// Raw output is deliberately never retained.
    pub eof_before_deadline: bool,
}
#[derive(Debug, Serialize)]
pub struct RootExit {
    pub at_ms: u64,
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct LaunchError {
    pub stage: String,
    pub errno: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub outcome: Outcome,
    pub config: Option<Config>,
    pub environment: Vec<String>,
    pub cgroup: Option<PathBuf>,
    pub root: Option<Identity>,
    pub root_exit: Option<RootExit>,
    pub launch_error: Option<LaunchError>,
    pub cgroup_empty_at_ms: Option<u64>,
    pub events: Vec<Event>,
    pub survivors: Vec<Identity>,
    pub live_members_present: Option<bool>,
    pub stdout: StreamState,
    pub stderr: StreamState,
    pub impact_at_ms: Option<u64>,
    pub shutdown_snapshot_at_ms: Option<u64>,
    pub verdict_at_ms: u64,
    pub observed_outcome: Option<Outcome>,
    pub cleanup_started_at_ms: Option<u64>,
    pub cleanup_finished_at_ms: Option<u64>,
    pub findings: Vec<Finding>,
    pub cleanup: Vec<String>,
    pub limitations: Vec<String>,
}
impl Default for Report {
    fn default() -> Self {
        Self {
            schema_version: 1,
            outcome: Outcome::InfrastructureError,
            config: None,
            environment: vec![],
            cgroup: None,
            root: None,
            root_exit: None,
            launch_error: None,
            cgroup_empty_at_ms: None,
            events: vec![],
            survivors: vec![],
            live_members_present: None,
            stdout: StreamState::default(),
            stderr: StreamState::default(),
            impact_at_ms: None,
            shutdown_snapshot_at_ms: None,
            verdict_at_ms: 0,
            observed_outcome: None,
            cleanup_started_at_ms: None,
            cleanup_finished_at_ms: None,
            findings: vec![],
            cleanup: vec![],
            limitations: vec![
                "Not a security sandbox; cooperative targets must not migrate out of the cgroup"
                    .into(),
                "cgroup.procs excludes zombies; no zombie-reaping conformance claim".into(),
                "PID/group signals do not reproduce terminal Ctrl-C".into(),
                "Observation timestamps are monotonic receipt times, not kernel exit timestamps"
                    .into(),
            ],
        }
    }
}
impl Report {
    pub fn finding(&mut self, id: &str, detail: impl Into<String>) {
        self.findings.push(Finding {
            id: id.into(),
            detail: detail.into(),
        });
    }
    pub fn human(&self) -> String {
        let mut s = format!(
            "{} (exit {})\n",
            self.outcome.label(),
            self.outcome.exit_code()
        );
        s.push_str(&format!("cgroup: {:?}\nroot: {:?}\nlive members present: {:?}\nobserved outcome: {:?}\ncleanup interval: {:?}..{:?} ms\n", self.cgroup, self.root, self.live_members_present, self.observed_outcome, self.cleanup_started_at_ms, self.cleanup_finished_at_ms));
        if let Some(c) = &self.config {
            s.push_str(&format!(
                "scenario: {:?}\ncontract: {:?}\n",
                c.scenario, c.contract
            ));
        }
        s.push_str(&format!("root exit: {:?}\nsurvivors: {:?}\nstdout: {:?}\nstderr: {:?}\nimpact: {:?} ms; snapshot: {:?} ms; verdict: {} ms\n", self.root_exit, self.survivors, self.stdout, self.stderr, self.impact_at_ms, self.shutdown_snapshot_at_ms, self.verdict_at_ms));
        s.push_str(&format!(
            "launch error: {:?}\ncgroup empty observed: {:?} ms\n",
            self.launch_error, self.cgroup_empty_at_ms
        ));
        for f in &self.findings {
            s.push_str(&format!("{}: {}\n", f.id, f.detail));
        }
        for e in &self.events {
            s.push_str(&format!(
                "event {} ms pid {}: {} signal={:?}\n",
                e.at_ms, e.pid, e.kind, e.signal
            ));
        }
        for e in &self.environment {
            s.push_str(&format!("environment: {e}\n"));
        }
        for a in &self.cleanup {
            s.push_str(&format!("cleanup: {a}\n"));
        }
        for l in &self.limitations {
            s.push_str(&format!("limitation: {l}\n"));
        }
        s
    }
}

/// Pure verdict evaluation, independent of forced cleanup.
pub fn evaluate(r: &mut Report, telemetry: bool, root_waited: Option<bool>) {
    let Some(c) = r.config.clone() else {
        return;
    };
    let seen = |kind: &str| r.events.iter().any(|e| e.kind == kind);
    let receipt = r
        .events
        .iter()
        .any(|e| e.kind == "signal_received" && e.signal == Some(15));
    let wrong_signal = seen("signal_received") && !receipt;
    let finished = seen("cleanup_finished");
    if r.root_exit
        .as_ref()
        .is_none_or(|e| e.at_ms > r.impact_at_ms.unwrap_or(0) + c.shutdown_ms)
    {
        r.finding(
            "ROOT_DEADLINE",
            "root did not exit before shutdown deadline",
        );
    }
    if c.contract.no_survivors && (!r.survivors.is_empty() || r.live_members_present == Some(true))
    {
        r.finding("LIVE_MEMBERS", "live cgroup members at shutdown deadline");
    }
    if c.contract.output_eof && !r.stdout.eof_before_deadline {
        r.finding(
            "STDOUT_OPEN",
            "stdout did not reach EOF before output deadline",
        );
    }
    if c.contract.output_eof && !r.stderr.eof_before_deadline {
        r.finding(
            "STDERR_OPEN",
            "stderr did not reach EOF before output deadline",
        );
    }
    if telemetry {
        if c.contract.signal_receipt && !receipt {
            if wrong_signal {
                r.finding(
                    "WRONG_SIGNAL",
                    "worker received a signal other than SIGTERM",
                );
            } else {
                r.finding(
                    "SIGNAL_NOT_RECEIVED",
                    "instrumented worker did not report SIGTERM",
                );
            }
        }
        if c.contract.cleanup_finished && !finished {
            r.finding("CLEANUP_INCOMPLETE", "worker did not complete cleanup");
        }
        if c.contract.root_waits_cleanup && root_waited == Some(false) {
            r.finding(
                "ROOT_DID_NOT_WAIT",
                "root was already exited before cleanup completion handshake",
            );
        }
    }
    let incomplete_evidence = !telemetry
        && (c.contract.signal_receipt
            || c.contract.cleanup_finished
            || c.contract.root_waits_cleanup);
    let unobserved_empty = c.contract.no_survivors
        && r.cgroup_empty_at_ms
            .is_none_or(|t| t > r.impact_at_ms.unwrap_or(0) + c.shutdown_ms);
    r.outcome = if !r.findings.is_empty() {
        Outcome::Fail
    } else if unobserved_empty {
        r.finding(
            "CGROUP_DEADLINE_UNOBSERVED",
            "cgroup emptiness was not observed before shutdown deadline",
        );
        Outcome::Unresolved
    } else if incomplete_evidence {
        r.finding(
            "TELEMETRY_MISSING",
            "required worker evidence is unavailable",
        );
        Outcome::Unresolved
    } else {
        Outcome::Pass
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> Report {
        Report {
            config: Some(Config {
                executable: "x".into(),
                args: vec![],
                cwd: "/tmp".into(),
                scenario: Scenario::ParentSigterm,
                readiness: "exec".into(),
                readiness_ms: 10,
                shutdown_ms: 10,
                output_ms: 10,
                contract: Contract {
                    signal_receipt: true,
                    cleanup_finished: true,
                    root_waits_cleanup: true,
                    no_survivors: true,
                    output_eof: true,
                },
            }),
            root_exit: Some(RootExit {
                at_ms: 5,
                code: Some(0),
                signal: None,
            }),
            live_members_present: Some(false),
            cgroup_empty_at_ms: Some(5),
            stdout: StreamState {
                eof_before_deadline: true,
                ..Default::default()
            },
            stderr: StreamState {
                eof_before_deadline: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    #[test]
    fn late_empty_snapshot_is_unresolved() {
        let mut r = report();
        r.config.as_mut().unwrap().contract.signal_receipt = false;
        r.config.as_mut().unwrap().contract.cleanup_finished = false;
        r.config.as_mut().unwrap().contract.root_waits_cleanup = false;
        r.cgroup_empty_at_ms = Some(11);
        evaluate(&mut r, false, None);
        assert_eq!(r.outcome, Outcome::Unresolved);
        assert!(
            r.findings
                .iter()
                .any(|f| f.id == "CGROUP_DEADLINE_UNOBSERVED")
        );
    }
    #[test]
    fn missing_wait_evidence_does_not_claim_early_root_exit() {
        let mut r = report();
        r.root_exit = None;
        evaluate(&mut r, true, None);
        assert_eq!(r.outcome, Outcome::Fail);
        assert!(r.findings.iter().any(|f| f.id == "CLEANUP_INCOMPLETE"));
        assert!(!r.findings.iter().any(|f| f.id == "ROOT_DID_NOT_WAIT"));
    }
    #[test]
    fn absent_telemetry_is_unresolved() {
        let mut r = report();
        evaluate(&mut r, false, None);
        assert_eq!(r.outcome, Outcome::Unresolved);
    }
    #[test]
    fn cleanup_cannot_erase_violation() {
        let mut r = report();
        r.stdout.eof_before_deadline = false;
        evaluate(&mut r, false, None);
        r.cleanup.push("killed everything".into());
        assert_eq!(r.outcome, Outcome::Fail);
    }
    #[test]
    fn signal_and_wait_are_required() {
        let mut r = report();
        evaluate(&mut r, true, None);
        assert_eq!(r.outcome, Outcome::Fail);
        assert!(r.findings.iter().any(|f| f.id == "SIGNAL_NOT_RECEIVED"));
    }
    #[test]
    fn verified_contract_passes() {
        let mut r = report();
        for kind in ["signal_received", "cleanup_finished"] {
            r.events.push(Event {
                at_ms: 2,
                pid: 7,
                kind: kind.into(),
                signal: if kind == "signal_received" {
                    Some(15)
                } else {
                    None
                },
            });
        }
        evaluate(&mut r, true, Some(true));
        assert_eq!(r.outcome, Outcome::Pass);
    }
}
