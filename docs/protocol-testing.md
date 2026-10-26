# Protocol testing

Real behaviour must be demonstrated against real endpoints. This document lists
the lab setup used for the M1 exit gates.

## SSH

- **Target:** Ubuntu OpenSSH server in a container or VM (the host the app is
  developed on also works for a self-test).
- **Scenarios:**
  - agent/key authentication;
  - interactive password and keyboard-interactive prompts answered in the PTY;
  - unknown host key → OpenSSH prompt surfaces in the terminal;
  - changed host key → connection blocked with a warning (AT-05);
  - `ProxyJump` through a bastion;
  - IPv6 literal destination;
  - remote resize (`stty size` reflects the new grid);
  - `Ctrl+C` interrupts a remote foreground process.

Run a throwaway server locally:

```bash
docker run --rm -p 2222:22 linuxserver/openssh-server
# then: ssh user@localhost:2222  → in Quick Connect: user@localhost:2222
```

## Local shell

Covered automatically by `src-tauri/tests/pty_shell.rs` (round-trip + resize).
Manual check: open a local shell, run `ls -la`, `vim`, `top`; resize while
inside `top`; `Ctrl+C` interrupts a running process; closing the tab reaps the
child.

## Not yet applicable

Telnet, serial, SFTP and broadcast tests are defined in the PRD but not run
until their milestones.
