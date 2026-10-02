#!/bin/sh
# Bound the CI client independently of the delegated validation service.
# Requires an already running user manager; never elevates privileges.
set -eu
binary="$1"
reports="$2"
mkdir -p "$reports"
repo=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
unit="exitscope-validation-${GITHUB_RUN_ID:-$$}-${GITHUB_RUN_ATTEMPT:-1}.service"
run_timeout="${EXITSCOPE_SYSTEMD_TIMEOUT:-180s}"
stop_timeout="${EXITSCOPE_SYSTEMD_STOP_TIMEOUT:-10s}"
cleanup() {
  status=$?
  trap - EXIT
  cleanup_status=0
  timeout --kill-after=2s "$stop_timeout" systemctl --user show \
    --property=LoadState --value "$unit" > "$reports/unit-load-state.txt" \
    2> "$reports/unit-cleanup.txt" || cleanup_status=$?
  if [ "$cleanup_status" -eq 0 ] && [ "$(cat "$reports/unit-load-state.txt")" != not-found ]; then
    timeout --kill-after=2s "$stop_timeout" systemctl --user stop "$unit" \
      >> "$reports/unit-cleanup.txt" 2>&1 || cleanup_status=$?
  fi
  printf '%s\n' "$status" > "$reports/execution-status.txt"
  printf '%s\n' "$cleanup_status" > "$reports/unit-cleanup-status.txt"
  if [ "$status" -eq 0 ] && [ "$cleanup_status" -ne 0 ]; then status=$cleanup_status; fi
  exit "$status"
}
trap cleanup EXIT
# RuntimeMaxSec also contains the service if its PTY client is killed/disconnected.
command="systemd-run --user --pty --collect --unit='$unit' --property=Delegate=yes --property=RuntimeMaxSec=120s --property=TimeoutStopSec=5s bash '$repo/scripts/systemd-checks.sh' '$binary' '$reports'"
printf '%s\n' "timeout --kill-after=5s $run_timeout script -q -e -c \"$command\" /dev/null" >> "$reports/commands.txt"
timeout --kill-after=5s "$run_timeout" script -q -e -c "$command" /dev/null
