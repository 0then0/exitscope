# Changelog

Notable user-facing changes are recorded here. Detailed execution evidence lives
in the [engineering report](docs/engineering-report.md).

## Unreleased

- Added monthly, grouped Dependabot updates for Cargo and GitHub Actions, with a
  seven-day cooldown and one open version-update PR per ecosystem.
- Added contributor, conduct and security policies, issue/PR templates and shared
  editor/Git settings. Linux CI cancels obsolete runs and has a job time limit.

## [0.1.1] - 2026-10-02

- Fixed late SIGTERM confirmation satisfying the shutdown deadline when the
  independent output deadline was longer. The boundary is inclusive; receipt
  time is observation evidence, not kernel signal delivery time.
- Preserved late trace events, selective contracts, finding IDs and conservative
  missing-telemetry outcomes. Added pure and real-process deadline regressions.
- Validated native ARM64/x86_64 execution, pinned pnpm/uv broken/fixed cases and
  real systemd user-manager delegation under an ordinary user.
- Bounded systemd CI setup, execution and cleanup; added timeout controls and
  excluded manager environment output from diagnostics.
- Kept compact validation inputs and selected reports in the source tree;
  archived historical successes and failures through pinned Git evidence links.

See [release notes](docs/release-notes-v0.1.1.md) for details and tested environments.
Kernel 5.14 is a documented minimum, not an execution-tested one.

## [0.1.0] - 2026-10-02

- Initial Linux CLI for parent-only SIGTERM and dedicated-group SIGKILL checks.
- Added cgroup containment, acknowledged worker telemetry, independent root exit,
  survivor and stream observations, and bounded cleanup after verdict recording.
- Added JSON profiles/reports, stable outcomes, fixtures, integration tests and
  external adapters for real wrapper shutdown behavior.

[0.1.1]: https://github.com/0then0/exitscope/releases/tag/v0.1.1
[0.1.0]: https://github.com/0then0/exitscope/releases/tag/v0.1.0
