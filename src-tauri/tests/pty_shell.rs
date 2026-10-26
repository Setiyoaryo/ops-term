//! Real PTY integration tests.
//!
//! These spawn an actual interactive shell under a pseudo-terminal and verify
//! byte-level round-trip, resize propagation, and process cleanup — the M0/M1
//! exit gate from PRD section 14 (Phase 0/1). No GUI and no network required.

use std::io::Read;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use opsterm_lib::domain::TerminalDimensions;
use opsterm_lib::providers::local_pty::spawn_local_shell;

/// Read from `reader` on a background thread, forwarding chunks over a channel.
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

/// Collect output until `needle` appears or the deadline passes.
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
fn local_shell_write_read_roundtrip() {
    let dims = TerminalDimensions { cols: 80, rows: 24 };
    let provider = spawn_local_shell(dims).expect("spawn local shell");
    let reader = provider.take_reader().expect("take reader");
    let (rx, pump) = start_pump(reader);

    provider
        .write(b"echo OPSTERM_PTY_OK\n")
        .expect("write command");

    let text = read_until(&rx, "OPSTERM_PTY_OK", Duration::from_secs(10));
    assert!(
        text.contains("OPSTERM_PTY_OK"),
        "did not observe marker in shell output: {text:?}"
    );

    provider.kill().expect("kill");
    let _ = pump.join();
    assert!(!provider.is_alive(), "process should be dead after kill");
}

#[test]
fn local_shell_resize_propagates_to_remote_pty() {
    let dims = TerminalDimensions { cols: 80, rows: 24 };
    let provider = spawn_local_shell(dims).expect("spawn local shell");
    let reader = provider.take_reader().expect("take reader");
    let (rx, pump) = start_pump(reader);

    // Consume any initial prompt noise.
    let _ = read_until(&rx, "$", Duration::from_secs(2));

    provider
        .resize(TerminalDimensions {
            cols: 100,
            rows: 40,
        })
        .expect("resize");

    provider.write(b"stty size\n").expect("write stty");
    let text = read_until(&rx, "40 100", Duration::from_secs(10));
    assert!(
        text.contains("40 100"),
        "resize was not reflected by stty size: {text:?}"
    );

    provider.kill().expect("kill");
    let _ = pump.join();
}
