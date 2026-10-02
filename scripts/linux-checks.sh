#!/bin/sh
# Run inside an explicitly prepared disposable Linux namespace/host.
set -eu
: "${EXITSCOPE_CGROUP_PARENT:?set a writable delegated cgroup v2 subtree}"
if [ "${EXITSCOPE_CHECK_STYLE:-1}" = 1 ]; then
  cargo fmt --check
  cargo clippy --locked --all-targets -- -D warnings
fi
status=0
python3 tests/documentation.py || status=1
python3 tests/systemd_timeout.py || status=1
cargo test --locked || status=1
cargo build --locked
binary="${CARGO_TARGET_DIR:-target}/debug/exitscope"
reports="${EXITSCOPE_REPORTS:-validation}"
python3 tests/integration.py --binary "$binary" \
  --cgroup-parent "$EXITSCOPE_CGROUP_PARENT" --reports "$reports/synthetic" || status=1
python3 tests/observer_regressions.py --binary "$binary" \
  --cgroup-parent "$EXITSCOPE_CGROUP_PARENT" --reports "$reports/observer" || status=1
exit "$status"
