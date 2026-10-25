# Contributing to ExitScope

ExitScope is in maintenance after v0.1.1. Focus on confirmed bugs, compatibility,
tests and documentation. Discuss a new feature or dependency in an issue with a
concrete use case before implementing it. Avoid expanding the supervisor's scope.
Participation follows the [Code of Conduct](CODE_OF_CONDUCT.md).

## Reporting problems

Use the [bug report form](https://github.com/0then0/exitscope/issues/new?template=bug_report.yml).
Include the version, architecture, distribution, kernel, delegation setup,
reproduction and expected/actual result. Distinguish native execution from
emulation and a target contract failure from an infrastructure failure.
For vulnerabilities, follow [SECURITY.md](SECURITY.md) instead of posting details
in a public issue or pull request.

## Development and checks

- Runtime tests require Linux, cgroup v2 and explicit delegation. See the
  [requirements and setup](README.md#requirements-and-installation).
- Use Rust 1.85.0 or newer, as declared in `Cargo.toml`. Keep `Cargo.lock` tracked
  and use `--locked`.
- Test scripts use Python 3.11+ and its standard library. Timeout regressions
  additionally require GNU `timeout`, available in Linux CI.
- Follow `.editorconfig`; `cargo fmt` is authoritative for Rust formatting.

Build and check code changes on Linux:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

For observation, verdict, containment or cleanup changes, run the process suites
inside an explicitly delegated, disposable subtree:

```sh
EXITSCOPE_CGROUP_PARENT=/path/to/delegated/runs sh scripts/linux-checks.sh
```

The shell and observer stay outside test run cgroups. Use the README systemd
recipe or CI's disposable namespace setup; do not change global cgroup permissions.
`tests/delegation.py` requires the explicit privileged disposable-container setup
shown in CI and is not a general ordinary-user test.

When external adapters or source pins change, also run:

```sh
python3 scripts/external.py --cgroup-parent /path/to/delegated/runs
```

Normal external validation verifies pinned downloads. `--discover` is a maintainer
operation that changes `validation/upstream-sources.json`; inspect URLs, upstream
provenance and digests. Preserve unexpected outcomes rather than weakening
expectations to make CI pass.

For documentation/configuration-only changes, run the relevant syntax/link checks
and `python3 tests/documentation.py`. Explain skipped runtime checks. A successful
macOS build does not establish Linux runtime compatibility.

## Review expectations

- Keep patches small and preserve unrelated behavior.
- Add regressions for observable bugs. Use pure tests for precise deadline
  boundaries and acknowledged events for readiness, with ample timing margins.
- Preserve parent-only SIGTERM, cgroup ownership, independent observations,
  verdict-before-cleanup and bounded output/IPC. Cleanup must not turn FAIL into PASS.
- Describe the problem, resulting behavior, exact checks and limitations in the
  PR. Update documentation and `CHANGELOG.md` when appropriate.
- Use English Conventional Commits: `fix:`, `test:`, `docs:`, `chore:`,
  `refactor:` or `feat:`. Features still require a demonstrated need.
- Exclude generated batches and credentials. Keep selected evidence small and
  record provenance; see [validation/README.md](validation/README.md).

CI runs native ARM64 and x86_64 Linux suites and pinned external cases, plus
ordinary-user systemd validation on x86_64. Formatting, clippy and minimum Rust
checks run once. Changes are reviewed before merging; no automatic merge is implied.

## License

Contributions use the project's [Apache-2.0 license](LICENSE). Submit only material
you have the right to contribute. No separate CLA or mandatory sign-off is required.
