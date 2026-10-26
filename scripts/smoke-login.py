#!/usr/bin/env python3
"""Real egui saved-credential auto-login smoke. Run with xvfb-run; uses loopback fixtures only."""
import os
import pathlib
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout.strip()


def wait_for(check, message, seconds=8):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(.05)
    raise RuntimeError(message)


def line(peer):
    """Read a line, consuming Telnet commands (including client resize frames)."""
    out = bytearray()
    def byte():
        value = peer.recv(1)
        if not value:
            raise RuntimeError("Fixture peer disconnected")
        return value[0]
    while True:
        value = byte()
        if value == 255:
            command = byte()
            if command == 250:
                previous = 0
                while True:
                    current = byte()
                    if previous == 255 and current == 240:
                        break
                    previous = current
            elif command in (251, 252, 253, 254):
                byte()
            elif command == 255:
                out.append(255)
            continue
        if value == 13:
            return bytes(out)
        if value not in (0, 10):
            out.append(value)


def main():
    helper = pathlib.Path("target/debug/examples/credential_store_smoke").resolve()
    reference = None
    binary = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "target/release/opsterm-egui").resolve()
    output = pathlib.Path("test-results/egui-login").resolve()
    output.mkdir(parents=True, exist_ok=True)
    connected, authenticated, command_received, stop = (threading.Event() for _ in range(4))
    errors = []
    with socket.socket() as listener, tempfile.TemporaryDirectory(prefix="opsterm-login-") as data:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(.2)
        port = listener.getsockname()[1]

        def serve():
            try:
                while not stop.is_set():
                    try:
                        peer, _ = listener.accept()
                        break
                    except socket.timeout:
                        continue
                else:
                    return
                with peer:
                    peer.settimeout(5)
                    connected.set()
                    peer.sendall(b"Loopback test device\r\nUsername:")
                    assert line(peer) == b"test-user", "Username was not sent correctly"
                    peer.sendall(b"\r\nPassword:")
                    assert line(peer) == b"fixture-password", "Password was not sent correctly"
                    peer.sendall(b"\r\nLOGIN_OK\r\n<lab> ")
                    authenticated.set()
                    assert line(peer) == b"show version", "Direct keyboard input did not reach the terminal"
                    peer.sendall(b"show version\r\nLoopback fixture ready\r\n<lab> ")
                    command_received.set()
                    stop.wait(5)
            except Exception as error:
                errors.append(str(error))

        server = threading.Thread(target=serve, daemon=True)
        server.start()
        env = dict(os.environ, XDG_DATA_HOME=data, LIBGL_ALWAYS_SOFTWARE="1")
        env.pop("WAYLAND_DISPLAY", None)
        process, window = None, None
        with (output / "gui.log").open("w") as log:
            def launch():
                app = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
                def find():
                    if app.poll() is not None:
                        raise RuntimeError("App exited before window appeared")
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid), "--name", "^OpsTerm$"], capture_output=True, text=True)
                    return found.stdout.splitlines()[0] if found.returncode == 0 else None
                try:
                    return app, wait_for(find, "No app window")
                except BaseException:
                    app.terminate()
                    app.wait(timeout=5)
                    raise

            try:
                process, window = launch()
                process.terminate()
                process.wait(timeout=5)
                reference = f"opsterm-smoke-login-{os.getpid()}-{time.time_ns()}"
                with sqlite3.connect(pathlib.Path(data) / "org.opsterm.app/opsterm.db") as db:
                    db.execute("INSERT INTO credentials(name,username,kind,tags_json,created_at,updated_at) VALUES(?,?,?, ?,?,?)",
                               (reference, "test-user", "password", "[]", "2026-10-23", "2026-10-23"))
                    db.execute("INSERT INTO sessions(id,name,host,port,protocol,credential_ref,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)",
                               ("login-fixture", "Lab Telnet", "127.0.0.1", port, "telnet", reference, "2026-10-23", "2026-10-23"))
                run(str(helper), "set", reference)
                run(str(helper), "kernel-delete", reference)
                process, window = launch()
                run("xdotool", "windowfocus", "--sync", window)
                time.sleep(.4)
                run("xdotool", "mousemove", "--window", window, "45", "208", "click", "1")
                wait_for(authenticated.is_set, "Telnet fixture login did not finish")
                time.sleep(.5)
                run("xdotool", "type", "--clearmodifiers", "show version")
                run("xdotool", "key", "Return")
                wait_for(command_received.is_set, "Terminal required an extra click before accepting input")
                time.sleep(.3)
                run("import", "-window", window, str(output / "logged-in.png"))
                run("xdotool", "windowsize", window, "760", "500")
                time.sleep(.3)
                run("import", "-window", window, str(output / "minimum.png"))
                assert not errors, errors
                print("GUI saved-credential smoke passed: automatic secure-store lookup, Telnet authentication without password entry, immediate keyboard input.")
            except BaseException:
                if window:
                    subprocess.run(["import", "-window", window, str(output / "failure.png")], check=False)
                if errors:
                    print("Fixture errors:", errors, file=sys.stderr)
                raise
            finally:
                stop.set()
                if process and process.poll() is None:
                    process.terminate()
                    process.wait(timeout=5)
                server.join(timeout=2)
                if reference:
                    run(str(helper), "delete", reference)


if __name__ == "__main__":
    main()
