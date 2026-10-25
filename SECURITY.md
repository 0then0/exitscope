# Security policy

## Supported versions

Security fixes target the latest stable release, currently **v0.1.1**.
Older releases are not maintained as separate security branches. Update to the
latest stable release before reporting a problem that may already be fixed.
No response-time or extended-support commitment is made.

## Reporting a vulnerability

Use GitHub's private [Report a vulnerability](https://github.com/0then0/exitscope/security/advisories/new)
flow when available. Do not disclose exploit details, credentials or sensitive
reports in public issues or pull requests.

If private reporting is unavailable, request a private contact channel from
[@0then0](https://github.com/0then0). A public issue may contain only that contact
request; send vulnerability details after a private channel is established.
No private reporting feature or email address is assumed to be configured.

Include the version/commit, expected security boundary, impact, minimal
reproduction, architecture, kernel and delegation setup. Explain required
privileges or access to the test subtree. Share only necessary, sanitized
evidence, excluding secrets and unrelated personal data.

The maintainer will assess reports and coordinate fixes and disclosure where
appropriate. This maintenance project cannot promise a fixed disclosure deadline.

## Security boundaries

ExitScope is a shutdown-contract test harness, **not a security sandbox**.
Use trusted workloads in an explicitly delegated, private test subtree. Targets
must not deliberately escape containment, forge telemetry, manipulate cgroups
or attack the observer. Automatic privilege escalation, global delegation setup
and a privileged watchdog are outside scope.

SIGKILL, host failure, lost permissions and persistent uninterruptible tasks can
prevent cleanup. Reports expose controlled paths and process identities; inspect
them before sharing. Raw command output and systemd environment dumps can contain
secrets even when an ExitScope JSON report does not.

See [cleanup and limits](README.md#cleanup-and-limits). Violations of documented
ownership, signal-targeting or bounded-processing guarantees within that model
are appropriate to report privately.
