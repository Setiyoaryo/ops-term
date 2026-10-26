import { afterEach, describe, expect, it, vi } from "vitest";
import type { Terminal } from "@xterm/xterm";
import {
  bufferOutput, clearState, decodeOutput, registerEncoding, registerTerminal,
  saveState, takeState, unregisterTerminal,
} from "./terminalRegistry";

const id = "registry-regression";
const limit = 256 * 1024;
afterEach(() => { unregisterTerminal(id); clearState(id); });

describe("terminal hibernation", () => {
  it("keeps streaming legacy encoding when the terminal pane detaches", () => {
    registerEncoding(id, "gbk");
    expect(decodeOutput(id, new Uint8Array([0xc4]))).toBe("");
    unregisterTerminal(id);
    expect(decodeOutput(id, new Uint8Array([0xe3]))).toBe("你");
  });

  it("resets a decoder when encoding changes", () => {
    registerEncoding(id, "gbk");
    expect(decodeOutput(id, new Uint8Array([0xc4, 0xe3]))).toBe("你");
    registerEncoding(id, "windows-1252");
    expect(decodeOutput(id, new Uint8Array([0xe9]))).toBe("é");
  });

  it("bounds a single oversized chunk", () => {
    registerEncoding(id, "utf-8");
    bufferOutput(id, new Uint8Array(limit * 2).fill(65));
    const write = vi.fn();
    registerTerminal(id, { write } as unknown as Terminal);
    const size = write.mock.calls.reduce((n, [chunk]) => n + chunk.length, 0);
    expect(size).toBeLessThanOrEqual(limit);
    expect(size).toBeGreaterThan(0);
  });

  it("clears queued output on permanent session disposal", () => {
    registerEncoding(id, "utf-8");
    bufferOutput(id, "stale output");
    saveState(id, "stale snapshot");
    clearState(id);
    bufferOutput(id, "late output after close");
    saveState(id, "late React cleanup snapshot");
    registerEncoding(id, "utf-8");
    const write = vi.fn();
    registerTerminal(id, { write } as unknown as Terminal);
    expect(write).not.toHaveBeenCalled();
    expect(takeState(id)).toBeUndefined();
  });
});
