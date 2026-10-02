# ExitScope 0.1 engineering report

Date: 2026-10-02. All runtime results below are **our own Linux executions**,
unless explicitly labelled upstream evidence or engineering inference.

## Hosted acceptance and review follow-up, 2026-10-02

**v0.1.1 acceptance is complete: ready for release and maintenance.** The initial
local preparation and its unavailable environments remain below as historical
evidence. Later authorized pushes made real hosted Linux execution available.
No tag or release was published. No new runtime features or dependencies were added.

Review identified one diagnostic risk: the proposed systemd recipe persisted
`systemctl --user show-environment` output, which could contain imported credentials.
The recipe and actual validation script now discard that output. The new
`tests/documentation.py` executes both diagnostic blocks with a fake manager and
a dummy secret, confirms the probe runs, and checks artifacts/stdout/stderr contain
no secret. It passed locally and in CI; the old recipe reproducibly fails this
regression. [Positive and negative evidence](../validation/v0.1.1/review-followup/)
is retained. This controlled test is separate from actual systemd execution.

### Native Linux execution and external results

[Hosted run 37025805580](https://github.com/0then0/exitscope/actions/runs/37025805580),
commit `a2c85cec16c5c48a33544b4429793af4276b0e06`, passed both native jobs:
`ubuntu-24.04-arm` / Linux ARM64 and `ubuntu-24.04` / Linux x86_64. The Docker
validation containers used Debian 13 (trixie), Rust 1.99.0 and the native host's
Linux **6.17.0-1022-azure** kernel. No cross-compilation or emulation substituted
for execution. On **each architecture**:

- build, **13 Rust tests**, **23 integration tests**, **6 observer regressions**,
  the documentation secret regression and both delegation checks passed;
- migration denial produced `cgroup_join` / EACCES and INFRASTRUCTURE_ERROR;
  explicitly delegated UID 65534 produced PASS in the disposable container;
- **pnpm 12.6.0 FAIL, pnpm 12.7.0 PASS, uv 0.5.1 FAIL, uv 0.5.2 PASS**, using
  the existing adapters, pinned upstream archives and common Rust engine;
- run cgroup removal / exit mapping checks passed, with verdict-before-cleanup
  and enabled receipt conditions rejecting late evidence.

Formatting, clippy and Rust 1.85.0 `cargo check --locked` passed once in the ARM64
job. The hosted jobs ran `scripts/linux-checks.sh`, `tests/delegation.py` and
`scripts/external.py` with per-architecture report paths; exact expanded commands
and execution output are retained in the
[ARM64 job log](../validation/v0.1.1/hosted/37025805580/arm64-job.txt) and
[x86_64 job log](../validation/v0.1.1/hosted/37025805580/x86_64-job.txt).
Both architecture artifacts uploaded successfully, including all target outcomes;
[artifact identities, SHA256 and download endpoints](../validation/v0.1.1/hosted/37025805580/artifacts.json)
are preserved. Raw JSON reports remain in those GitHub artifacts. Copying the
connector's temporary artifact URLs into the local workspace returned HTTP 403;
the complete decoded job logs were retained locally instead. This transport
limitation does not replace or invalidate uploaded execution reports.

### Actual systemd user-manager validation

The x86_64 hosted **Ubuntu 24.04.5 LTS** VM ran systemd **255.4-1ubuntu8.17**,
Linux **6.17.0-1022-azure**, unified cgroup v2. This validation ran **outside Docker**
as ordinary **UID 1001 (runner)**. Explicit workflow setup started the existing
user manager; it did not install a service/watchdog or change cgroup permissions.
The transient delegated service shell ran the engine under the ordinary user.

Exact setup / execution commands (the workflow expands absolute repository paths):

```sh
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
sudo systemctl start "user@$(id -u).service"
cargo build --locked --target-dir /tmp/exitscope-systemd-target
reports="$PWD/validation/ci/x86_64/systemd"
script -q -e -c "systemd-run --user --pty --collect --property=Delegate=yes bash '$PWD/scripts/systemd-checks.sh' '/tmp/exitscope-systemd-target/debug/exitscope' '$reports'" /dev/null
```

`script` supplies the terminal required by the documented `--pty` recipe in the
noninteractive CI shell; no target PTY scenario was added. The production engine
still gives the workload `/dev/null` stdin and independently observes output pipes.
A native host build avoids depending on the container's glibc version.

Inside that shell, `scripts/systemd-checks.sh` performs the README's explicit setup:
derive the service cgroup, create `harness` and `runs`, move the shell to `harness`,
then pass only `runs` as the engine's delegated parent. The recorded service
ancestor and its `cgroup.procs` are owned by runner. Observations:

- correct forwarding / finished cleanup: **PASS**, verified by the existing
  `test_correct_forwarding` assertion;
- broken forwarding / surviving worker: expected **FAIL**, verified by
  `test_missing_forwarding_and_pipes`, including survivor and signal findings;
- **6 observer regressions passed**, including timely receipt PASS, late receipt
  FAIL and disabled receipt PASS. They explicitly read the observer's actual
  cgroup and verify it remains outside the test cgroup;
- resource audit confirmed **all 8 systemd run cgroups and private IPC directories
  removed**, and no directories remained under `runs`, which was then removed;
- the delegated shell was in
  `/user.slice/user-1001.slice/user@1001.service/app.slice/run-u0.service/harness`;
  the transient service exited successfully and used `--collect`.

The independent insufficient-migration-permission test remains the UID 65534
container check on both native architectures. It is not mislabeled as systemd
ordinary-user evidence. The engine never starts a manager, elevates privileges,
recursively chowns a hierarchy, or installs a service.

### Retained attempts and guarantee boundaries

The first native hosted batch,
[37024615472](https://github.com/0then0/exitscope/actions/runs/37024615472), already
passed both architectures and external cases; its logs remain under
[hosted/37024615472](../validation/v0.1.1/hosted/37024615472/).
Systemd setup was then added. Two unsuccessful validation attempts are retained:
[37025136734](../validation/v0.1.1/hosted/37025136734/x86_64-job.txt) could not create
its reports within a Docker-root-owned directory; host-side directory creation
fixed that without permission weakening. [37025608115](../validation/v0.1.1/hosted/37025608115/x86_64-job.txt)
started a valid delegated ordinary-user service but its default home working
directory hid repository tests; the script now explicitly enters the repository.
Neither attempt was a target forwarding result; expected target outcomes were
never adjusted. The subsequent full run passed.

The prior local x86_64 emulation failure remains valid historical evidence about
that environment. It is superseded for compatibility acceptance by native hosted
execution, not hidden. Kernel **5.14 remains untested**; the minimum is based on
documented syscall availability. Systemd policy on other distributions remains
environment dependent. SIGKILL/host failure cleanup limitations and all original
non-goals remain. Upstream binary validation is not evidence of external adoption.

## Initial v0.1.1 preparation, 2026-10-02

**Prepared locally, acceptance incomplete, not ready to release or declare the
maintenance transition complete.** ARM64 correctness and external validation
passed. Native x86_64 execution, all four expected x86_64 external outcomes, and
the real systemd user-manager recipe remain open. Hosted CI was configured but
not executed. Everything after this section is retained historical v0.1.0 evidence,
including unsuccessful attempts; the current evidence is under
[validation/v0.1.1](../validation/v0.1.1/).

### Confirmed defect and verdict change

The hypothesis was confirmed both in source and by running the pre-change engine
with the new controlled regression. It accepted SIGTERM without checking its
receipt timestamp while collecting evidence until `max(shutdown_ms, output_ms)`.
The [baseline report](../validation/v0.1.1/arm64/baseline/receipt-late.json) records
impact 62 ms, root exit 67 ms, shutdown deadline 562 ms, snapshot 564 ms, receipt
1372 ms, stream EOF 1379 ms, and **PASS with no findings**. The worker deliberately
waited after receiving SIGTERM before reporting it. This demonstrates late
observation, and makes no assertion of late kernel delivery.

`model::evaluate` now requires `signal_received.at_ms <= impact_at_ms + shutdown_ms`
for SIGTERM confirmation. Timestamps are integer monotonic milliseconds; the exact
boundary is inclusive, consistent with existing root/EOF comparisons. Late evidence
remains intact and yields FAIL after acknowledged readiness with the existing
`SIGNAL_NOT_RECEIVED` ID and detail explaining the missed deadline. This ID means
no timely confirmation, even if confirmation is eventually observed. No new IDs,
report schema, deadline framework or engine dependencies were introduced.

Longer output observation cannot extend shutdown. The selective contract requires
root exit, signal receipt and both EOFs while disabling cleanup completion, root
waiting and no-survivors. It now fails only `SIGNAL_NOT_RECEIVED` for a late receipt;
the timely control passes. Disabling receipt passes with the same late evidence.
Absent required telemetry remains FAIL with readiness, UNRESOLVED without readiness;
an independent violation still establishes FAIL. The unrelated cleanup, root-wait,
survivor and EOF semantics were preserved, including verdict freeze before cleanup.

### Changes and verification

- `src/model.rs`: receipt deadline in pure evaluation and five regression tests
  covering late/disabled receipt, exact deadline minus/equal/plus one millisecond,
  readiness-dependent missing evidence, and late evidence without readiness.
- `tests/observer_regressions.py`: three real-process selective-contract cases,
  blocked SIGTERM before spawn, acknowledged readiness, sigwait-based forwarding,
  and an 800 ms margin between shutdown deadline and delayed reporting. Tests
  verify retained evidence, observer outside the test cgroup, verdict-before-cleanup,
  cgroup removal and IPC removal. Early infrastructure results are also saved.
- `.github/workflows/linux.yml`, `scripts/linux-checks.sh`: native ARM64/x86_64
  runner matrix, each executing build/unit/integration/observer/delegation/external
  checks. Fail-fast is disabled, architecture-specific artifacts upload with
  `always()`, independent suites continue after failures. Formatting/clippy/MSRV
  run once to avoid duplicate expensive checks.
- `scripts/external.py`, `validation/upstream-sources.json`: four x86_64 archives
  discovered using the existing maintainer workflow, preserving ARM64 and old uv
  entries. Exact URLs, SHA256 and tag commits are recorded. Normal mode re-downloaded
  and verified the new pins. Adapters additionally check exit mapping and removal
  of run/IPC resources after preserving reports. Expectations were not changed.
- `Cargo.toml`, `Cargo.lock`: package version 0.1.1; dependency graph unchanged.
  README, this report and [release notes](release-notes-v0.1.1.md) describe actual
  evidence, the receipt/delivery distinction and incomplete acceptance.

Native local ARM64 execution: Debian **13.7 (trixie)**, Linux
**7.0.14-linuxkit**, Docker Desktop private cgroup v2 namespace, explicit privileged
container setup, Rust **1.99.0**. The underlying development host is macOS ARM64;
this is Linux container validation, not macOS runtime support. Results:

- `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: **13 passed**, including actual Linux pidfd lifecycle.
- `cargo build --locked`: passed; `cargo check --locked` in `rust:1.85.0`: passed.
- Linux integration suite: **23 passed**, no skips; observer suite: **6 passed**.
  The observer suite was repeated after adding early-error artifact preservation.
- Delegation: common-ancestor migration denied with `cgroup_join` / EACCES gives
  INFRASTRUCTURE_ERROR; explicit permissions limited to the private subtree give
  UID 65534 PASS. This is container delegation evidence, not systemd recipe proof.
- External ARM64: **pnpm 12.6.0 FAIL, 12.7.0 PASS; uv 0.5.1 FAIL, 0.5.2 PASS**.
  Broken reports remain FAIL after successful forced cleanup. All expected outcomes
  matched, including exit-code mapping and resource-removal checks.

[ARM64 check output](../validation/v0.1.1/arm64/checks.txt),
[observer final output](../validation/v0.1.1/arm64/observer-final.txt),
[delegation/external output](../validation/v0.1.1/arm64/external-checks.txt) and
[MSRV output](../validation/v0.1.1/arm64/msrv.txt) are preserved with all JSON reports.

Local x86_64 **emulation**, on the same ARM64 LinuxKit VM: Debian 13.7, Rust 1.99.0.
Build succeeded and 12 Rust tests passed, including all pure verdict tests.
The pidfd lifecycle test failed: `pidfd_open` returned **ENOSYS (errno 38)**. Runtime
attempts fail before impact/readiness for the same missing syscall; this is an
emulation environment limitation, not evidence of broken forwarding in the targets.
An [independent Python pidfd probe](../validation/v0.1.1/x86_64/pidfd-environment-probe.txt)
also returned ENOSYS, without using ExitScope.
Of 23 integration tests, 2 setup checks passed and 21 failed their expected-outcome
assertions. Observer stopped at its first pre-readiness infrastructure result;
delegation stopped before verifying the expected migration-denied diagnostic.
All four upstream binaries executed `--version` successfully, but ExitScope returned
**INFRASTRUCTURE_ERROR for pnpm 12.6.0/12.7.0 and uv 0.5.1/0.5.2** in both discovery
and subsequent hash-verifying runs. No expectations or engine syscall protections
were weakened. [Build/unit output](../validation/v0.1.1/x86_64/checks.txt),
[runtime attempt](../validation/v0.1.1/x86_64/runtime-attempt.txt),
[discovery](../validation/v0.1.1/x86_64/external-discovery.txt) and
[normal pin verification](../validation/v0.1.1/x86_64/external-verified.txt) retain
these failures and reports. This does **not** satisfy x86_64 Linux compatibility.

Post-run inspections in each container found no run cgroup directories, target
processes or private IPC directories. Build directories and the explicit read-only
mount remained only until disposal of the validation containers. JSON reports
record `os=linux; arch=aarch64` or `arch=x86_64` and the kernel. Execution metadata
and cleanup inspections are stored per architecture. CI is configured for native
hosted runners; no hosted success or minimum-kernel success is claimed.

### Commands actually executed

The execution inventory is [commands.txt](../validation/v0.1.1/commands.txt).
Each architecture used a separate disposable container, avoiding shared build
outputs, with this explicit setup (ARM64 shown; x86_64 used `linux/amd64` and its
own container name). No host cgroup hierarchy was bind-mounted:

```sh
docker run -d --rm --name exitscope-v011-arm64 --platform linux/arm64 \
  --init --privileged --cgroupns=private -v "$PWD:/work" -w /work \
  -e CARGO_TARGET_DIR=/tmp/exitscope-target rust:1.99.0 sleep infinity
docker exec exitscope-v011-arm64 sh -c '
  mkdir /sys/fs/cgroup/exitscope-tests /tmp/exitscope-ro
  mount --bind /sys/fs/cgroup/exitscope-tests /tmp/exitscope-ro
  mount -o remount,bind,ro /tmp/exitscope-ro
  rustup component add rustfmt clippy
  cargo fmt
  cargo fmt --check
  cargo clippy --locked --all-targets -- -D warnings
  EXITSCOPE_CGROUP_PARENT=/sys/fs/cgroup/exitscope-tests \
  EXITSCOPE_READONLY_CGROUP=/tmp/exitscope-ro EXITSCOPE_CHECK_STYLE=0 \
  EXITSCOPE_REPORTS=validation/v0.1.1/arm64 sh scripts/linux-checks.sh
  python3 tests/delegation.py --binary /tmp/exitscope-target/debug/exitscope \
    --cgroup-parent /sys/fs/cgroup/exitscope-tests --reports validation/v0.1.1/arm64/delegation
  python3 scripts/external.py --binary /tmp/exitscope-target/debug/exitscope \
    --cgroup-parent /sys/fs/cgroup/exitscope-tests --reports validation/v0.1.1/arm64/external
'
docker run --rm --platform linux/arm64 -v "$PWD:/work" -w /work \
  -e CARGO_TARGET_DIR=/tmp/exitscope-msrv rust:1.85.0 cargo check --locked
```

The x86_64 external discovery ran `scripts/external.py --discover` on the x86_64
container and then normal mode without `--discover`. The pinned tag commits match
ARM64. The source manifest calls x86_64 `x64` to match pnpm asset names; uv uses
`x86_64-unknown-linux-gnu`. Local Python syntax checks and `sh -n
scripts/linux-checks.sh` passed. These executions install no project dependencies.

### Systemd recipe blocker and exact remaining reproduction

No suitable Linux host or disposable VM with a running systemd user manager was
available. The host is macOS, available Linux containers have `docker-init` PID 1,
`systemd` and `systemctl` are absent, and no Lima, Multipass, QEMU or libvirt VM
command was available. [Environment probes](../validation/v0.1.1/systemd-environment-probe.txt)
record exact commands and statuses. No systemd version or systemd execution result
can be reported. This acceptance criterion is **unfulfilled**. The privileged
UID 65534 checks above must not be substituted for it.

The following is an **unexecuted reproduction**, on a real suitable Linux host/VM,
logged in as its ordinary non-root user, with repository and Rust already present:

```sh
cd /absolute/path/to/exitscope
cargo build --locked
systemd-run --user --pty --collect --property=Delegate=yes bash
# Inside that explicitly delegated shell:
cd /absolute/path/to/exitscope
test "$(id -u)" -ne 0
parent="/sys/fs/cgroup$(awk -F: '$1 == "0" { print $3 }' /proc/self/cgroup)"
mkdir "$parent/harness" "$parent/runs"
printf '%s\n' "$$" > "$parent/harness/cgroup.procs"
reports="$PWD/validation/systemd/$(date +%s)-$(uname -m)"
mkdir -p "$reports"
{ cat /etc/os-release; uname -a; systemd --version; id; \
  systemctl --user show-environment >/dev/null; cat /proc/self/cgroup; \
  stat -f -c %T "$parent"; ls -ld "$parent" "$parent/cgroup.procs"; \
} > "$reports/environment.txt"
python3 - --cgroup-parent "$parent/runs" --reports "$reports/integration" <<'PY'
import runpy
import unittest
m = runpy.run_path("tests/integration.py", run_name="systemd_validation")
suite = unittest.TestSuite(m["Integration"](name) for name in [
    "test_correct_forwarding", "test_missing_forwarding_and_pipes"])
result = unittest.TextTestRunner(verbosity=2).run(suite)
raise SystemExit(0 if result.wasSuccessful() else 1)
PY
python3 tests/observer_regressions.py --cgroup-parent "$parent/runs" \
  --reports "$reports/observer"
python3 - "$reports" "$parent/runs" <<'PY'
import json
import sys
import tempfile
from pathlib import Path
for path in Path(sys.argv[1]).rglob("*.json"):
    r = json.loads(path.read_text())
    if r.get("cgroup"):
        assert not Path(r["cgroup"]).exists(), path
        assert not (Path(tempfile.gettempdir()) / Path(r["cgroup"]).name).exists(), path
        assert r["verdict_at_ms"] <= r["cleanup_started_at_ms"], path
assert not [p for p in Path(sys.argv[2]).iterdir() if p.is_dir()]
PY
rmdir "$parent/runs"
exit
# systemd collects the stopped service; its harness subtree is service-owned.
```

The availability probe discards the user manager's environment because imported
variables may contain credentials. `tests/documentation.py` exercises this exact
diagnostic block with a fake manager and a dummy secret; it checks that the probe
runs and the secret is absent from reports and console output. This test does not
validate a real systemd setup and runs with the Linux checks on both architectures.

Save console output and exact commands alongside these reports. The positive case
must PASS; broken forwarding must FAIL with survivors; the observer regressions
explicitly assert observer exclusion and IPC/cgroup removal. No automatic privilege
escalation, hierarchy changes, recursive chown, installed service or watchdog is
part of ExitScope. A denial by user-manager policy is a setup blocker, not a target
shutdown result. The independent migration-permission negative check remains the
recorded ARM64 container test until reproduced in another suitable environment.

### Release and maintenance decision

The deadline fix is verified on ARM64 and in pure evaluation. Release metadata and
notes are prepared. Acceptance still requires native x86_64 suites and four expected
external outcomes, plus actual systemd recipe execution with ordinary-user setup,
recorded systemd version, observer exclusion and cleanup. Kernel 5.14 and hosted CI
are also unverified; no minimum-kernel execution claim is made. External binary
validation establishes target behavior, not external adoption. No upstream PRs,
tags or release publication were performed. The original preparation was left
uncommitted; the review follow-up is authorized for commit and push. Maintenance begins
when this limited acceptance is fulfilled; new capabilities require confirmed need.

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
