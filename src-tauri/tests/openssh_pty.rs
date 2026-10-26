//! Real SSH integration test (opt-in).
//!
//! Ignored by default because it requires a reachable OpenSSH server and
//! pre-authorised key/agent authentication for the current user. Run with:
//!
//! ```bash
//! OPSTERM_SSH_HOST=localhost OPSTERM_SSH_USER=$USER \
//!   cargo test --test openssh_pty -- --ignored --nocapture
//! ```
//!
//! This exercises the actual M1 SSH path: validated argv → OpenSSH under a PTY
//! → interactive remote shell → resize-free command round-trip → cleanup.

use std::io::Read;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use opsterm_lib::domain::{SshSpec, TerminalDimensions};
use opsterm_lib::providers::openssh_pty::spawn_openssh;

fn start_pump(
    mut reader: Box<dyn Read + Send>,
) -> (mpsc::Receiver<Vec<u8>>, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    (rx, handle)
}

fn read_until(rx: &mpsc::Receiver<Vec<u8>>, needle: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut acc: Vec<u8> = Vec::new();
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                acc.extend_from_slice(&chunk);
                if String::from_utf8_lossy(&acc).contains(needle) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    String::from_utf8_lossy(&acc).into_owned()
}

#[test]
#[ignore = "requires a reachable OpenSSH server with key auth"]
fn ssh_command_roundtrip() {
    let host = std::env::var("OPSTERM_SSH_HOST").unwrap_or_else(|_| "localhost".into());
    let port: u16 = std::env::var("OPSTERM_SSH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(22);
    let username = std::env::var("OPSTERM_SSH_USER")
        .ok()
        .or_else(|| std::env::var("USER").ok());

    let spec = SshSpec {
        host,
        port,
        username,
        identity_file: None,
        proxy_jump: None,
        name: Some("self-test".into()),
        term: None,
        connect_timeout: None,
        server_alive_interval: None,
        x11_forwarding: false,
        forward_agent: false,
        compression: false,
        certificate_file: None,
        pkcs11_provider: None,
        engine: None,
        password: None,
    };
    let dims = TerminalDimensions { cols: 80, rows: 24 };
    let provider = spawn_openssh(&spec, dims).expect("spawn ssh");
    let reader = provider.take_reader().expect("take reader");
    let (rx, pump) = start_pump(reader);

    // Wait for the login shell to settle, then run a marker command.
    let _ = read_until(&rx, ">", Duration::from_secs(3));
    let _ = read_until(&rx, "$", Duration::from_secs(3));
    provider.write(b"echo OPSTERM_SSH_OK\n").expect("write ssh");

    let text = read_until(&rx, "OPSTERM_SSH_OK", Duration::from_secs(15));
    assert!(
        text.contains("OPSTERM_SSH_OK"),
        "did not observe SSH marker: {text:?}"
    );

    provider.kill().expect("kill ssh");
    let _ = pump.join();
    assert!(
        !provider.is_alive(),
        "ssh process should be dead after kill"
    );
}
