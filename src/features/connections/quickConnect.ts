import type { SshSpec } from "../../types";

export interface QuickConnectResult {
  ok: boolean;
  spec?: SshSpec;
  error?: string;
}

const HOST_RE = /^[A-Za-z0-9._:%\-[\]]+$/;
const USER_RE = /^[A-Za-z0-9._\-]+$/;

/**
 * Parse a Quick Connect string into a validated SSH spec.
 *
 * Mirrors the Rust-side validation (no leading '-', no whitespace/control
 * characters) so malformed input is rejected before any process is spawned.
 * Accepted forms: `host`, `user@host`, `ssh user@host`, `host:port`,
 * `user@host:port`.
 */
export function parseQuickConnect(raw: string): QuickConnectResult {
  let input = raw.trim();
  if (!input) {
    return { ok: false, error: "Enter a host to connect." };
  }

  const lower = input.toLowerCase();
  if (lower.startsWith("ssh ")) {
    input = input.slice(4).trim();
  }

  if (/\s/.test(input)) {
    return { ok: false, error: "Host must not contain spaces." };
  }

  let username: string | undefined;
  let hostPort = input;
  const at = input.indexOf("@");
  if (at !== -1) {
    username = input.slice(0, at);
    hostPort = input.slice(at + 1);
    if (!username || !USER_RE.test(username) || username.startsWith("-")) {
      return { ok: false, error: "Invalid username." };
    }
  }

  let host = hostPort;
  let port: number | undefined;

  // Bracketed IPv6 literal, optionally with a port.
  if (hostPort.startsWith("[")) {
    const end = hostPort.indexOf("]");
    if (end === -1) {
      return { ok: false, error: "Unterminated IPv6 literal." };
    }
    host = hostPort.slice(0, end + 1);
    const suffix = hostPort.slice(end + 1);
    if (suffix) {
      if (!suffix.startsWith(":")) {
        return { ok: false, error: "Invalid host." };
      }
      port = Number(suffix.slice(1));
    }
  } else {
    const colonCount = (hostPort.match(/:/g) || []).length;
    if (colonCount === 1) {
      const [h, p] = hostPort.split(":");
      host = h;
      port = Number(p);
    }
  }

  if (!host) {
    return { ok: false, error: "Missing host." };
  }
  if (host.startsWith("-")) {
    return { ok: false, error: "Host must not begin with '-'." };
  }
  if (!HOST_RE.test(host)) {
    return { ok: false, error: "Host contains unsupported characters." };
  }

  let resolvedPort = 22;
  if (port !== undefined) {
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      return { ok: false, error: "Port must be between 1 and 65535." };
    }
    resolvedPort = port;
  }

  const spec: SshSpec = {
    host,
    port: resolvedPort,
    username: username ?? null,
    identityFile: null,
    proxyJump: null,
    name: null,
    term: null,
  };
  return { ok: true, spec };
}

/** Human-readable label for a tab. */
export function specLabel(spec: SshSpec): string {
  return spec.username ? `${spec.username}@${spec.host}` : spec.host;
}
