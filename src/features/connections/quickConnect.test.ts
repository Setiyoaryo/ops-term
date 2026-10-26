import { describe, expect, it } from "vitest";

import { parseQuickConnect } from "./quickConnect";

describe("parseQuickConnect", () => {
  it("parses a bare host with default port", () => {
    const result = parseQuickConnect("router.example.net");
    expect(result.ok).toBe(true);
    expect(result.spec).toMatchObject({ host: "router.example.net", port: 22, username: null });
  });

  it("parses user@host and strips the ssh prefix", () => {
    const result = parseQuickConnect("ssh operator@router.example.net");
    expect(result.ok).toBe(true);
    expect(result.spec).toMatchObject({
      host: "router.example.net",
      username: "operator",
      port: 22,
    });
  });

  it("parses host:port", () => {
    const result = parseQuickConnect("router.example.net:2222");
    expect(result.ok).toBe(true);
    expect(result.spec).toMatchObject({ host: "router.example.net", port: 2222 });
  });

  it("parses bracketed IPv6 literals", () => {
    const result = parseQuickConnect("operator@[2001:db8::1]:2022");
    expect(result.ok).toBe(true);
    expect(result.spec).toMatchObject({
      host: "[2001:db8::1]",
      username: "operator",
      port: 2022,
    });
  });

  it("accepts bare IPv6", () => {
    const result = parseQuickConnect("2001:db8::1");
    expect(result.ok).toBe(true);
    expect(result.spec).toMatchObject({ host: "2001:db8::1", port: 22 });
  });

  it("rejects a host that looks like an option", () => {
    const result = parseQuickConnect("-oProxyCommand=touch");
    expect(result.ok).toBe(false);
    expect(result.spec).toBeUndefined();
  });

  it("rejects empty input", () => {
    expect(parseQuickConnect("   ").ok).toBe(false);
  });

  it("rejects whitespace in the host", () => {
    expect(parseQuickConnect("bad host").ok).toBe(false);
  });

  it("rejects out-of-range ports", () => {
    expect(parseQuickConnect("host:0").ok).toBe(false);
    expect(parseQuickConnect("host:70000").ok).toBe(false);
    expect(parseQuickConnect("host:notaport").ok).toBe(false);
  });

  it("rejects invalid usernames", () => {
    expect(parseQuickConnect("-x@host").ok).toBe(false);
    expect(parseQuickConnect("bad user@host").ok).toBe(false);
  });
});
