#!/usr/bin/env python3
"""Run a Linux build serially and stop its process group before RAM is exhausted.

Usage: python3 scripts/build_guard.py -- cargo build -p opsterm-egui --release
The watchdog samples RSS (shared pages counted conservatively) every 100 ms.
This is not a kernel hard limit or a guarantee against unrelated memory spikes.
"""
import fcntl
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
MIB = 1024 * 1024


def limit(name, default):
    value = int(os.environ.get(name, default))
    if value < 1:
        raise ValueError(f"{name} must be a positive integer")
    return value * MIB


def available_memory():
    for line in Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemAvailable:"):
            available = int(line.split()[1]) * 1024
            break
    else:
        raise RuntimeError("MemAvailable is unavailable")
    # Respect enclosing cgroup v2 limits too (containers/CI).
    for line in Path("/proc/self/cgroup").read_text().splitlines():
        if line.startswith("0::"):
            relative = line[3:].lstrip("/")
            group = Path("/sys/fs/cgroup") / relative
            while group.is_relative_to(Path("/sys/fs/cgroup")):
                try:
                    maximum = (group / "memory.max").read_text().strip()
                    if maximum != "max":
                        current = int((group / "memory.current").read_text())
                        available = min(available, max(0, int(maximum) - current))
                except FileNotFoundError:
                    pass
                group = group.parent
    return available


def group_rss(pgid):
    pages = 0
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            # comm may contain spaces/parentheses; fields after the last ')'
            # begin with state, ppid, pgrp.
            fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
            if int(fields[2]) == pgid:
                pages += int((entry / "statm").read_text().split()[1])
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            continue
    return pages * os.sysconf("SC_PAGE_SIZE")


def stop_group(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
        time.sleep(0.3)
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def main():
    args = sys.argv[1:]
    if args[:1] == ["--"]:
        args = args[1:]
    if not args:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        maximum = limit("OPSTERM_BUILD_MAX_MB", "2048")
        reserve = limit("OPSTERM_BUILD_RESERVE_MB", "1536")
    except ValueError as error:
        print(f"build-guard: invalid memory setting: {error}", file=sys.stderr)
        return 2
    with (ROOT / ".opsterm-build.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print("build-guard: another guarded build is already running", file=sys.stderr)
            return 75
        if available_memory() < reserve:
            print("build-guard: insufficient available RAM; command was not started", file=sys.stderr)
            return 75
        env = {**os.environ, "CARGO_BUILD_JOBS": "1"}
        env.setdefault("NODE_OPTIONS", "--max-old-space-size=384 --max-semi-space-size=8")
        try:
            process = subprocess.Popen(args, cwd=ROOT, env=env, start_new_session=True)
        except OSError as error:
            print(f"build-guard: {error}", file=sys.stderr)
            return 127
        peak = 0

        def interrupted(signum, _frame):
            raise KeyboardInterrupt

        signal.signal(signal.SIGTERM, interrupted)
        try:
            while process.poll() is None:
                rss = group_rss(process.pid)
                peak = max(peak, rss)
                if rss >= maximum or available_memory() < reserve:
                    print(f"build-guard: Stopping build to protect RAM (RSS {rss / MIB:.0f} MiB)", file=sys.stderr, flush=True)
                    stop_group(process)
                    return 75
                time.sleep(0.1)
            return process.returncode if process.returncode >= 0 else 128 - process.returncode
        except KeyboardInterrupt:
            stop_group(process)
            return 130
        except (OSError, ValueError, RuntimeError) as error:
            print(f"build-guard: memory monitoring failed; stopping build: {error}", file=sys.stderr)
            stop_group(process)
            return 75
        finally:
            print(f"build-guard: peak sampled RSS {peak / MIB:.1f} MiB", file=sys.stderr)


if __name__ == "__main__":
    raise SystemExit(main())
