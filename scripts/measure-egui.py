#!/usr/bin/env python3
"""Linux/Xvfb benchmark: temporary 500-device library, idle, then 12k lines.

Run under scripts/build_guard.py and xvfb-run. No network connections are made.
CPU percent uses 100% for one core; software GL may use several cores.
"""
import json
import os
import pathlib
import sqlite3
import subprocess
import sys
import tempfile
import time


def memory(pid):
    fields = {}
    for line in pathlib.Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[2] == "kB":
            fields[parts[0].rstrip(":")] = int(parts[1])
    return {key: round(fields[key] / 1024, 2)
            for key in ("Rss", "Pss", "Private_Dirty")}


def cpu(pid):
    fields = pathlib.Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(fields[11]) + int(fields[12])


def idle_sample(pid):
    start, before = time.monotonic(), cpu(pid)
    time.sleep(5)
    percent = (cpu(pid) - before) / os.sysconf("SC_CLK_TCK") / (time.monotonic() - start) * 100
    return {"cpu_percent": round(percent, 2), "memory_mib": memory(pid)}


def launch(binary, env):
    diagnostic = tempfile.TemporaryFile(mode="w+t")
    process = subprocess.Popen([str(binary)], env=env,
                               stdout=diagnostic, stderr=subprocess.STDOUT)
    try:
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            result = subprocess.run(
                ["xdotool", "search", "--onlyvisible", "--pid", str(process.pid), "--name", "^OpsTerm$"],
                capture_output=True, text=True, check=False)
            if result.returncode == 0:
                return process, result.stdout.splitlines()[0]
            if process.poll() is not None:
                diagnostic.seek(0)
                raise RuntimeError("App exited before its window appeared:\n" + diagnostic.read()[-6000:])
            time.sleep(.05)
        raise RuntimeError("No app window appeared")
    except BaseException:
        stop(process)
        raise
    finally:
        diagnostic.close()


def stop(process):
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def main():
    binary = pathlib.Path(sys.argv[1]).resolve()
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 500
    env = dict(os.environ, LIBGL_ALWAYS_SOFTWARE="1")
    env.pop("WAYLAND_DISPLAY", None)
    with tempfile.TemporaryDirectory(prefix="opsterm-measure-") as data:
        env.update(XDG_DATA_HOME=data, OPSTERM_BENCH_DIR=data)
        process, _ = launch(binary, env)
        stop(process)
        with sqlite3.connect(pathlib.Path(data) / "org.opsterm.app/opsterm.db") as db:
            db.executemany(
                "INSERT INTO sessions(id,name,host,protocol,created_at,updated_at) VALUES(?,?,?,?,?,?)",
                [(f"fixture-{i}", f"Lab router {i:04d}", "192.0.2.1", "ssh", "2026-10-22", "2026-10-22")
                 for i in range(count)])
        process, window = launch(binary, env)
        try:
            time.sleep(5)
            idle = idle_sample(process.pid)
            subprocess.run(["xdotool", "windowfocus", "--sync", window], check=True)
            subprocess.run(["xdotool", "mousemove", "--window", window, "450", "300", "click", "1"], check=True)
            subprocess.run(["xdotool", "type", "--clearmodifiers",
                            'seq 1 12000; touch "$OPSTERM_BENCH_DIR/output-ready"'], check=True)
            subprocess.run(["xdotool", "key", "Return"], check=True)
            deadline = time.monotonic() + 15
            while not (pathlib.Path(data) / "output-ready").exists():
                if time.monotonic() > deadline:
                    raise RuntimeError("Local shell output workload did not complete")
                time.sleep(.1)
            time.sleep(2)
            print(json.dumps({"binary": str(binary), "devices": count, "output_lines": 12000,
                              "idle": idle, "after_output_idle": idle_sample(process.pid)}, indent=2))
        finally:
            stop(process)


if __name__ == "__main__":
    main()
