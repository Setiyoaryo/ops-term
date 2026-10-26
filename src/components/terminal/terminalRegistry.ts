import type { Terminal } from "@xterm/xterm";
import type { SearchAddon } from "@xterm/addon-search";

/**
 * Global registry of live xterm instances keyed by backend session id.
 *
 * Terminal output events arrive independently of React rendering, so the
 * routing layer needs a stable way to find the right terminal instance even
 * when a tab is inactive or mid-remount. The search addon is registered in the
 * same registry so the toolbar can drive find-in-terminal without prop
 * drilling through the pane tree.
 */
const terminals = new Map<string, Terminal>();
const searches = new Map<string, SearchAddon>();
const pending = new Map<string, { chunks: (Uint8Array | string)[]; size: number }>();
const encodings = new Map<string, string>();
const decoders = new Map<string, TextDecoder>();

/**
 * Serialized terminal snapshots for hibernated tabs. Inactive tabs have their
 * xterm instance disposed and only this small string is kept, so hundreds of
 * idle tabs cost almost nothing.
 */
const STATE_CACHE_LIMIT = 500;
const states = new Map<string, string>();
const closed = new Set<string>();

/** Cap buffered output for a hibernated session (bytes/chars). */
const PENDING_LIMIT = 256 * 1024;

export function saveState(id: string, state: string): void {
  if (closed.has(id)) return;
  states.delete(id);
  states.set(id, state);
  while (states.size > STATE_CACHE_LIMIT) {
    const oldest = states.keys().next().value;
    if (oldest === undefined) break;
    states.delete(oldest);
  }
}

export function takeState(id: string): string | undefined {
  return states.get(id);
}

export function clearState(id: string): void {
  states.delete(id);
  pending.delete(id);
  encodings.delete(id);
  decoders.delete(id);
  closed.add(id);
  // Ignore late output and React cleanup after close without retaining all IDs.
  while (closed.size > STATE_CACHE_LIMIT) closed.delete(closed.values().next().value!);
}

export function registerTerminal(id: string, term: Terminal): void {
  if (closed.has(id)) return;
  terminals.set(id, term);
  const queued = pending.get(id);
  if (queued) {
    for (const chunk of queued.chunks) term.write(chunk);
    pending.delete(id);
  }
}

export function unregisterTerminal(id: string): void {
  terminals.delete(id);
  searches.delete(id);
}

/** Remember the configured output encoding for a session. */
export function registerEncoding(id: string, encoding: string): void {
  closed.delete(id);
  if (encodings.get(id) !== (encoding === "utf-8" ? undefined : encoding)) {
    decoders.delete(id);
  }
  if (encoding && encoding !== "utf-8") encodings.set(id, encoding);
  else encodings.delete(id);
}

/**
 * Decode session output for the terminal. UTF-8 is returned as raw bytes so
 * xterm handles multi-byte characters split across chunks; other encodings use
 * a streaming TextDecoder (GBK, latin1, …).
 */
export function decodeOutput(id: string, bytes: Uint8Array): string | Uint8Array {
  const encoding = encodings.get(id);
  if (!encoding) return bytes;
  let decoder = decoders.get(id);
  if (!decoder) {
    try {
      decoder = new TextDecoder(encoding, { fatal: false });
    } catch {
      return bytes;
    }
    decoders.set(id, decoder);
  }
  return decoder.decode(bytes, { stream: true });
}

export function registerSearch(id: string, search: SearchAddon): void {
  searches.set(id, search);
}

export function getSearch(id: string): SearchAddon | undefined {
  return searches.get(id);
}

export function getTerminal(id: string): Terminal | undefined {
  return terminals.get(id);
}

/**
 * Buffer output for a session whose terminal pane has not mounted yet. This
 * closes the race between `connection_open_*` returning and React rendering
 * the new pane, so no early bytes are dropped.
 */
export function bufferOutput(id: string, data: Uint8Array | string): void {
  if (closed.has(id)) return;
  let queued = pending.get(id);
  if (!queued) {
    queued = { chunks: [], size: 0 };
    pending.set(id, queued);
  }
  // slice copies oversized typed arrays instead of retaining their backing buffer.
  const chunk = data.length > PENDING_LIMIT ? data.slice(-PENDING_LIMIT) : data;
  queued.chunks.push(chunk);
  queued.size += chunk.length;
  // Keep hibernated-tab buffers bounded so idle tabs cannot grow without limit.
  while (queued.size > PENDING_LIMIT && queued.chunks.length > 1) {
    queued.size -= queued.chunks.shift()!.length;
  }
}

/** Decode a base64 event payload into raw bytes. */
export function decodeBase64(data: string): Uint8Array {
  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}
