import type { HighlightPreset } from "../state/highlight";

export interface VendorProfile {
  id: string;
  label: string;
  pager: "none" | "huawei" | "cisco" | "juniper" | "mikrotik" | "custom";
  pagerCommand?: string;
  prompt: string;
  backspace: "del" | "bs";
  loginScript: string;
  highlights: HighlightPreset[];
  commands: { name: string; content: string }[];
}

function preset(
  id: string,
  name: string,
  pattern: string,
  color: string,
  enabled = true,
): HighlightPreset {
  return { id, name, pattern, regex: true, caseSensitive: false, enabled, color };
}

export const VENDOR_PROFILES: VendorProfile[] = [
  {
    id: "huawei",
    label: "Huawei VRP",
    pager: "huawei",
    prompt: "[<\\[\\]>]\\s*$",
    backspace: "bs",
    loginScript:
      "waitfor [#>\\]]\\s*$\nsleep 200\n# sendline enable\n# waitfor [Pp]assword:\n# sendline <enable-secret>",
    highlights: [
      preset("hw-err", "Errors", "Error|down|failed|invalid", "#E06C75"),
      preset("hw-warn", "Warning", "Warning", "#E5C07B"),
    ],
    commands: [
      { name: "Huawei: display version", content: "display version" },
      { name: "Huawei: display interface brief", content: "display interface brief" },
      { name: "Huawei: display current-configuration", content: "display current-configuration" },
      { name: "Huawei: display ip routing-table", content: "display ip routing-table" },
    ],
  },
  {
    id: "cisco",
    label: "Cisco IOS/IOS-XE",
    pager: "cisco",
    prompt: "[>#]\\s*$",
    backspace: "bs",
    loginScript: "# waitfor [#>]\\s*$\n# sendline enable\n# waitfor [Pp]assword:\n# sendline <enable-secret>",
    highlights: [
      preset("cs-err", "Errors", "% ?Error|down|failed|invalid", "#E06C75"),
      preset("cs-up", "Interfaces up", "\\bup\\b", "#98C379"),
    ],
    commands: [
      { name: "Cisco: show version", content: "show version" },
      { name: "Cisco: show ip interface brief", content: "show ip interface brief" },
      { name: "Cisco: show running-config", content: "show running-config" },
      { name: "Cisco: show ip route", content: "show ip route" },
    ],
  },
  {
    id: "juniper",
    label: "Juniper Junos",
    pager: "juniper",
    prompt: "[>#]\\s*$",
    backspace: "del",
    loginScript: "# waitfor [>#]\\s*$\n# sendline configure\n# sendline show | display set",
    highlights: [
      preset("ju-err", "Errors", "error|down|fail", "#E06C75"),
      preset("ju-bgp", "BGP", "BGP|bgp", "#4F8EE8"),
    ],
    commands: [
      { name: "Junos: show version", content: "show version" },
      { name: "Junos: show interfaces terse", content: "show interfaces terse" },
      { name: "Junos: show bgp summary", content: "show bgp summary" },
    ],
  },
  {
    id: "mikrotik",
    label: "MikroTik RouterOS",
    pager: "mikrotik",
    prompt: "[>]\\s*$",
    backspace: "del",
    loginScript: "# waitfor [>]\\s*$",
    highlights: [preset("mt-err", "Errors", "failure|error", "#E06C75")],
    commands: [
      { name: "RouterOS: export", content: "/export" },
      { name: "RouterOS: interface print", content: "/interface print" },
      { name: "RouterOS: ip route print", content: "/ip route print" },
    ],
  },
  {
    id: "zte",
    label: "ZTE OLT",
    pager: "none",
    prompt: "[>#]\\s*$",
    backspace: "bs",
    loginScript: "waitfor [>#]\\s*$\nsendline terminal length 0",
    highlights: [preset("zte-err", "Errors", "% ?Error|down", "#E06C75")],
    commands: [
      { name: "ZTE: show version", content: "show version" },
      { name: "ZTE: show interface", content: "show interface" },
    ],
  },
  {
    id: "linux",
    label: "Linux / generic shell",
    pager: "none",
    prompt: "[$#]\\s*$",
    backspace: "del",
    loginScript: "# waitfor [$#]\\s*$",
    highlights: [
      preset("lx-err", "Errors", "error|failed|cannot|denied", "#E06C75"),
      preset("lx-path", "Paths", "(/[\\w.-]+){2,}", "#56B6C2", false),
    ],
    commands: [
      { name: "Shell: uptime", content: "uptime" },
      { name: "Shell: disk usage", content: "df -h" },
      { name: "Shell: ip addresses", content: "ip -br addr" },
    ],
  },
];

export function findVendor(id: string | undefined): VendorProfile | undefined {
  return VENDOR_PROFILES.find((v) => v.id === id);
}
