#!/bin/sh
# Run inside an explicitly prepared disposable Linux namespace/host.
set -eu
: "${EXITSCOPE_CGROUP_PARENT:?set a writable delegated cgroup v2 subtree}"
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
python3 tests/integration.py --cgroup-parent "$EXITSCOPE_CGROUP_PARENT"
python3 tests/observer_regressions.py --cgroup-parent "$EXITSCOPE_CGROUP_PARENT"
