"""Run pinned upstream binaries through the SAME CLI engine on disposable resources.

No alternate process discovery, signaling, or cleanup implementation in this adapter.
All outcomes are persisted before assertions. Downloads are SHA256 verified against
validation/upstream-sources.json; --discover is a maintainer-only metadata bootstrap.
"""

import argparse
import hashlib
import json
import os
import platform
import subprocess
import tarfile
import tempfile
import time
import urllib.request
from pathlib import Path

CASES = [
    ("pnpm", "12.6.0", "FAIL"),
    ("pnpm", "12.7.0", "PASS"),
    ("uv", "0.5.1", "FAIL"),
    ("uv", "0.5.2", "PASS"),
]
REPOS = {"pnpm": "pnpm/pnpm", "uv": "astral-sh/uv"}
ROOT = Path(__file__).resolve().parents[1]


def fetch(url):
    request = urllib.request.Request(
        url, headers={"User-Agent": "ExitScope-validation/0.1"}
    )
    with urllib.request.urlopen(request, timeout=60) as r:
        return r.read()


def api(path):
    return json.loads(fetch("https://api.github.com/" + path))


def commit(repo, tag):
    obj = api(f"repos/{repo}/git/ref/tags/{tag}")["object"]
    if obj["type"] == "tag":
        obj = api(f"repos/{repo}/git/tags/{obj['sha']}")["object"]
    if obj["type"] != "commit":
        raise ValueError("tag does not identify a commit")
    return obj["sha"]


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary", type=Path, default=ROOT / "target/debug/exitscope")
    p.add_argument("--cgroup-parent", type=Path, required=True)
    p.add_argument("--reports", type=Path, default=ROOT / "validation/external")
    p.add_argument("--discover", action="store_true")
    args = p.parse_args()
    args.reports = args.reports / str(time.time_ns())
    args.reports.mkdir(parents=True, exist_ok=True)
    machine = {"aarch64": "arm64", "x86_64": "x64"}[platform.machine()]
    manifest_path = ROOT / "validation/upstream-sources.json"
    manifest = (
        json.loads(manifest_path.read_text())
        if manifest_path.exists()
        else {"sources": []}
    )
    failures = []
    with tempfile.TemporaryDirectory(prefix="exitscope-external-") as tmp:
        tmp = Path(tmp)
        # Runner caches and project resources are private to this batch.
        env = dict(
            os.environ,
            UV_CACHE_DIR=str(tmp / "uv-cache"),
            UV_PYTHON_DOWNLOADS="never",
            XDG_CACHE_HOME=str(tmp / "cache"),
            XDG_DATA_HOME=str(tmp / "data"),
            XDG_STATE_HOME=str(tmp / "state"),
        )
        for runner, version, expected in CASES:
            repo = REPOS[runner]
            tag = ("v" if runner == "pnpm" else "") + version
            arch = "aarch64" if machine == "arm64" else "x86_64"
            asset = (
                f"pnpm-linux-{machine}.tar.gz"
                if runner == "pnpm"
                else f"uv-{arch}-unknown-linux-gnu.tar.gz"
            )
            url = f"https://github.com/{repo}/releases/download/{tag}/{asset}"
            label = f"{runner}-{version}-{machine}"
            archive = fetch(url)
            digest = hashlib.sha256(archive).hexdigest()
            if args.discover:
                source = {
                    "runner": runner,
                    "version": version,
                    "tag": tag,
                    "commit": commit(repo, tag),
                    "arch": machine,
                    "url": url,
                    "sha256": digest,
                    "expected": expected,
                }
                manifest["sources"] = [
                    s
                    for s in manifest["sources"]
                    if (s["runner"], s["version"], s["arch"])
                    != (runner, version, machine)
                ] + [source]
                manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
            else:
                source = next(
                    s
                    for s in manifest["sources"]
                    if (s["runner"], s["version"], s["arch"])
                    == (runner, version, machine)
                )
                if digest != source["sha256"] or url != source["url"]:
                    raise ValueError("upstream archive identity changed")
            work = tmp / label
            work.mkdir()
            tarpath = work / "archive.tar.gz"
            tarpath.write_bytes(archive)
            with tarfile.open(tarpath) as tar:
                tar.extractall(work, filter="data")
            executable = next(f for f in work.rglob(runner) if f.is_file())
            observed_version = subprocess.check_output(
                [str(executable), "--version"], env=env, text=True
            ).strip()
            project = work / "project"
            project.mkdir()
            if runner == "pnpm":
                (project / "package.json").write_text(
                    json.dumps(
                        {
                            "name": "exitscope-upstream-case",
                            "private": True,
                            "scripts": {"serve": '"$EXIT_SCOPE_BIN" fixture'},
                        }
                    )
                )
                command_args = ["run", "serve"]
                scenario = "group_sigkill"
            else:
                (project / "pyproject.toml").write_text(
                    '[project]\nname = "exitscope-case"\nversion = "0.0.0"\nrequires-python = ">=3.11"\ndependencies = []\n'
                )
                command_args = [
                    "run",
                    "--offline",
                    "--no-sync",
                    str(args.binary.resolve()),
                    "fixture",
                    "--cleanup-ms",
                    "200",
                ]
                scenario = "parent_sigterm"
            graceful = runner == "uv"
            profile = {
                "executable": str(executable),
                "args": command_args,
                "cwd": str(project),
                "scenario": scenario,
                "readiness": "fixture",
                "readiness_ms": 10000,
                "shutdown_ms": 1500,
                "output_ms": 1800,
                "contract": {
                    "signal_receipt": graceful,
                    "cleanup_finished": graceful,
                    "root_waits_cleanup": graceful,
                    "no_survivors": True,
                    "output_eof": True,
                },
            }
            config = work / "config.json"
            config.write_text(json.dumps(profile))
            run = subprocess.run(
                [
                    str(args.binary.resolve()),
                    "run",
                    "--config",
                    str(config),
                    "--cgroup-parent",
                    str(args.cgroup_parent),
                    "--json",
                ],
                env=env,
                text=True,
                capture_output=True,
                timeout=20,
                check=False,
            )
            (args.reports / f"{label}.json").write_text(run.stdout)
            (args.reports / f"{label}.stderr.txt").write_text(run.stderr)
            r = json.loads(run.stdout)
            metadata = {
                "source": source,
                "observed_version": observed_version,
                "outcome": r["outcome"],
                "exit_code": run.returncode,
            }
            (args.reports / f"{label}.source.json").write_text(
                json.dumps(metadata, indent=2) + "\n"
            )
            print(
                label,
                observed_version,
                r["outcome"],
                [f["id"] for f in r["findings"]],
                flush=True,
            )
            if r["outcome"] != expected:
                failures.append(label)
    if failures:
        raise SystemExit("unexpected outcomes: " + ", ".join(failures))


if __name__ == "__main__":
    main()
