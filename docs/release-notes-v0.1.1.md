# v0.1.1: Deadline Correctness & Linux Compatibility

Release preparation, 2026-10-02. **Acceptance verified; ready for release and maintenance.**

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
- Systemd CI setup, build, client execution and unit stop have explicit time
  limits. A transient service runtime limit remains effective after client loss;
  timeout regressions retain diagnostics and propagate failures.
- x86_64 upstream archives were added through the existing maintainer discovery
  workflow with exact URLs, commits and SHA256. Normal runs verified these pins.

Local ARM64 on Debian 13 / Linux 7.0.14-linuxkit passed all relevant suites and
pnpm 12.6.0 FAIL / 12.7.0 PASS, uv 0.5.1 FAIL / 0.5.2 PASS. Local x86_64 emulation
compiled and passed pure verdict tests, but `pidfd_open` returned ENOSYS; its Linux
runtime and all four external attempts produced infrastructure errors. Expected
outcomes were retained. Subsequent hosted native ARM64 and x86_64 CI passed the
full suites and all four expected pinned external outcomes on each architecture.

The systemd user-manager recipe passed on a hosted Ubuntu 24.04.5 x86_64 VM,
kernel 6.17.0-1022-azure, systemd 255.4, UID 1001, outside Docker. Correct forwarding
passed, broken forwarding failed, and observer exclusion / cleanup were verified.
UID 65534 container delegation checks remain separate evidence. Diagnostic probes
now discard the manager environment; regression coverage prevents dummy secrets
from entering artifacts. Kernel 5.14 remains untested. See the
[engineering report](engineering-report.md#hosted-acceptance-and-review-follow-up-2026-10-02)
for exact commands, preserved unsuccessful attempts and current evidence.

This is a correctness and compatibility milestone. No v0.2 features or runtime
dependencies were added. The completed milestone moves to maintenance;
new features require demonstrated need. No tag or release was created.
