# v0.1.1: Deadline Correctness & Linux Compatibility

Release preparation, 2026-10-02. **Acceptance is incomplete; not ready to release.**

- An enabled `signal_receipt` now requires observed SIGTERM confirmation at or
  before `impact_at_ms + shutdown_ms`. A longer output deadline cannot turn late
  confirmation into PASS. Late evidence is retained; `SIGNAL_NOT_RECEIVED` describes
  absent or late confirmation. No finding IDs or report schema were added.
- Selective contracts still permit late receipts when that condition is disabled.
  Missing telemetry retains FAIL after confirmed readiness and conservative
  UNRESOLVED without readiness. Other contract semantics are unchanged.
- Pure tests cover the inclusive millisecond boundary; real Linux regressions
  cover late, timely and disabled receipt conditions with acknowledged readiness.
- CI is configured for native ARM64 and x86_64 execution of build, unit, integration,
  observer, delegation and four external cases. Style and Rust 1.85 checks run once.
  Architecture specific artifacts are uploaded even after failures.
- x86_64 upstream archives were added through the existing maintainer discovery
  workflow with exact URLs, commits and SHA256. Normal runs verified these pins.

Local ARM64 on Debian 13 / Linux 7.0.14-linuxkit passed all relevant suites and
pnpm 12.6.0 FAIL / 12.7.0 PASS, uv 0.5.1 FAIL / 0.5.2 PASS. Local x86_64 emulation
compiled and passed pure verdict tests, but `pidfd_open` returned ENOSYS; its Linux
runtime and all four external attempts produced infrastructure errors. Expected
outcomes were retained. Hosted native x86_64 CI has not run.

The systemd user-manager setup requires a suitable real Linux host or disposable
VM, unavailable here. UID 65534 delegation checks in a privileged container are
separate evidence. Kernel 5.14 remains untested. See the
[engineering report](engineering-report.md#v011-milestone-status-2026-10-02) for
commands, preserved results and open acceptance criteria.

This is a correctness and compatibility milestone. No v0.2 features or runtime
dependencies were added. Transition to maintenance follows completed acceptance;
new features require demonstrated need. No tag or release was created.
