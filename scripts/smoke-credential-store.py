#!/usr/bin/env python3
"""Real secure-store checks using unique fixtures; never reads user secrets.

Build helper: cargo build -p opsterm-core --example credential_store_smoke
"""
import pathlib
import subprocess
import uuid


def main():
    helper = pathlib.Path("target/debug/examples/credential_store_smoke").resolve()
    references = [f"opsterm-smoke-{uuid.uuid4()}" for _ in range(2)]
    def operation(mode, reference):
        subprocess.run([str(helper), mode, reference], check=True,
                       stdout=subprocess.DEVNULL, timeout=15)
    try:
        operation("set", references[0])
        operation("get", references[0])
        operation("kernel-delete", references[0])
        operation("get", references[0])
        operation("durable-get", references[0])
        operation("kernel-set", references[1])
        operation("get", references[1])
        operation("durable-get", references[1])
        operation("kernel-delete", references[1])
        operation("get", references[1])
    finally:
        for reference in references:
            operation("delete", reference)
            operation("missing", reference)
    print("Credential store smoke passed: independent processes, kernel-cache loss, durable read, legacy migration, dual-store cleanup.")


if __name__ == "__main__":
    main()
