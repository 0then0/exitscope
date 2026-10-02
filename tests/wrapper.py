"""Small controlled wrapper; no subprocess discovery or cleanup engine lives here."""

import os
import signal
import subprocess
import sys

mode, behavior = sys.argv[1:]
worker = None


def stop(signum, frame):
    if mode == "missing_forward":
        raise SystemExit(0)
    if mode == "early_root":
        worker.send_signal(signum)
        raise SystemExit(0)
    worker.send_signal(signal.SIGINT if mode == "wrong_signal" else signum)


signal.signal(signal.SIGTERM, stop)
worker = subprocess.Popen(
    [
        os.environ["EXIT_SCOPE_BIN"],
        "fixture",
        "--cleanup-ms",
        "120",
        "--behavior",
        behavior,
    ]
)
if marker := os.environ.get("EXITSCOPE_TEST_MARKER"):
    # This is only an interruption-test trigger, never harness readiness evidence.
    with open(marker, "w") as f:
        f.write(str(os.getpid()))
raise SystemExit(worker.wait())
