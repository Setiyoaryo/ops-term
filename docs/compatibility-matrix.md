# Device compatibility matrix

A "vendor profile" is only CLI defaults/suggestions — the transports are
SSH/Telnet/Serial. This tracks what has actually been validated.

| Family | Transport | Verified in OpsTerm | Notes |
|---|---|---|---|
| Ubuntu/Debian Linux | SSH / local shell | local shell ✅ ; SSH pending lab | PTY + ANSI validated locally |
| Cisco IOS / IOS-XE | SSH | Planned (M1) | pagination optional per session |
| Cisco IOS-XR / NX-OS | SSH | Planned | prompt/syntax differ |
| Juniper Junos | SSH | Planned | screen width + pager |
| Huawei VRP | SSH | Planned | `display` CLI |
| MikroTik RouterOS | SSH | Planned | ROS v6 vs v7 syntax |
| ZTE OLT | SSH/Telnet/Serial | Planned (M2) | firmware/pager nuances |
| Generic BSD/Linux server | SSH | Planned (M1) | agent, ProxyJump |

Pager-disable commands (e.g. `terminal length 0`) are **not** sent
automatically; they will be an explicit opt-in preset with a compatibility
note.
