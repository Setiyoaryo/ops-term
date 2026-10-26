import { describe, expect, it } from "vitest";

import { parseSecureCrt } from "./ImportExportPanel";

describe("parseSecureCrt", () => {
  it("parses host, user and hex port", () => {
    const ini = [
      'S:"Hostname"=10.0.0.1',
      'S:"Username"=admin',
      'D:"[SSH2] Port"=000008ae', // 2222
      'S:"Protocol Name"=SSH2',
    ].join("\r\n");
    const s = parseSecureCrt(ini, "CORE-JKT-01")!;
    expect(s.name).toBe("CORE-JKT-01");
    expect(s.host).toBe("10.0.0.1");
    expect(s.username).toBe("admin");
    expect(s.port).toBe(2222);
    expect(s.protocol).toBe("ssh");
  });

  it("detects telnet and defaults its port", () => {
    const ini = ['S:"Hostname"=sw.old', 'S:"Protocol Name"=Telnet'].join("\n");
    const s = parseSecureCrt(ini, "SW-OLD")!;
    expect(s.protocol).toBe("telnet");
    expect(s.port).toBe(23);
  });

  it("returns null when there is no hostname", () => {
    expect(parseSecureCrt('S:"Username"=x', "NOPE")).toBeNull();
  });
});
