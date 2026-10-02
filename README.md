<p align="center">
  <img src="assets/exitscope.svg" width="128" height="128" alt="ExitScope: root exit, a surviving subprocess, and an open output pipe">
</p>

# ExitScope

[![Linux checks](https://img.shields.io/github/actions/workflow/status/0then0/exitscope/linux.yml?branch=main&label=Linux%20checks)](https://github.com/0then0/exitscope/actions/workflows/linux.yml)
[![License: Apache-2.0](https://img.shields.io/github/license/0then0/exitscope)](LICENSE)
[![Rust: 1.85+](https://img.shields.io/badge/Rust-1.85%2B-555555?logo=rust)](#requirements-and-installation)
[![Linux: 5.14+](https://img.shields.io/badge/Linux-5.14%2B-555555?logo=linux)](#requirements-and-installation)

Run a command, exercise its shutdown contract, and detect unfinished cleanup,
surviving subprocesses, and output pipes that never close.

ExitScope 0.1 is a Linux CLI for testing wrappers, task runners and launchers.
It sends either SIGTERM to the wrapper PID alone or SIGKILL to its dedicated
process group. It observes root exit, worker events, live cgroup members and
stdout/stderr EOF independently. A successful harness cleanup never changes
a failed shutdown check into PASS.

## Requirements and installation

- Linux kernel **5.14+**, unified cgroup v2, mounted procfs exposing the run's PIDs.
  `cgroup.kill` requires 5.14; pidfds require 5.3. Seccomp/LSM must permit those
  syscalls, `setsid`, signals and migration into the test cgroup.
- An **already delegated, writable subtree**. ExitScope creates only a fresh
  child within the path passed in `--cgroup-parent`; it does not mount cgroupfs,
  configure systemd, enable controllers, or elevate privileges.
- Rust stable, **minimum 1.85.0**, Cargo. Python 3.11+ is needed only for the
  integration and upstream validation scripts. No Python packages are required.

```sh
cargo install --path . --locked
exitscope --help
exitscope run --config examples/parent-sigterm.json \
  --cgroup-parent /path/to/delegated/runs
exitscope run --config examples/group-sigkill.json \
  --cgroup-parent /path/to/delegated/runs --json
```

Runtime support is Linux only. A macOS/Windows build reports the limitation;
it is not a substitute for Linux validation. No claim of rootless operation
on every Linux host is made.

## Delegation and explicit setup

Have your administrator or service manager delegate a private subtree with
permission to create directories and write `cgroup.procs` in the destination
and the common ancestor of source/destination. The new child's `cgroup.kill`
must be writable. Merely making a directory writable is insufficient.
The runner must stay outside its test child. Keep other users' processes out
of the delegated subtree, and do not share run directories between concurrent
tests. See [kernel delegation rules](https://docs.kernel.org/admin-guide/cgroup-v2.html#delegation).

One possible setup, when the **user systemd manager supports delegation**, is
an explicit delegated service shell:

```sh
systemd-run --user --pty --collect --property=Delegate=yes bash
# Inside that shell:
parent="/sys/fs/cgroup$(awk -F: '$1 == "0" { print $3 }' /proc/self/cgroup)"
mkdir "$parent/harness" "$parent/runs"
printf '%s\n' "$$" > "$parent/harness/cgroup.procs"
exitscope run --config /absolute/path/to/profile.json \
  --cgroup-parent "$parent/runs" --json
```

This is a setup example, not an automatic fallback; systemd versions and
administrator policy differ. Moving the shell into `harness` avoids keeping
the runner in a parent that may enable resource controllers. Verify delegation
locally. Do not recursively chown the global cgroup hierarchy or make it
world-writable. The tested environment and exact disposable Docker setup are
recorded in [the engineering report](docs/engineering-report.md).

## Configuration and workload injection

JSON is the only configuration format. Unknown fields are rejected. Start from
the profiles in `examples/`. Paths in `cwd` must be absolute. `executable` and
each `args` element are passed without shell interpolation; a shell is used
only if explicitly configured. `{fixture}` in either field expands to the
absolute ExitScope executable. The wrapper receives `EXIT_SCOPE_BIN` and
`EXIT_SCOPE_SOCKET`; other environment variables are inherited, never reported.

Invoke `"$EXIT_SCOPE_BIN" fixture --cleanup-ms 200` through the wrapper's normal
workload mechanism. For pnpm this is a package.json script, for uv it is the
command after `uv run`. Adapter-specific setup and historical flags live in
`scripts/external.py`; containment, observations, verdict and cleanup live in
the common Rust engine.

`readiness: "fixture"` requires an acknowledged `handler_installed` followed by
`ready` event. The worker has installed a blocked-signal/signalfd handler before
these events. `readiness: "exec"` means only successful `exec` of the command,
**not application readiness**. Use it for uninstrumented commands only when
that is the readiness contract you intend. It cannot establish correct signal
receipt or cleanup. An otherwise successful run with required graceful evidence
but no ready worker is UNRESOLVED.

`readiness_ms` runs from harness start; `shutdown_ms` and `output_ms` run from
the test impact. All are monotonic, bounded to 1..3600000 ms. EOF and shutdown
have separate deadlines, both measured from impact. v0.1 always requires root
exit by `shutdown_ms`. Contract booleans additionally select signal receipt,
finished cleanup, root waiting for cleanup, no live members, and both stream
EOFs. Setting `no_survivors: false` permits intentional background processes
in the observation verdict; disposable run members are still killed afterward.
SIGKILL profiles cannot require graceful events.

## Observation and telemetry

The runner launches a fresh copy of itself as a gated launcher. It enters the
new cgroup **before executing any wrapper code**, calls `setsid`, authenticates
to the harness over a private Unix stream socket, and waits for ACK. The diagnostic
connection is opened before cgroup entry so failed delegation can report its errno. The harness
checks cgroup membership, session/group identity and opens a root pidfd before
releasing the gate. CLOEXEC closure of the gate indicates successful exec;
exec failures are infrastructure errors. stdin is `/dev/null`.

For SIGTERM, only the root pidfd receives the test signal. The harness never
forwards SIGTERM to descendants. For SIGKILL, the test signal goes to the root's
dedicated group. The still-unreaped root anchors its PGID against PID reuse at
impact; a private session prevents cooperative unrelated processes from joining.
Process identities contain PID, `/proc` start ticks, PPID, PGID, SID and state.
Membership comes from cgroup v2, including nested cgroups, rather than PPID or
command matching. Survivor identities are checked with pidfds and membership
again to reject exit/reuse races.

The control socket is independent of output pipes. SO_PEERCRED authenticates
peer PIDs and pidfds pin them; only live run members are accepted. One
instrumented worker is supported. Frames are newline-delimited JSON of exactly
`{"kind":"EVENT"}`, each followed by harness `ack\n`. `signal_received` additionally
requires a numeric `signal` field, for example `{"kind":"signal_received","signal":15}`:

1. `handler_installed`
2. `ready`
3. `signal_received` (the controlled workload's SIGTERM)
4. `cleanup_started`
5. `cleanup_finished`

Each frame must wait for its ACK. In particular, the worker stays alive after
cleanup until the final ACK. The harness checks root pidfd liveness before that
ACK. `root_waits_cleanup` is therefore a strict witnessed contract: root must
still be alive at the acknowledged completion boundary. A wrapper intentionally
exiting immediately when it independently sees cleanup completion can fail this
stricter profile; disable that condition if it is not your contract. External
workloads implementing this protocol are trusted instrumentation, not adversaries.

Nonblocking reads and bounded draining keep pipes, control messages and process
observation independent. Read ends and accepted control connections stay open
through verdict recording and forced cleanup. A 2 ms bounded wait reduces CPU
use; it is never used to prove readiness. Observation receipt timestamps have
scheduler/polling uncertainty. A kernel event near a deadline may be observed
after it and fail a strict deadline contract. Cgroup emptiness is sampled throughout
shutdown; `cgroup_empty_at_ms` records its first observation. A late empty snapshot
without emptiness witnessed by the deadline yields UNRESOLVED, never PASS.
Queued events from an already authenticated worker remain evidence after its death;
interrupted cleanup is a target failure, not an infrastructure failure.

## Outcomes and report schema

- `PASS`, exit **0**: all required conditions have evidence.
- `FAIL`, exit **1**: an observed contract violation. Missing required graceful
  events after confirmed fixture readiness is a violation.
- `UNRESOLVED`, exit **2**: no observed violation, but required evidence is absent.
- `INFRASTRUCTURE_ERROR`, exit **3**: invalid config, containment/exec/IPC/observation
  failure, readiness failure, runner interruption, or failed harness cleanup.
  clap syntax errors use its standard exit **2**, with no run report.

Human output is default; `--json` emits schema version 1. Reports include the
scenario and contract, capabilities observed during setup, root identity and
exit disposition (`code` versus `signal`), event timestamps, survivors at the
shutdown snapshot (with its actual timestamp), first observed cgroup emptiness,
stream byte counts and EOF times, impact/snapshot/verdict
times, stable findings, cleanup actions and limitations. `observed_outcome`
is frozen before cleanup. `outcome` ordinarily matches it but becomes
INFRASTRUCTURE_ERROR if forced cleanup fails. Cleanup does not rewrite event,
survivor, EOF or root-exit evidence. Root exits during forced cleanup are not
presented as target shutdown success.

Stable finding IDs: `ROOT_DEADLINE`, `LIVE_MEMBERS`, `STDOUT_OPEN`, `STDERR_OPEN`,
`SIGNAL_NOT_RECEIVED`, `WRONG_SIGNAL`, `CLEANUP_INCOMPLETE`, `ROOT_DID_NOT_WAIT`, `CLEANUP_DEADLINE`,
`TELEMETRY_MISSING`, `CGROUP_DEADLINE_UNOBSERVED`, `READINESS_TIMEOUT`, `RUNNER_INTERRUPTED`, `INFRASTRUCTURE`,
`OBSERVATION_ERROR`, `CLEANUP_ERROR`. IDs may have more detailed text per run.

`launch_error` is null or an object containing a bounded stage name and numeric
errno. Stages distinguish signal mask, cgroup entry, session creation, working
directory and exec failures; neither command arguments nor raw error output is
stored. `ROOT_DID_NOT_WAIT` requires a witnessed failed completion handshake,
rather than absence of a completed handshake. Late cleanup affects the verdict
only when `cleanup_finished` is required.

Configuration input is limited to 64 KiB, IPC to 16 peers, 64 events, and 4 KiB
per frame buffer. Cgroup traversal allows 256 directories and 4096 PIDs.
Raw stdout/stderr is **not retained**, only bounded counters; flooding output
is drained without unlimited memory. argv and executable are omitted from
reports to avoid recording command-line credentials. Reports never contain a
full environment. Paths and known controlled fixture events are still visible.

## Cleanup and limits

After observations and verdict freeze, an already-open descriptor to this
run's `cgroup.kill` kills only members of that cgroup and its descendants,
regardless of their process groups, sessions or reparenting. The harness waits
up to two seconds for `populated 0`, removes nested run directories, reaps its
own direct child with a bounded wait, and removes private IPC files. There is
no recursive PID kill fallback. A not-yet-released owned launcher can be killed
by its pidfd if setup fails before it joins the cgroup.

SIGINT/SIGTERM directed at ExitScope are caught through signalfd and yield an
infrastructure report followed by the same cleanup. Intermediate errors use
the same path; RAII also attempts cleanup on unwinding. Repeated interruption
does not abort forced cleanup. **SIGKILL, process abort, host failure, persistent
uninterruptible kernel tasks, lost permissions or removed cgroupfs can defeat
cleanup**. The printed run cgroup path identifies residual resources for the
operator. No detached privileged watchdog is installed.

ExitScope is not a security sandbox. The target must not deliberately migrate
out, forge telemetry, manipulate cgroup directories, move to the runner's
session, or kill the observer. Migrated-out target PIDs are not blindly signaled.
Run trusted workloads in a private namespace/subtree. `cgroup.procs` excludes
zombies: an empty live set is not proof of correct reaping. There are no PTY,
terminal Ctrl-C, Windows/macOS runtime, HTTP draining, production supervision,
eBPF tracing, arbitrary fault injection, or universal shutdown guarantees.

## Validation

```sh
EXITSCOPE_CGROUP_PARENT=/path/to/delegated/runs sh scripts/linux-checks.sh
python3 scripts/external.py --cgroup-parent /path/to/delegated/runs
```

The external batch downloads SHA256-pinned upstream archives and uses disposable
projects/caches. It currently pins Linux ARM64 assets. `--discover` is an explicit
maintainer bootstrap that changes the source manifest; normal validation verifies
it. Each batch writes a new directory below `validation/`; unexpected and unresolved
outcomes are saved before checking expectations. CI uses the same engine on Linux
ARM64, preserves artifacts even on failure, and checks Rust's minimum version.

Own results, upstream reports, reproduction details and remaining uncertainty
are distinguished in [docs/engineering-report.md](docs/engineering-report.md).
