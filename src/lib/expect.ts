import type { AutomationStep } from "../types";

/**
 * A tiny expect DSL used by login scripts and the automation panel.
 *
 *   send <text>            send without newline
 *   sendline <text>        send with a trailing CR
 *   waitfor <regex> [ms]   wait for a regex (default 10000 ms)
 *   sleep <ms>             pause
 *   # comment / blank lines are ignored
 */
export function parseExpect(script: string, sessionId?: string): AutomationStep[] {
  const steps: AutomationStep[] = [];
  for (const raw of script.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const spaceAt = line.search(/\s/);
    const cmd = (spaceAt === -1 ? line : line.slice(0, spaceAt)).toLowerCase();
    const arg = spaceAt === -1 ? "" : line.slice(spaceAt).trim();
    switch (cmd) {
      case "send":
        steps.push({ type: "send", sessionId, data: arg, appendNewline: false });
        break;
      case "sendline":
        steps.push({ type: "send", sessionId, data: arg, appendNewline: true });
        break;
      case "waitfor": {
        const parts = arg.split(/\s+/);
        const last = parts[parts.length - 1];
        const hasTimeout = /^\d+$/.test(last) && parts.length > 1;
        steps.push({
          type: "waitFor",
          sessionId,
          pattern: hasTimeout ? parts.slice(0, -1).join(" ") : arg,
          timeoutMs: hasTimeout ? Number(last) : 10000,
        });
        break;
      }
      case "sleep":
        steps.push({ type: "sleep", ms: Number(arg) || 0 });
        break;
      default:
        break;
    }
  }
  return steps;
}

/** Pager-disable command presets per vendor family. */
export const PAGER_PRESETS: Record<string, string> = {
  huawei: "screen-length 0 temporary",
  cisco: "terminal length 0",
  juniper: "set cli screen-length 0",
  mikrotik: "/terminal print",
};

/** A generic shell/device prompt used before sending the pager command. */
export const DEFAULT_PROMPT = "[>#\\]]\\s*$";
