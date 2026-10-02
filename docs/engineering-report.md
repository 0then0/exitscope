# ExitScope 0.1 engineering report

Date: 2026-10-02. All runtime results below are **our own Linux executions**,
unless explicitly labelled upstream evidence or engineering inference.

## Implemented behavior and architecture

Both scoped scenarios are implemented: parent-only SIGTERM and dedicated-group
SIGKILL. The implementation has five modules: `main.rs` (CLI), `model.rs`
(configuration/report/verdict), `linux.rs` (cgroup and process primitives),
`fixture.rs` (acknowledged instrumented workload) and `runner.rs`
(launch/observation/cleanup orchestration). There are no runner-specific engines.

Containment is established in a gated internal launcher before target exec.
The harness stays outside the run cgroup and pins root identity with a pidfd.
Private-session PGID anchoring makes the group test independent of caller
processes. Worker peer credentials, cgroup membership and pidfds authenticate
telemetry. Nonblocking stream reads, explicit ACKs and monotonic deadlines avoid
`wait_with_output` and fixed-sleep readiness assumptions. At the shutdown deadline,
both enumerated identities and hierarchical `populated` state are captured.
EOF remains an independent observation. Evidence and `observed_outcome` freeze
before forced cleanup. `cgroup.kill` includes reparented, regrouped, re-sessioned
and nested-cgroup members. Cleanup failure is separately reflected in the final
operational outcome.

PASS requires every enabled condition. Missing graceful evidence without a
ready worker yields UNRESOLVED, unless an independent observed violation already
establishes FAIL. Missing events after confirmed worker readiness are FAIL.
Readiness/setup/exec/protocol failures and runner interruption are infrastructure
errors. Deliberate background processes can be allowed by disabling the relevant
contract conditions; all run resources remain disposable afterward.

The cleanup-wait witness deliberately uses an ACK boundary: the worker remains
alive after cleanup completion while root liveness is checked. This is stronger
than inference from stdout ordering and a stricter contract than an unacknowledged
"completed" print. One instrumented worker is supported in v0.1.

## Stack and requirements

Cargo resolves and locks actual published dependencies: clap **4.6.7**, serde
**1.0.229**, serde_json **1.0.151**, nix **0.31.3**. `Cargo.lock` is included in
the repository changes. Rust **1.99.0** stable was used for execution and checks;
Rust **1.85.0** successfully compiled the same locked graph.

nix was selected for signalfd/SigSet, signals, socket credentials, poll, fcntl,
setsid/setpgid and statfs. Its API has no pidfd module in the selected version,
so two small documented wrappers call pidfd syscalls through nix's libc re-export
and return owned descriptors. No second Unix primitives library, Tokio, database,
web service or plugin framework was added. A single-thread observer with bounded
nonblocking reads is sufficient for this narrow workload.

Runtime requires Linux **5.14+**, cgroup v2, accessible procfs, permitted pidfd
syscalls/signals, and a writable delegated subtree including migration permissions
at the common ancestor. Minimum-kernel operation was not tested; 5.14 is derived
from the documented availability of `cgroup.kill`. This is not a claim of rootless
support on every Linux machine. Python 3.11+ and network access are required for
the optional external validation scripts; they install no Python packages.

See [README](../README.md) for installation, delegation, JSON semantics,
finding IDs, outcomes and the telemetry protocol. `cargo install --path . --locked
--root /tmp/exitscope-install` succeeded. The installed optimized binary produced
[help](../validation/installed-help.txt), a [human PASS report](../validation/installed-human.txt)
and the final external results below.

## Feasibility checks and execution environment

The development host is macOS ARM64, with no Rust initially in PATH. It was not
used as runtime validation. Docker Desktop provided Linux **7.0.14-linuxkit**,
ARM64 and cgroup v2. An ordinary alpine container exposed cgroupfs read-only;
creating a run child failed with EROFS. A separate disposable container with
**explicit `--privileged --cgroupns=private`** could create/remove its own child
and write `cgroup.kill`. No host cgroup bind mount, systemd change, security-policy
change or host mount operation was performed.

The Rust image actually used was:
`rust@sha256:5d05167b28cef0fa3a6c781cd77949386848191f3382e82cf53bd1277a47a98f`.
Rustfmt/clippy and the minimum toolchain were installed only inside that
disposable development container. The GitHub workflow uses an explicit Linux
ARM64 container with a private cgroup namespace and preserves reports on failure.
The hosted GitHub workflow itself has not been executed in this task.

UID 65534 was tested independently using `tests/delegation.py`. Without write
permission to the common ancestor's `cgroup.procs`, launch was
INFRASTRUCTURE_ERROR and its empty run child was removed. With permissions
delegated only within the disposable test subtree, the same user obtained PASS.
This is a verified configured-rootless case. The README's systemd setup example
was not executed here and remains dependent on local service-manager policy.

## Checks actually run

- `cargo fmt --check`: passed after formatting.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: **6 passed**, including real Linux pidfd lifecycle tests.
- `cargo build --locked`: passed.
- `cargo +1.85.0 check --locked`: passed on Linux ARM64.
- `tests/integration.py`: final batch **19 passed**, with no skips. The read-only
  cgroup check used an explicit read-only bind mount inside the container.
- `tests/delegation.py`: permission-denied outcome and delegated-user PASS verified.
- `scripts/external.py`: both runner broken/fixed pairs matched expectations with
  the installed release binary.
- `cargo install --path . --locked --root /tmp/exitscope-install`: passed.

[Tool output](../validation/checks.txt) and
[final integration reports](../validation/synthetic/1790931514630446055/) are
preserved. The suite covers forwarding and cleanup, missing forwarding, premature
worker exit, premature root exit, other-group/session survivors, open pipes after
root exit, survivors with closed pipes, readiness failure, missing telemetry,
invalid/read-only cgroups, exec errors, interruption, unrelated-process safety,
output flooding, wrong-signal forwarding, late cleanup and explicitly allowed background processes.
Every run asserts its exit-code mapping and removal of its run directory; final
reports assert verdict-before-cleanup timing.

Earlier reports are retained, including expected FAIL/UNRESOLVED outcomes. The
first delegation test assertion incorrectly counted cgroup pseudo-files as
remaining run directories; its report already showed successful cleanup. That
assertion was corrected to inspect directories and both delegation checks then
passed. Initial missing rustfmt and formatting failures were development checks,
not discarded target shutdown outcomes.

## Upstream evidence and pinned identities

The following are **upstream reports/source evidence**, separately from our runs:

- [pnpm issue #15555](https://github.com/pnpm/pnpm/issues/15555) reports 12.6.0 as
  broken and 12.5.1 as the prior working version. Detached group termination left
  the script and inherited pipes alive.
- [pnpm PR #15119](https://github.com/pnpm/pnpm/pull/15119), merge commit
  `2af480acf4f44a82b223c50f2b36f53879af312a`, introduced script groups outside
  terminal use. API ancestry confirms it is included in v12.6.0.
- [pnpm PR #15564](https://github.com/pnpm/pnpm/pull/15564), merge commit
  `4abfa5a66cd8c0157cfe1344adeb710540f20e17`, added a pipe-based group watchdog.
  API ancestry and [v12.7.0 release notes](https://github.com/pnpm/pnpm/releases/tag/v12.7.0)
  confirm that the tested fixed release includes it. No guessed patch release
  number was used; 12.7.0 is a verified fixed release, not a claim about the first
  possible fixed build.
- [uv issue #6724](https://github.com/astral-sh/uv/issues/6724) concerns missing
  forwarding in a Docker Compose project command, reported on 0.3.3.
- [uv PR #8933](https://github.com/astral-sh/uv/pull/8933), merge commit
  `052b4e77a60847de0ccd275f4c7fc241378c9bc0`, added Unix SIGTERM handling to
  project/tool run paths. API ancestry places 0.5.1 before that commit and 0.5.2
  after it. [The uv changelog](https://github.com/astral-sh/uv/blob/main/changelogs/0.5.x.md)
  lists this fix in 0.5.2.

Immutable tag commits used in our comparisons:

- pnpm 12.6.0: `a959dceda501e552dbb03bbab19ab5e060718fde`.
- pnpm 12.7.0: `b341f5ada21dc59d7d274889a83ce1bdaf7b4351`.
- uv 0.5.1: `f399a52719a032ee41975c3eb3fdd1631b3d10cc`.
- uv 0.5.2: `195f4b634ff0230fcef5445c6023a74faab92184`.
- Initial uv 0.3.3 attempt: `deea6025a1afd2ca28f8f56afbec0a808636285e`.

Archive URLs, architectures, SHA256 hashes and commits are stored in
[upstream-sources.json](../validation/upstream-sources.json).
[upstream-evidence.json](../validation/upstream-evidence.json) stores compact
API ancestry evidence; each external run also has source metadata and observed
`--version`. Normal runs verify the pinned download hash, without refreshing tags.

## Our external outcomes

The final installed-binary batch is
[validation/external/1790931571146498886](../validation/external/1790931571146498886/).

- **pnpm 12.6.0, group_sigkill: FAIL.** Root terminated by signal 9. Two live
  members remained in a script group different from root's group, in the same
  session; stdout and stderr had no EOF. Findings: LIVE_MEMBERS, STDOUT_OPEN,
  STDERR_OPEN. Forced cgroup cleanup subsequently emptied and removed the run.
- **pnpm 12.7.0, group_sigkill: PASS.** Root terminated by signal 9, no live members,
  both streams reached EOF before their deadline. Harness cleanup remained separate.
- **uv 0.5.1, parent_sigterm: FAIL.** Root terminated by signal 15. The acknowledged
  ready worker remained alive, reported no forwarded signal/cleanup, and held both
  streams open. Findings additionally include SIGNAL_NOT_RECEIVED,
  CLEANUP_INCOMPLETE and ROOT_DID_NOT_WAIT.
- **uv 0.5.2, parent_sigterm: PASS.** Worker reported signal receipt, cleanup start
  and completion, root was alive at the final ACK boundary and exited normally
  with code 0 afterward, no live members remained, and both streams reached EOF.

These are real released upstream binaries, not synthetic replacements for the
runners. The controlled workload replaces pnpm's HTTP server and uv's FastAPI
application, keeping the relevant shell/group topology and project-run signal
contract. No Playwright, Docker Compose Ctrl-C emulation or FastAPI HTTP draining
was tested. The pnpm result matches the reported separated-script-group and open
pipe failure mechanism; the uv result verifies the narrower parent-only SIGTERM
forwarding and wait behavior fixed in project/run.rs.

The original **uv 0.3.3 attempt was INFRASTRUCTURE_ERROR**, preserved in
[its initial report](../validation/external/uv-0.3.3-arm64.json). That release
implicitly tried to build the local dependency-free project using setuptools,
which was unavailable in the offline cache. It never reached readiness and no
shutdown impact occurred. Instead of adding package dependencies, the broken
comparison uses 0.5.1 with its supported `--no-sync`; both uv runs still traverse
the project runner. This is a setup mismatch, not evidence of forwarding in 0.3.3.

## Exact reproduction

On a Linux host with an existing delegated subtree:

```sh
cargo install --path . --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
EXITSCOPE_CGROUP_PARENT=/path/to/delegated/runs sh scripts/linux-checks.sh
python3 scripts/external.py --binary target/debug/exitscope \
  --cgroup-parent /path/to/delegated/runs
```

The external batch currently pins ARM64 release assets. To repeat the tested
Linux namespace approach on Docker Desktop or a disposable Linux CI worker:

```sh
docker run --rm --init --privileged --cgroupns=private \
  -v "$PWD:/work" -w /work \
  rust@sha256:5d05167b28cef0fa3a6c781cd77949386848191f3382e82cf53bd1277a47a98f \
  sh -c '
    set -eu
    rustup component add rustfmt clippy
    mkdir /sys/fs/cgroup/exitscope-tests /tmp/exitscope-ro
    mount --bind /sys/fs/cgroup/exitscope-tests /tmp/exitscope-ro
    mount -o remount,bind,ro /tmp/exitscope-ro
    EXITSCOPE_CGROUP_PARENT=/sys/fs/cgroup/exitscope-tests \
    EXITSCOPE_READONLY_CGROUP=/tmp/exitscope-ro sh scripts/linux-checks.sh
    python3 tests/delegation.py --cgroup-parent /sys/fs/cgroup/exitscope-tests
    python3 scripts/external.py --cgroup-parent /sys/fs/cgroup/exitscope-tests
  '
```

The mount and UID setup are explicit test setup within the private container;
the production CLI never performs them. Reproduction requires pulling the image
and upstream archives. Run only against disposable resources. Neither Git state
nor remote repositories/maintainers were modified.

## Guarantee boundaries and product-gap assessment

Verified: the same observer/verdict/cleanup implementation diagnoses two unrelated
runners, distinguishes broken/fixed releases, and detects survivors/open pipes
after root exit. Delegation can be limited to one subtree and user identity. The
adapters have no independent observer or signal engine. This supports the intended
**narrow reusable testing gap** and shows that the model works with practical local
CI setup.

Engineering inference: requiring a delegated cgroup is a material adoption cost,
but the tested setup is small enough for disposable CI and managed Linux hosts.
The evidence does not establish market novelty, universal compatibility or a
guarantee for arbitrary launchers. No survey of all competing tools was performed.

Unverified: kernel 5.14 behavior, x86_64 upstream binaries, the hosted GitHub Actions
run, the systemd user-manager recipe, adversarial targets, actual SIGKILL/host-failure
recovery, persistent D-state tasks, full zombie reaping and terminal Ctrl-C semantics.
SIGKILL of the harness cannot run its RAII cleanup; cgroup paths must be handled by
an operator or enclosing disposable resource owner after such failures. Targets
must not migrate out or modify the observation infrastructure. These limits are
part of the public contract, not replaced by weaker process-tree heuristics.

## Post-review corrections, 2026-10-02

The original validation batches above remain historical evidence, not validation of
these corrections. The review reproduced a false PASS after a delayed cgroup
snapshot and an infrastructure misclassification when the wrapper killed a worker
with queued telemetry. Both are corrected: first witnessed cgroup emptiness must
precede the shutdown deadline to certify no survivors; an authenticated worker's
death preserves queued events and remains a target contract failure. A late empty
snapshot without timely evidence yields UNRESOLVED.

Disabled cleanup requirements no longer cause CLEANUP_DEADLINE. Root waiting is
represented internally as unknown/satisfied/violated, so an unfinished handshake
cannot assert that a live root exited early. Launcher setup errors now retain a
bounded stage and errno, including delegation denial, working-directory failure,
and executable ENOENT/EACCES. Raw argv, environment and output remain excluded.

The corrected source passed 8 Rust tests, 23 Linux integration checks, 3 dedicated
observer regressions, both UID 65534 delegation checks, formatting and clippy.
Pinned pnpm 12.6.0/12.7.0 and uv 0.5.1/0.5.2 again produced FAIL/PASS respectively.
Rust 1.85.0 cargo check --locked passed independently. Ruff 0.16.10 confirmed the
requested C408/PLW1510 rules for the Python validation scripts. These tooling checks
introduce no runtime dependencies. The new observer suite runs in linux-checks.sh
and therefore in the existing GitHub workflow; hosted Actions was not executed
locally. Minimum-kernel and x86_64 validation remain unperformed in this batch.

[Exact commands and check results](../validation/review-fixes/checks.txt), and
[all run outcomes](../validation/review-fixes/) include the unsuccessful first
observer-test attempt. That attempt resumed after cgroup.procs emptied but before
cgroup.events populated became zero; its FAIL was retained, and the test now waits
for the latter condition explicitly. No engine behavior was weakened to satisfy
that assertion. Controlled waits occur only after IPC readiness.

Reproduce the new race checks with:

```sh
python3 tests/observer_regressions.py --cgroup-parent /path/to/delegated/runs
```
