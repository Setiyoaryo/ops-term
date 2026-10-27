//! OpsTerm — custom-rendered UI (egui) with a real terminal (alacritty_terminal).
//!
//! Terax-inspired modern dark workspace, tuned for network engineers: a left
//! panel for saved devices, tabs on top, a block terminal in the centre, and an
//! editor-like command input panel at the bottom. No GTK, no webview.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Processor, Rgb};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, Stroke, Vec2,
};
use serde::{Deserialize, Serialize};

use opsterm_core::domain::{
    Command, Folder, FolderInput, Runbook, RunbookInput, SavedSession, SavedSessionInput,
    SerialSpec, SshSpec, TelnetSpec, TerminalDimensions, Workspace, WorkspaceInput,
};
use opsterm_core::providers::ConnectionProvider;
use opsterm_core::sftp::SftpEntry;
use opsterm_core::tunnel::{TunnelManager, TunnelSpec};
use opsterm_core::vault::{self, Vault};
use opsterm_core::{security, storage::Database};

mod appearance;
mod background;
mod close_confirmation;
mod command_input;
mod connections;
mod device_list;
mod display_backend;
mod distribution;
mod quick_connect;
mod remote_files;
mod repaint;
mod session_io;
mod startup;
mod terminal_render;
mod update;
use connections::Origin;

// ---- theme (selectable palettes) -----------------------------------------
#[derive(Clone, Copy)]
struct Theme {
    bg: Color32,
    panel: Color32,
    card: Color32,
    border: Color32,
    text: Color32,
    muted: Color32,
    accent: Color32,
    ok: Color32,
}
impl Theme {
    fn opencode() -> Self {
        Theme {
            bg: Color32::from_rgb(0x0A, 0x0A, 0x0A),
            panel: Color32::from_rgb(0x14, 0x14, 0x14),
            card: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            border: Color32::from_rgb(0x3C, 0x3C, 0x3C),
            text: Color32::from_rgb(0xFA, 0xFA, 0xFA),
            muted: Color32::from_rgb(0x80, 0x80, 0x80),
            accent: Color32::from_rgb(0xFA, 0xB2, 0x83),
            ok: Color32::from_rgb(0x7F, 0xD8, 0x8F),
        }
    }
    fn terax() -> Self {
        Theme {
            bg: Color32::from_rgb(0x0D, 0x11, 0x17),
            panel: Color32::from_rgb(0x14, 0x1A, 0x24),
            card: Color32::from_rgb(0x1A, 0x22, 0x30),
            border: Color32::from_rgb(0x26, 0x2E, 0x3B),
            text: Color32::from_rgb(0xE6, 0xED, 0xF3),
            muted: Color32::from_rgb(0x89, 0x96, 0xAA),
            accent: Color32::from_rgb(0x75, 0xA7, 0xFF),
            ok: Color32::from_rgb(0x3F, 0xD1, 0x8B),
        }
    }
    fn tokyo() -> Self {
        Theme {
            bg: Color32::from_rgb(0x1A, 0x1B, 0x26),
            panel: Color32::from_rgb(0x16, 0x16, 0x1E),
            card: Color32::from_rgb(0x1F, 0x23, 0x35),
            border: Color32::from_rgb(0x29, 0x2E, 0x42),
            text: Color32::from_rgb(0xC0, 0xCA, 0xF5),
            muted: Color32::from_rgb(0x56, 0x5F, 0x89),
            accent: Color32::from_rgb(0x7A, 0xA2, 0xF7),
            ok: Color32::from_rgb(0x9E, 0xCE, 0x6A),
        }
    }
    fn catppuccin() -> Self {
        Theme {
            bg: Color32::from_rgb(0x1E, 0x1E, 0x2E),
            panel: Color32::from_rgb(0x18, 0x18, 0x25),
            card: Color32::from_rgb(0x31, 0x32, 0x44),
            border: Color32::from_rgb(0x45, 0x47, 0x5A),
            text: Color32::from_rgb(0xCD, 0xD6, 0xF4),
            muted: Color32::from_rgb(0x7F, 0x84, 0x9C),
            accent: Color32::from_rgb(0x89, 0xB4, 0xFA),
            ok: Color32::from_rgb(0xA6, 0xE3, 0xA1),
        }
    }
}
fn ttl_label(secs: u64) -> &'static str {
    match secs {
        3_600 => "1 hour",
        28_800 => "8 hours",
        86_400 => "1 day",
        604_800 => "7 days",
        2_592_000 => "30 days",
        _ => "custom",
    }
}

const DEFAULT_TERM_FONT: f32 = 17.0;
const DEFAULT_SCROLL_SPEED: f32 = 72.0;
const DEFAULT_TERMINAL_SCROLL_SPEED: f32 = 96.0;

fn default_terminal_scroll_speed() -> f32 {
    DEFAULT_TERMINAL_SCROLL_SPEED
}
const DEFAULT_VAULT_TTL_SECS: u64 = 7 * 24 * 60 * 60;
static THEME: std::sync::OnceLock<Mutex<Theme>> = std::sync::OnceLock::new();
fn theme() -> Theme {
    *THEME
        .get_or_init(|| Mutex::new(Theme::opencode()))
        .lock()
        .unwrap()
}
fn set_theme(t: Theme) {
    *THEME
        .get_or_init(|| Mutex::new(Theme::opencode()))
        .lock()
        .unwrap() = t;
}

const THEME_NAMES: [&str; 4] = ["Opencode", "Terax", "Tokyo", "Catppuccin"];

fn theme_for(idx: usize) -> Theme {
    match idx {
        1 => Theme::terax(),
        2 => Theme::tokyo(),
        3 => Theme::catppuccin(),
        _ => Theme::opencode(),
    }
}

fn theme_index(name: &str) -> usize {
    THEME_NAMES
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(name.trim()))
        .unwrap_or(0)
}

fn theme_name(idx: usize) -> &'static str {
    THEME_NAMES[idx.min(THEME_NAMES.len() - 1)]
}
fn bg() -> Color32 {
    theme().bg
}
fn panel() -> Color32 {
    theme().panel
}
fn card() -> Color32 {
    theme().card
}
fn border() -> Color32 {
    theme().border
}
fn text_c() -> Color32 {
    theme().text
}
fn muted() -> Color32 {
    theme().muted
}
fn accent() -> Color32 {
    theme().accent
}
fn ok() -> Color32 {
    theme().ok
}

/// Terminal feedback that must be routed back to the PTY (device attributes,
/// cursor reports, color queries, window-size queries) or to the host UI
/// (title, clipboard, bell). Dropping these events is what makes TUIs such as
/// Codex/opencode fall back to the terminal palette.
#[derive(Clone)]
struct Listener {
    events: Arc<Mutex<Vec<Event>>>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        if let Ok(mut queue) = self.events.lock() {
            // Bound the queue: unsupported requests must never grow memory.
            if queue.len() < 512 {
                queue.push(event);
            }
        }
    }
}

struct Size {
    cols: usize,
    rows: usize,
}
impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
const OUTPUT_QUEUE_CHUNKS: usize = 16;
const SCROLLBACK_LINES: usize = 2000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SelMode {
    Char,
    Word,
    Line,
}

struct Session {
    id: u64,
    title: String,
    provider: Arc<dyn ConnectionProvider>,
    rx: Receiver<Vec<u8>>,
    processor: Processor,
    term: Term<Listener>,
    cols: usize,
    rows: usize,
    exited: bool,
    log: Arc<Mutex<Option<std::fs::File>>>,
    log_path: Option<String>,
    bell: Arc<AtomicBool>,
    saved_id: Option<String>,
    origin: Origin,
    reconnect_tried: bool,
    connected_at: Instant,
    broadcast_selected: bool,
    backspace: u8,
    sel_anchor: Option<Point>,
    sel_head: Option<Point>,
    sel_mode: SelMode,
    selecting: bool,
    sel_scroll_at: Option<Instant>,
    scroll_pixels: f32,
    cell_width: f32,
    cell_height: f32,
    reported_focus: bool,
    mouse_cell: Option<(usize, i32)>,
    events: Arc<Mutex<Vec<Event>>>,
    opening: bool,
    connection_error: Option<String>,
    wake: repaint::Wake,
    io_status: Option<session_io::Status>,
}

impl Session {
    fn close_is_risky(&self) -> bool {
        !self.opening
            && !self.exited
            && self.provider.is_alive()
            && (!matches!(self.origin, Origin::Local) || self.provider.has_running_command())
    }

    fn new(
        title: String,
        provider: Box<dyn ConnectionProvider>,
        cols: usize,
        rows: usize,
        saved_id: Option<String>,
        origin: Origin,
    ) -> Self {
        let mut reader: Box<dyn Read + Send> = provider.take_reader().expect("reader");
        let (tx, rx) = sync_channel(OUTPUT_QUEUE_CHUNKS);
        let log: Arc<Mutex<Option<std::fs::File>>> = Arc::new(Mutex::new(None));
        let log_thread = log.clone();
        let bell: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let bell_thread = bell.clone();
        let wake = repaint::Wake::default();
        let notify = wake.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if buf[..n].contains(&0x07) {
                            bell_thread.store(true, Ordering::SeqCst);
                        }
                        if let Ok(mut g) = log_thread.lock() {
                            if let Some(f) = g.as_mut() {
                                let _ = f.write_all(&buf[..n]);
                            }
                        }
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                        notify.output_ready();
                    }
                }
            }
            notify.request();
        });
        let provider: Arc<dyn ConnectionProvider> = Arc::from(provider);
        let events = Arc::new(Mutex::new(Vec::new()));
        let term = Term::new(
            TermConfig {
                scrolling_history: SCROLLBACK_LINES,
                ..Default::default()
            },
            &Size { cols, rows },
            Listener {
                events: events.clone(),
            },
        );
        Session {
            id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            title,
            provider,
            rx,
            processor: Processor::new(),
            term,
            cols,
            rows,
            exited: false,
            log,
            log_path: None,
            bell,
            saved_id,
            origin,
            reconnect_tried: false,
            connected_at: Instant::now(),
            broadcast_selected: true,
            backspace: 0x7f,
            sel_anchor: None,
            sel_head: None,
            sel_mode: SelMode::Char,
            selecting: false,
            sel_scroll_at: None,
            scroll_pixels: 0.0,
            cell_width: 8.0,
            cell_height: 17.0,
            reported_focus: false,
            mouse_cell: None,
            events,
            opening: false,
            connection_error: None,
            wake,
            io_status: None,
        }
    }

    fn row_text(&self, line: i32) -> Vec<char> {
        let text = self.term.bounds_to_string(
            Point::new(Line(line), Column(0)),
            Point::new(Line(line), Column(self.cols.saturating_sub(1))),
        );
        text.chars().collect()
    }

    fn word_bounds(&self, line: i32, column: usize) -> (usize, usize) {
        let chars = self.row_text(line);
        if chars.is_empty() {
            return (0, 0);
        }
        let idx = column.min(chars.len() - 1);
        let is_separator = |ch: char| ch.is_whitespace() || "()[]{}\"'`,;".contains(ch);
        if is_separator(chars[idx]) {
            return (idx, idx);
        }
        let mut start = idx;
        while start > 0 && !is_separator(chars[start - 1]) {
            start -= 1;
        }
        let mut end = idx;
        while end + 1 < chars.len() && !is_separator(chars[end + 1]) {
            end += 1;
        }
        (start, end)
    }

    fn expanded(&self, point: Point) -> (Point, Point) {
        match self.sel_mode {
            SelMode::Char => (point, point),
            SelMode::Word => {
                let (start, end) = self.word_bounds(point.line.0, point.column.0);
                (
                    Point::new(point.line, Column(start)),
                    Point::new(point.line, Column(end)),
                )
            }
            SelMode::Line => (
                Point::new(point.line, Column(0)),
                Point::new(point.line, Column(self.cols.saturating_sub(1))),
            ),
        }
    }

    fn selection(&self) -> Option<(Point, Point)> {
        let anchor = self.sel_anchor?;
        let head = self.sel_head?;
        let (a_start, a_end) = self.expanded(anchor);
        let (b_start, b_end) = self.expanded(head);
        let start = if (a_start.line.0, a_start.column.0) <= (b_start.line.0, b_start.column.0) {
            a_start
        } else {
            b_start
        };
        let end = if (a_end.line.0, a_end.column.0) >= (b_end.line.0, b_end.column.0) {
            a_end
        } else {
            b_end
        };
        Some((start, end))
    }

    fn selection_text(&self) -> String {
        self.selection()
            .map(|(start, end)| self.term.bounds_to_string(start, end))
            .unwrap_or_default()
    }

    fn clear_selection(&mut self) {
        self.sel_anchor = None;
        self.sel_head = None;
        self.sel_mode = SelMode::Char;
        self.selecting = false;
        self.sel_scroll_at = None;
    }

    fn toggle_log(&mut self) {
        if self.log_path.is_some() {
            if let Ok(mut g) = self.log.lock() {
                if let Some(f) = g.as_mut() {
                    let _ = f.flush();
                }
                *g = None;
            }
            self.log_path = None;
            return;
        }
        let dir = data_dir().join("logs");
        let _ = std::fs::create_dir_all(&dir);
        let ts = chrono_like_timestamp();
        let safe: String = self
            .title
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || ".-_".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = dir.join(format!("{safe}_{ts}.log"));
        let mut opts = std::fs::OpenOptions::new();
        opts.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        if let Ok(file) = opts.open(&path) {
            if let Ok(mut g) = self.log.lock() {
                *g = Some(file);
            }
            self.log_path = Some(path.to_string_lossy().into_owned());
        }
    }

    fn term_text(&self) -> String {
        let content = self.term.renderable_content();
        let mut out = String::new();
        let mut cur = i32::MIN;
        for indexed in content.display_iter {
            let p = indexed.point;
            if p.line.0 != cur {
                if cur != i32::MIN {
                    out.push('\n');
                }
                cur = p.line.0;
            }
            out.push(indexed.cell.c);
        }
        out
    }

    fn provider(&self) -> Arc<dyn ConnectionProvider> {
        self.provider.clone()
    }

    fn pump(&mut self) {
        if self.opening {
            return;
        }
        if let Some(error) = self
            .io_status
            .as_ref()
            .and_then(|status| status.take_error())
        {
            self.connection_error = Some(error);
        }
        let started = Instant::now();
        let mut received = false;
        for _ in 0..8 {
            let Ok(chunk) = self.rx.try_recv() else {
                break;
            };
            received = true;
            self.processor.advance(&mut self.term, &chunk);
            if started.elapsed() >= Duration::from_millis(1) {
                break;
            }
        }
        // A finite burst may leave queued chunks after the reader's last wake.
        // Keep draining on the next frame rather than waiting for the idle clock.
        if received {
            self.wake.output_ready();
        }
        if !self.provider.is_alive() {
            self.exited = true;
        }
    }

    fn scroll_by_pixels(&mut self, pixels: f32, cell_height: f32) {
        let old_offset = self.term.grid().display_offset() as f32;
        let target = (old_offset + (self.scroll_pixels + pixels) / cell_height)
            .clamp(0.0, self.term.history_size() as f32);
        let offset = target.floor();
        self.term
            .scroll_display(alacritty_terminal::grid::Scroll::Delta(
                offset as i32 - old_offset as i32,
            ));
        self.scroll_pixels = (target - offset) * cell_height;
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        if cols == 0 || rows == 0 || (cols == self.cols && rows == self.rows) {
            return;
        }
        if self
            .provider
            .resize(TerminalDimensions {
                cols: cols as u16,
                rows: rows as u16,
            })
            .is_err()
        {
            // Keep the cached size unchanged so the next frame retries.
            return;
        }
        self.scroll_pixels = 0.0;
        self.cols = cols;
        self.rows = rows;
        self.term.resize(Size { cols, rows });
    }

    fn write(&self, data: &[u8]) -> opsterm_core::error::AppResult<()> {
        self.provider.write(data)
    }

    fn key_bytes(&self, key: egui::Key, modifiers: egui::Modifiers) -> Option<Vec<u8>> {
        if key == egui::Key::Backspace {
            if modifiers.ctrl || modifiers.command {
                return Some(vec![0x08]);
            }
            let mut out = Vec::with_capacity(2);
            if modifiers.alt {
                out.push(0x1b);
            }
            out.push(self.backspace);
            return Some(out);
        }
        key_to_bytes(key, modifiers, *self.term.mode())
    }

    fn drain_events(&self) -> Vec<Event> {
        self.events
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.provider.kill();
    }
}

#[derive(Clone)]
struct EditorState {
    id: Option<String>,
    folder_id: Option<String>,
    name: String,
    host: String,
    port: u16,
    protocol: String,
    username: String,
    credential: String,
    pager: String,
    backspace: String,
    login_script: String,
    error: Option<String>,
}

impl EditorState {
    fn new() -> Self {
        EditorState {
            id: None,
            folder_id: None,
            name: String::new(),
            host: String::new(),
            port: 22,
            protocol: "ssh".into(),
            username: String::new(),
            credential: String::new(),
            pager: "none".into(),
            backspace: "del".into(),
            login_script: String::new(),
            error: None,
        }
    }
    fn from_saved(sv: &SavedSession) -> Self {
        let ov = sv
            .settings_override
            .clone()
            .unwrap_or(serde_json::json!({}));
        EditorState {
            error: None,
            id: Some(sv.id.clone()),
            folder_id: sv.folder_id.clone(),
            name: sv.name.clone(),
            host: sv.host.clone(),
            port: sv.port,
            protocol: sv.protocol.clone(),
            username: sv.username.clone().unwrap_or_default(),
            credential: sv.credential_ref.clone().unwrap_or_default(),
            pager: ov
                .get("pager")
                .and_then(|v| v.as_str())
                .unwrap_or("none")
                .to_string(),
            backspace: ov
                .get("backspace")
                .and_then(|v| v.as_str())
                .unwrap_or("del")
                .to_string(),
            login_script: ov
                .get("loginScript")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }
    }
}

struct Vendor {
    label: &'static str,
    pager: &'static str,
    backspace: &'static str,
    login: &'static str,
    commands: &'static [(&'static str, &'static str)],
    highlights: &'static [(&'static str, [u8; 3])],
}

const VENDORS: &[Vendor] = &[
    Vendor {
        label: "Huawei VRP",
        pager: "huawei",
        backspace: "bs",
        login: "waitfor [#>\\]]\\s*$\nsleep 200\n# sendline enable\n# waitfor [Pp]assword:\n# sendline <secret>",
        commands: &[
            ("Huawei: display version", "display version"),
            ("Huawei: display interface brief", "display interface brief"),
            ("Huawei: display current-configuration", "display current-configuration"),
        ],
        highlights: &[("down", [224, 108, 117]), ("error", [224, 108, 117])],
    },
    Vendor {
        label: "Cisco IOS",
        pager: "cisco",
        backspace: "bs",
        login: "# waitfor [#>]\\s*$\n# sendline enable\n# waitfor [Pp]assword:\n# sendline <secret>",
        commands: &[
            ("Cisco: show version", "show version"),
            ("Cisco: show ip interface brief", "show ip interface brief"),
            ("Cisco: show running-config", "show running-config"),
        ],
        highlights: &[("down", [224, 108, 117]), ("error", [224, 108, 117])],
    },
    Vendor {
        label: "Juniper Junos",
        pager: "juniper",
        backspace: "del",
        login: "# waitfor [>#]\\s*$",
        commands: &[
            ("Junos: show version", "show version"),
            ("Junos: show interfaces terse", "show interfaces terse"),
            ("Junos: show bgp summary", "show bgp summary"),
        ],
        highlights: &[("bgp", [110, 139, 255]), ("error", [224, 108, 117])],
    },
    Vendor {
        label: "MikroTik RouterOS",
        pager: "mikrotik",
        backspace: "del",
        login: "# waitfor [>]\\s*$",
        commands: &[
            ("RouterOS: export", "/export"),
            ("RouterOS: interface print", "/interface print"),
        ],
        highlights: &[("failure", [224, 108, 117])],
    },
    Vendor {
        label: "ZTE OLT",
        pager: "none",
        backspace: "bs",
        login: "waitfor [>#]\\s*$\nsendline terminal length 0",
        commands: &[("ZTE: show version", "show version")],
        highlights: &[("down", [224, 108, 117])],
    },
    Vendor {
        label: "Linux / shell",
        pager: "none",
        backspace: "del",
        login: "# waitfor [$#]\\s*$",
        commands: &[
            ("Shell: uptime", "uptime"),
            ("Shell: disk usage", "df -h"),
        ],
        highlights: &[("error", [224, 108, 117]), ("failed", [224, 108, 117])],
    },
];

fn serial_spec_from_saved(saved: &SavedSession) -> SerialSpec {
    let mut spec = SerialSpec {
        device: saved.host.clone(),
        baud: 9600,
        data_bits: 8,
        parity: "none".into(),
        stop_bits: 1,
        flow_control: "none".into(),
        newline: "crlf".into(),
    };
    if let Some(ser) = saved
        .settings_override
        .as_ref()
        .and_then(|o| o.get("serial"))
    {
        if let Some(v) = ser.get("baud").and_then(|v| v.as_u64()) {
            spec.baud = v as u32;
        }
        if let Some(v) = ser.get("dataBits").and_then(|v| v.as_u64()) {
            spec.data_bits = v as u8;
        }
        if let Some(v) = ser.get("parity").and_then(|v| v.as_str()) {
            spec.parity = v.to_string();
        }
        if let Some(v) = ser.get("stopBits").and_then(|v| v.as_u64()) {
            spec.stop_bits = v as u8;
        }
        if let Some(v) = ser.get("flowControl").and_then(|v| v.as_str()) {
            spec.flow_control = v.to_string();
        }
        if let Some(v) = ser.get("newline").and_then(|v| v.as_str()) {
            spec.newline = v.to_string();
        }
    }
    spec
}

fn normalize_path(p: &str) -> String {
    let mut out = String::from("/");
    let segs: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    for (i, seg) in segs.iter().enumerate() {
        out.push_str(seg);
        if i + 1 < segs.len() {
            out.push('/');
        }
    }
    out
}

fn subnet_calc(ip: &str, prefix: u32) -> Option<String> {
    let addr: std::net::Ipv4Addr = ip.trim().parse().ok()?;
    if prefix > 32 {
        return None;
    }
    let ip_u = u32::from(addr);
    let mask: u32 = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    let network = ip_u & mask;
    let broadcast = network | !mask;
    let count = 1u64 << (32 - prefix);
    let first = if prefix >= 31 { network } else { network + 1 };
    let last = if prefix >= 31 {
        broadcast
    } else {
        broadcast - 1
    };
    Some(format!(
        "Network:   {}/{}\nMask:      {}\nBroadcast: {}\nRange:     {} - {}\nHosts:     {} usable ({} total)",
        std::net::Ipv4Addr::from(network),
        prefix,
        std::net::Ipv4Addr::from(mask),
        std::net::Ipv4Addr::from(broadcast),
        std::net::Ipv4Addr::from(first),
        std::net::Ipv4Addr::from(last),
        count.saturating_sub(2),
        count,
    ))
}

#[derive(Clone)]
enum Step {
    Send(String, bool),
    WaitFor(String, u64),
    Sleep(u64),
}

struct Runner {
    target: u64,
    steps: Vec<Step>,
    index: usize,
    deadline: Option<Instant>,
}

fn parse_expect(script: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    for raw in script.split('\n') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (cmd, arg) = match line.find(char::is_whitespace) {
            Some(i) => (line[..i].to_lowercase(), line[i..].trim().to_string()),
            None => (line.to_lowercase(), String::new()),
        };
        match cmd.as_str() {
            "send" => steps.push(Step::Send(arg, false)),
            "sendline" => steps.push(Step::Send(arg, true)),
            "waitfor" => {
                let parts: Vec<&str> = arg.split_whitespace().collect();
                let has_to = parts
                    .last()
                    .map(|p| p.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false)
                    && parts.len() > 1;
                let (pat, to) = if has_to {
                    (
                        parts[..parts.len() - 1].join(" "),
                        parts[parts.len() - 1].parse().unwrap_or(10000),
                    )
                } else {
                    (arg.clone(), 10000)
                };
                steps.push(Step::WaitFor(pat, to));
            }
            "sleep" => steps.push(Step::Sleep(arg.parse().unwrap_or(0))),
            _ => {}
        }
    }
    steps
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct UiSettings {
    terminal_appearance: appearance::Appearance,
    theme: String,
    font_size: f32,
    auto_reconnect: bool,
    copy_on_select: bool,
    scroll_speed: f32,
    #[serde(default = "default_terminal_scroll_speed")]
    terminal_scroll_speed: f32,
    vault_ttl_secs: u64,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            terminal_appearance: appearance::Appearance::default(),
            theme: THEME_NAMES[0].into(),
            font_size: DEFAULT_TERM_FONT,
            auto_reconnect: false,
            copy_on_select: false,
            scroll_speed: DEFAULT_SCROLL_SPEED,
            terminal_scroll_speed: DEFAULT_TERMINAL_SCROLL_SPEED,
            vault_ttl_secs: DEFAULT_VAULT_TTL_SECS,
        }
    }
}

#[derive(Clone)]
struct FolderPick {
    index: usize,
    name: String,
    filter: String,
}

struct CredEditorState {
    original: Option<String>,
    name: String,
    username: String,
    kind: String,
    tags: String,
    secret: String,
    reveal: bool,
    error: Option<String>,
}

impl CredEditorState {
    fn new() -> Self {
        Self {
            original: None,
            name: String::new(),
            username: String::new(),
            kind: "password".into(),
            tags: String::new(),
            secret: String::new(),
            reveal: false,
            error: None,
        }
    }

    fn from_record(record: &opsterm_core::domain::CredentialRecord) -> Self {
        Self {
            original: Some(record.name.clone()),
            name: record.name.clone(),
            username: record.username.clone().unwrap_or_default(),
            kind: record.kind.clone(),
            tags: record.tags.join(" "),
            secret: String::new(),
            reveal: false,
            error: None,
        }
    }
}

struct App {
    db: Database,
    context: Option<egui::Context>,
    styled_theme: Option<usize>,
    devices: Vec<SavedSession>,
    device_indices: Vec<usize>,
    applied_filter: String,
    composer_open: bool,
    runtime_rss_mib: usize,
    last_memory_check: Instant,
    heap_trim_at: Option<Instant>,
    last_bell: Option<Instant>,
    last_flash: String,
    flash_until: Instant,
    sessions: Vec<Session>,
    active: usize,
    terminal_focus: Option<u64>,
    close_confirmation: close_confirmation::Confirmation,
    allow_close: bool,
    close_input_blocked: bool,
    close_blocked_frame: Option<u64>,
    quick: String,
    command: String,
    font_size: f32,
    fonts_ready: bool,
    terminal_fonts: Vec<appearance::FontOption>,
    scratch: Vec<String>,
    scratch_input: String,
    show_scratch: bool,
    highlight: bool,
    broadcast: bool,
    commands: Vec<Command>,
    editor: Option<EditorState>,
    import_open: bool,
    import_text: String,
    import_msg: String,
    search: String,
    search_open: bool,
    tunnels: TunnelManager,
    show_tunnels: bool,
    t_kind: String,
    t_bind: String,
    t_bind_port: u16,
    t_dest: String,
    t_dest_port: u16,
    t_session: usize,
    show_sftp: bool,
    sftp_session: usize,
    sftp_path: String,
    sftp_entries: Vec<SftpEntry>,
    sftp_location: Option<(String, String)>,
    sftp_msg: String,
    sftp_pending: Option<remote_files::Pending>,
    pending_connections: Vec<connections::Pending>,
    queued_connections: VecDeque<connections::Queued>,
    sftp_new: String,
    split: bool,
    grid: bool,
    auto_reconnect: bool,
    hl_rules: Vec<(String, [u8; 3], bool)>,
    show_hl: bool,
    hl_new: String,
    hl_color: [u8; 3],
    theme_idx: usize,
    runners: Vec<Runner>,
    pump_cursor: usize,
    show_runbooks: bool,
    runbooks: Vec<Runbook>,
    rb_id: Option<String>,
    rb_name: String,
    rb_script: String,
    show_subnet: bool,
    sub_ip: String,
    sub_prefix: u32,
    show_ping: bool,
    ping_host: String,
    ping_count: u32,
    ping_out: String,
    ping_rx: Option<Receiver<String>>,
    show_trace: bool,
    trace_host: String,
    trace_out: String,
    trace_rx: Option<Receiver<String>>,
    show_dns: bool,
    dns_host: String,
    dns_out: String,
    dns_rx: Option<Receiver<String>>,
    flash: String,
    show_targets: bool,
    show_help: bool,
    dev_filter: String,
    split_ratio: f32,
    vendor_idx: usize,
    show_ws: bool,
    workspaces: Vec<Workspace>,
    ws_name: String,
    ws_msg: String,
    folders: Vec<Folder>,
    folder_editor: Option<FolderEditorState>,
    folder_delete: Option<Folder>,
    updater: update::Updater,
    ui_settings: UiSettings,
    scroll_speed: f32,
    terminal_scroll_speed: f32,
    applied_scroll_speed: f32,
    show_settings: bool,
    show_credentials: bool,
    vault: Option<opsterm_core::vault::Vault>,
    vault_prompt: bool,
    vault_password: String,
    vault_new_password: String,
    vault_new_confirm: String,
    vault_error: Option<String>,
    vault_msg: Option<String>,
    vault_ttl_secs: u64,
    vault_import_keyring: bool,
    pending_vault_connect: Option<SavedSession>,
    cred_editor: Option<CredEditorState>,
    cred_delete: Option<String>,
    folder_pick: Option<FolderPick>,
    toast: Option<(String, Instant)>,
}

struct FolderEditorState {
    id: Option<String>,
    parent_id: Option<String>,
    name: String,
    error: Option<String>,
}

impl FolderEditorState {
    fn new(parent_id: Option<String>) -> Self {
        Self {
            id: None,
            parent_id,
            name: String::new(),
            error: None,
        }
    }

    fn rename(folder: &Folder) -> Self {
        Self {
            id: Some(folder.id.clone()),
            parent_id: folder.parent_id.clone(),
            name: folder.name.clone(),
            error: None,
        }
    }
}

impl App {
    fn new(db: Database) -> Self {
        let mut app = App {
            db,
            context: None,
            styled_theme: None,
            devices: Vec::new(),
            device_indices: Vec::new(),
            applied_filter: String::new(),
            composer_open: false,
            runtime_rss_mib: 0,
            last_memory_check: Instant::now(),
            heap_trim_at: Some(Instant::now() + Duration::from_secs(2)),
            last_bell: None,
            last_flash: String::new(),
            flash_until: Instant::now(),
            sessions: Vec::new(),
            active: 0,
            terminal_focus: None,
            close_confirmation: close_confirmation::Confirmation::default(),
            allow_close: false,
            close_input_blocked: false,
            close_blocked_frame: None,
            quick: String::new(),
            command: String::new(),
            font_size: DEFAULT_TERM_FONT,
            fonts_ready: false,
            terminal_fonts: appearance::font_options(),
            scratch: load_scratch(),
            scratch_input: String::new(),
            show_scratch: false,
            highlight: true,
            broadcast: false,
            commands: Vec::new(),
            editor: None,
            import_open: false,
            import_text: String::new(),
            import_msg: String::new(),
            search: String::new(),
            search_open: false,
            tunnels: TunnelManager::new(),
            show_tunnels: false,
            t_kind: "local".into(),
            t_bind: "127.0.0.1".into(),
            t_bind_port: 8080,
            t_dest: String::new(),
            t_dest_port: 80,
            t_session: 0,
            show_sftp: false,
            sftp_session: 0,
            sftp_path: "/".into(),
            sftp_entries: Vec::new(),
            sftp_location: None,
            sftp_msg: String::new(),
            sftp_pending: None,
            pending_connections: Vec::new(),
            queued_connections: VecDeque::new(),
            sftp_new: String::new(),
            split: false,
            grid: false,
            auto_reconnect: false,
            hl_rules: vec![
                ("down".into(), [224, 108, 117], true),
                ("error".into(), [224, 108, 117], true),
                ("failed".into(), [224, 108, 117], true),
                ("denied".into(), [224, 108, 117], true),
                ("invalid".into(), [224, 108, 117], true),
                ("warning".into(), [229, 192, 123], true),
                ("up".into(), [152, 195, 121], true),
                ("bgp".into(), [110, 139, 255], true),
                ("ospf".into(), [110, 139, 255], true),
                ("isis".into(), [110, 139, 255], true),
                ("evpn".into(), [110, 139, 255], true),
            ],
            show_hl: false,
            hl_new: String::new(),
            hl_color: [229, 192, 123],
            theme_idx: 0,
            runners: Vec::new(),
            pump_cursor: 0,
            show_runbooks: false,
            runbooks: Vec::new(),
            rb_id: None,
            rb_name: String::new(),
            rb_script: String::new(),
            show_subnet: false,
            sub_ip: "192.168.1.10".into(),
            sub_prefix: 24,
            show_ping: false,
            ping_host: String::new(),
            ping_count: 4,
            ping_out: String::new(),
            ping_rx: None,
            show_trace: false,
            trace_host: String::new(),
            trace_out: String::new(),
            trace_rx: None,
            show_dns: false,
            dns_host: String::new(),
            dns_out: String::new(),
            dns_rx: None,
            flash: String::new(),
            show_targets: false,
            show_help: false,
            dev_filter: String::new(),
            split_ratio: 0.5,
            vendor_idx: 0,
            show_ws: false,
            workspaces: Vec::new(),
            ws_name: String::new(),
            ws_msg: String::new(),
            folders: Vec::new(),
            folder_editor: None,
            folder_delete: None,
            updater: update::Updater::new(),
            ui_settings: UiSettings::default(),
            scroll_speed: DEFAULT_SCROLL_SPEED,
            terminal_scroll_speed: DEFAULT_TERMINAL_SCROLL_SPEED,
            applied_scroll_speed: 0.0,
            show_settings: false,
            show_credentials: false,
            vault: None,
            vault_prompt: false,
            vault_password: String::new(),
            vault_new_password: String::new(),
            vault_new_confirm: String::new(),
            vault_error: None,
            vault_msg: None,
            vault_ttl_secs: DEFAULT_VAULT_TTL_SECS,
            vault_import_keyring: true,
            pending_vault_connect: None,
            cred_editor: None,
            cred_delete: None,
            folder_pick: None,
            toast: None,
        };
        app.commands = app.db.list_commands().unwrap_or_default();
        app.refresh_folders();
        app.refresh_devices();
        app.load_ui_settings();
        app
    }

    fn load_ui_settings(&mut self) {
        let settings = self
            .db
            .load_named_setting::<UiSettings>("ui.egui")
            .ok()
            .flatten()
            .unwrap_or_default();
        self.font_size = settings.font_size.clamp(10.0, 30.0);
        self.auto_reconnect = settings.auto_reconnect;
        self.scroll_speed = settings.scroll_speed.clamp(40.0, 200.0);
        self.terminal_scroll_speed = settings.terminal_scroll_speed.clamp(40.0, 320.0);
        self.theme_idx = theme_index(&settings.theme);
        set_theme(theme_for(self.theme_idx));
        self.vault_ttl_secs = settings.vault_ttl_secs.max(60);
        self.ui_settings = settings;
        self.ui_settings
            .terminal_appearance
            .normalize(&self.terminal_fonts);
        self.ui_settings.theme = theme_name(self.theme_idx).into();
        let path = vault::default_path();
        if vault::is_initialized_at(&path) {
            match Vault::resume(&path) {
                Ok(Some(unlocked)) => {
                    self.vault_ttl_secs = unlocked.ttl_secs();
                    self.vault = Some(unlocked);
                }
                Ok(None) | Err(_) => {
                    self.vault_prompt = true;
                }
            }
        }
    }

    fn save_ui_settings(&mut self) {
        self.ui_settings.theme = theme_name(self.theme_idx).into();
        self.ui_settings.font_size = self.font_size;
        self.ui_settings.auto_reconnect = self.auto_reconnect;
        self.ui_settings.scroll_speed = self.scroll_speed;
        self.ui_settings.terminal_scroll_speed = self.terminal_scroll_speed;
        self.ui_settings.vault_ttl_secs = self.vault_ttl_secs;
        let _ = self.db.save_named_setting("ui.egui", &self.ui_settings);
    }

    fn copy_to_clipboard(&mut self, ctx: &egui::Context, text: String) {
        if text.is_empty() {
            return;
        }
        ctx.copy_text(text);
        self.toast = Some(("Copied to clipboard".into(), Instant::now()));
    }

    fn check_vault_expiry(&mut self) {
        if self.vault.as_ref().is_some_and(|vault| vault.is_expired()) {
            self.lock_vault();
            self.vault_msg = Some("Vault locked (session expired)".into());
            self.toast = Some(("Vault locked (session expired)".into(), Instant::now()));
        }
    }

    fn lock_vault(&mut self) {
        if let Some(mut vault) = self.vault.take() {
            vault.lock();
        }
        self.cred_editor = None;
        self.vault_msg = Some("Vault locked".into());
    }

    fn try_unlock_vault(&mut self) {
        let path = vault::default_path();
        match Vault::unlock(&path, &self.vault_password) {
            Ok(unlocked) => {
                self.vault_ttl_secs = unlocked.ttl_secs();
                self.vault = Some(unlocked);
                self.vault_password.clear();
                self.vault_error = None;
                self.vault_msg = Some("Vault unlocked".into());
                self.vault_prompt = false;
                if let Some(saved) = self.pending_vault_connect.take() {
                    self.open_saved_new(&saved);
                }
            }
            Err(error) => self.vault_error = Some(error.to_string()),
        }
    }

    fn create_vault(&mut self) {
        if self.vault_new_password != self.vault_new_confirm {
            self.vault_error = Some("Master password confirmation does not match".into());
            return;
        }
        let path = vault::default_path();
        match Vault::create(&path, &self.vault_new_password, self.vault_ttl_secs) {
            Ok(mut unlocked) => {
                self.vault_ttl_secs = unlocked.ttl_secs();
                let imported = if self.vault_import_keyring {
                    let names: Vec<String> = self
                        .db
                        .list_credentials()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|record| record.name)
                        .collect();
                    let (imported, failed) =
                        opsterm_core::credential::import_keyring_into(&mut unlocked, &names);
                    Some((imported, failed))
                } else {
                    None
                };
                self.vault = Some(unlocked);
                self.vault_new_password.clear();
                self.vault_new_confirm.clear();
                self.vault_error = None;
                self.vault_msg = Some(match imported {
                    Some((imported, 0)) => {
                        format!("Vault created · imported {imported} credential(s)")
                    }
                    Some((imported, failed)) => format!(
                        "Vault created · imported {imported} credential(s), {failed} without an OS keyring secret"
                    ),
                    None => "Vault created · keyring credentials left untouched".into(),
                });
                self.toast = Some(("Vault created".into(), Instant::now()));
                self.save_ui_settings();
                if let Some(saved) = self.pending_vault_connect.take() {
                    self.open_saved_new(&saved);
                }
            }
            Err(error) => self.vault_error = Some(error.to_string()),
        }
    }

    fn import_credentials(&mut self) {
        let names: Vec<String> = self
            .db
            .list_credentials()
            .unwrap_or_default()
            .into_iter()
            .map(|record| record.name)
            .collect();
        if let Some(unlocked) = self.vault.as_mut() {
            let (imported, failed) =
                opsterm_core::credential::import_keyring_into(unlocked, &names);
            self.vault_msg = Some(if failed == 0 {
                format!("Imported {imported} credential(s) from the OS keyring")
            } else {
                format!("Imported {imported} credential(s); {failed} had no keyring secret")
            });
        }
    }

    fn change_master_password(&mut self) {
        if self.vault_new_password != self.vault_new_confirm {
            self.vault_error = Some("New password confirmation does not match".into());
            return;
        }
        let result = self
            .vault
            .as_mut()
            .map(|unlocked| unlocked.change_password(&self.vault_new_password));
        match result {
            Some(Ok(())) => {
                self.vault_new_password.clear();
                self.vault_new_confirm.clear();
                self.vault_error = None;
                self.vault_msg = Some("Master password updated".into());
                self.toast = Some(("Master password updated".into(), Instant::now()));
            }
            Some(Err(error)) => self.vault_error = Some(error.to_string()),
            None => self.vault_error = Some("Vault is locked".into()),
        }
    }

    fn vault_panel(&mut self, ui: &mut egui::Ui) {
        let initialized = vault::is_initialized();
        if self.vault.is_some() {
            let until = self.vault.as_ref().and_then(|vault| vault.unlock_until());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Vault unlocked").strong().color(ok()));
                if let Some(until) = until {
                    ui.label(
                        egui::RichText::new(format!(
                            "until {}",
                            until.format("%Y-%m-%d %H:%M UTC")
                        ))
                        .small()
                        .color(muted()),
                    );
                }
            });
            ui.horizontal(|ui| {
                ui.label("Stay unlocked for");
                let mut ttl = self.vault_ttl_secs;
                egui::ComboBox::from_id_salt("vault-ttl")
                    .selected_text(ttl_label(ttl))
                    .show_ui(ui, |ui| {
                        for choice in vault::TTL_CHOICES {
                            ui.selectable_value(&mut ttl, choice, ttl_label(choice));
                        }
                    });
                if ttl != self.vault_ttl_secs {
                    self.vault_ttl_secs = ttl;
                    if let Some(unlocked) = self.vault.as_mut() {
                        unlocked.set_ttl(ttl);
                    }
                    self.save_ui_settings();
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Lock now").clicked() {
                    self.lock_vault();
                }
                if ui.button("Import from OS keyring").clicked() {
                    self.import_credentials();
                }
            });
            ui.collapsing("Change master password", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.vault_new_password)
                        .password(true)
                        .hint_text("New master password")
                        .desired_width(260.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.vault_new_confirm)
                        .password(true)
                        .hint_text("Confirm new master password")
                        .desired_width(260.0),
                );
                if ui.button("Change password").clicked() {
                    self.change_master_password();
                }
            });
        } else if initialized {
            ui.label(egui::RichText::new("Vault locked").strong().color(accent()));
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.vault_password)
                        .password(true)
                        .hint_text("Master password")
                        .desired_width(240.0),
                );
                let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Unlock").clicked() || enter {
                    self.try_unlock_vault();
                }
            });
        } else {
            ui.label(
                "No credential vault yet. Create one to encrypt credentials with a master password. Existing keyring secrets are imported and removed from the OS keyring.",
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.vault_new_password)
                    .password(true)
                    .hint_text("Master password (min. 8 characters)")
                    .desired_width(280.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.vault_new_confirm)
                    .password(true)
                    .hint_text("Confirm master password")
                    .desired_width(280.0),
            );
            ui.horizontal(|ui| {
                ui.label("Auto-lock after");
                let mut ttl = self.vault_ttl_secs;
                egui::ComboBox::from_id_salt("vault-create-ttl")
                    .selected_text(ttl_label(ttl))
                    .show_ui(ui, |ui| {
                        for choice in vault::TTL_CHOICES {
                            ui.selectable_value(&mut ttl, choice, ttl_label(choice));
                        }
                    });
                self.vault_ttl_secs = ttl;
            });
            ui.checkbox(
                &mut self.vault_import_keyring,
                "Import existing credentials and remove their OS keyring copies",
            );
            ui.label(
                egui::RichText::new(
                    "No recovery: if you forget the master password, stored secrets cannot be recovered.",
                )
                .small()
                .color(Color32::from_rgb(229, 192, 123)),
            );
            if ui.button("Create vault").clicked() {
                self.create_vault();
            }
        }
        if let Some(error) = &self.vault_error {
            ui.colored_label(Color32::from_rgb(224, 108, 117), error);
        }
        if let Some(message) = &self.vault_msg {
            ui.colored_label(ok(), message);
        }
    }

    fn install_selected_fonts(&mut self, ctx: &egui::Context) {
        let path = self
            .terminal_fonts
            .iter()
            .find(|choice| choice.font == self.ui_settings.terminal_appearance.font)
            .and_then(|choice| choice.path.as_deref());
        install_fonts(ctx, path);
        self.fonts_ready = true;
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        egui::Window::new("Settings")
            .open(&mut open)
            .resizable(false)
            .vscroll(true)
            .default_height(640.0)
            .constrain_to(ctx.content_rect())
            .max_height((ctx.content_rect().height() - 80.0).max(180.0))
            .default_width(560.0)
            .show(ctx, |ui| {
                let mut changed = false;
                ui.heading("Appearance");
                ui.horizontal(|ui| {
                    ui.label("UI theme");
                    egui::ComboBox::from_id_salt("settings-theme")
                        .selected_text(theme_name(self.theme_idx))
                        .show_ui(ui, |ui| {
                            for (index, name) in THEME_NAMES.iter().enumerate() {
                                if ui
                                    .selectable_value(&mut self.theme_idx, index, *name)
                                    .changed()
                                {
                                    set_theme(theme_for(index));
                                    changed = true;
                                }
                            }
                        });
                });
                let old_font_size = self.font_size;
                let (appearance_changed, font_changed) =
                    self.ui_settings.terminal_appearance.controls(
                        ui,
                        &self.terminal_fonts,
                        &mut self.font_size,
                        bg(),
                        text_c(),
                    );
                changed |= appearance_changed;
                if font_changed {
                    self.install_selected_fonts(ctx);
                    ctx.request_repaint();
                }
                if font_changed || old_font_size != self.font_size {
                    for session in &mut self.sessions {
                        session.scroll_pixels = 0.0;
                    }
                }
                ui.separator();
                ui.heading("Terminal");
                changed |= ui
                    .checkbox(
                        &mut self.ui_settings.copy_on_select,
                        "Copy selection to clipboard automatically",
                    )
                    .changed();
                ui.horizontal(|ui| {
                    ui.label("List / sidebar scroll speed");
                    changed |= ui
                        .add(egui::Slider::new(&mut self.scroll_speed, 40.0..=200.0))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Terminal scroll speed");
                    changed |= ui
                        .add(egui::Slider::new(
                            &mut self.terminal_scroll_speed,
                            40.0..=320.0,
                        ))
                        .changed();
                });
                ui.label(
                    egui::RichText::new(
                        "Scroll speed applies instantly (higher is faster, 60 fps smooth).",
                    )
                    .small()
                    .color(muted()),
                );
                ui.separator();
                ui.heading("General");
                changed |= ui
                    .checkbox(&mut self.auto_reconnect, "Auto-reconnect dropped sessions")
                    .changed();
                ui.separator();
                ui.heading("Security");
                self.vault_panel(ui);
                if changed {
                    self.save_ui_settings();
                }
            });
        if !open {
            self.show_settings = false;
        }
    }

    fn credentials_window(&mut self, ctx: &egui::Context) {
        if !self.show_credentials {
            return;
        }
        let mut open = true;
        egui::Window::new("Credentials")
            .open(&mut open)
            .resizable(true)
            .default_width(840.0)
            .default_height(500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Credential vault").strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.vault.is_some() && ui.button("Lock now").clicked() {
                            self.lock_vault();
                        }
                    });
                });
                ui.separator();
                if self.vault.is_none() {
                    self.vault_panel(ui);
                    ui.separator();
                    let records = self.db.list_credentials().unwrap_or_default();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} existing credential record(s) will be imported when the vault is created",
                            records.len()
                        ))
                        .small()
                        .color(muted()),
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("credential-preview")
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for record in &records {
                                let user = record.username.as_deref().unwrap_or("-");
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}  ·  {user}  ·  {}",
                                        record.name, record.kind
                                    ))
                                    .monospace()
                                    .color(text_c()),
                                );
                            }
                        });
                    return;
                }
                ui.horizontal(|ui| {
                    if ui.button("+ New credential").clicked() {
                        self.cred_editor = Some(CredEditorState::new());
                    }
                    if ui.button("Import from OS keyring").clicked() {
                        self.import_credentials();
                    }
                    if let Some(message) = &self.vault_msg {
                        ui.label(egui::RichText::new(message).small().color(ok()));
                    }
                });
                ui.separator();
                let records = self.db.list_credentials().unwrap_or_default();
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(260.0);
                        egui::ScrollArea::vertical()
                            .id_salt("credential-list")
                            .max_height(370.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if records.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No credentials yet")
                                            .small()
                                            .color(muted()),
                                    );
                                }
                                for record in &records {
                                    let selected = self
                                        .cred_editor
                                        .as_ref()
                                        .and_then(|editor| editor.original.as_deref())
                                        == Some(record.name.as_str());
                                    let label = match record.username.as_deref() {
                                        Some(user) if !user.is_empty() => {
                                            format!("{}  ·  {user}", record.name)
                                        }
                                        _ => record.name.clone(),
                                    };
                                    let response = ui
                                        .selectable_label(selected, label)
                                        .on_hover_text(format!(
                                            "{} · tags: {}",
                                            record.kind,
                                            if record.tags.is_empty() {
                                                "-".into()
                                            } else {
                                                record.tags.join(" ")
                                            }
                                        ));
                                    if response.clicked() {
                                        self.cred_editor =
                                            Some(CredEditorState::from_record(record));
                                    }
                                }
                            });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        self.credential_editor(ui);
                    });
                });
            });
        if !open {
            self.show_credentials = false;
        }
    }

    fn folder_picker_window(&mut self, ctx: &egui::Context) {
        let Some(pick) = self.folder_pick.clone() else {
            return;
        };
        let mut open = true;
        let mut cancel = false;
        let mut move_to = None;
        let mut filter = pick.filter.clone();
        let choices = self.folder_choices();
        egui::Window::new("Move to folder")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(format!("Move “{}” to:", pick.name)).strong());
                ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text("Search folders…")
                        .desired_width(320.0),
                );
                ui.add_space(4.0);
                let needle = filter.trim().to_lowercase();
                egui::ScrollArea::vertical()
                    .id_salt("folder-picker")
                    .max_height(300.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (label, id) in &choices {
                            if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                                continue;
                            }
                            if ui.selectable_label(false, label).clicked() {
                                move_to = Some(id.clone());
                                cancel = true;
                            }
                        }
                    });
                ui.add_space(4.0);
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        if let Some(id) = move_to {
            self.set_device_folder(pick.index, id);
            self.folder_pick = None;
        } else if !open || cancel {
            self.folder_pick = None;
        } else {
            self.folder_pick = Some(FolderPick { filter, ..pick });
        }
    }

    fn credential_editor(&mut self, ui: &mut egui::Ui) {
        let Some(mut editor) = self.cred_editor.take() else {
            ui.label(egui::RichText::new("Select a credential or create a new one").color(muted()));
            return;
        };
        ui.add_enabled_ui(self.vault.is_some(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add_enabled(
                    editor.original.is_none(),
                    egui::TextEdit::singleline(&mut editor.name).desired_width(260.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Username");
                ui.text_edit_singleline(&mut editor.username);
            });
            ui.horizontal(|ui| {
                ui.label("Kind");
                egui::ComboBox::from_id_salt("cred-kind")
                    .selected_text(&editor.kind)
                    .show_ui(ui, |ui| {
                        for kind in ["password", "passphrase", "enable"] {
                            ui.selectable_value(&mut editor.kind, kind.to_string(), kind);
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Tags");
                ui.text_edit_singleline(&mut editor.tags);
            });
        });
        let mut reveal = false;
        ui.horizontal(|ui| {
            ui.label("Secret");
            ui.add(
                egui::TextEdit::singleline(&mut editor.secret)
                    .password(!editor.reveal)
                    .desired_width(260.0)
                    .hint_text(if editor.original.is_some() {
                        "leave blank to keep current"
                    } else {
                        "required"
                    }),
            );
            if editor.original.is_some() && ui.button("Reveal").clicked() {
                reveal = true;
            }
        });
        if reveal {
            if let (Some(unlocked), Some(name)) = (&self.vault, &editor.original) {
                editor.secret = unlocked.get_secret(name).unwrap_or_default();
                editor.reveal = true;
            }
        }
        if let Some(error) = &editor.error {
            ui.colored_label(Color32::from_rgb(224, 108, 117), error);
        }
        let mut save = false;
        let mut delete = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.vault.is_some(), egui::Button::new("Save"))
                .clicked()
            {
                save = true;
            }
            if editor.original.is_some() && ui.button("Delete").clicked() {
                delete = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
        if delete {
            self.cred_delete = editor.original.clone();
        }
        if save {
            let name = editor.name.trim().to_string();
            let tags: Vec<String> = editor
                .tags
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect();
            let username = {
                let user = editor.username.trim();
                if user.is_empty() {
                    None
                } else {
                    Some(user.to_string())
                }
            };
            if name.is_empty() {
                editor.error = Some("Name is required".into());
            } else if editor.original.is_none() && editor.secret.is_empty() {
                editor.error = Some("Secret is required".into());
            } else {
                editor.error = None;
                match self
                    .db
                    .upsert_credential(&name, username.as_deref(), &editor.kind, &tags)
                {
                    Ok(()) => {
                        if !editor.secret.is_empty() {
                            if let Some(unlocked) = self.vault.as_mut() {
                                if let Err(error) = unlocked.set_secret(&name, &editor.secret) {
                                    editor.error = Some(error.to_string());
                                }
                            }
                        }
                        if editor.error.is_none() {
                            self.toast = Some((format!("Credential {name} saved"), Instant::now()));
                            cancel = true;
                        }
                    }
                    Err(error) => editor.error = Some(error.to_string()),
                }
            }
        }
        if cancel {
            self.cred_editor = None;
        } else {
            self.cred_editor = Some(editor);
        }
    }

    fn delete_credential(&mut self, name: &str) {
        let _ = self.db.remove_credential_name(name);
        if let Some(unlocked) = self.vault.as_mut() {
            let _ = unlocked.delete_secret(name);
        }
        let _ = opsterm_core::credential::delete(name);
        if self
            .cred_editor
            .as_ref()
            .and_then(|editor| editor.original.as_deref())
            == Some(name)
        {
            self.cred_editor = None;
        }
        self.toast = Some((format!("Credential {name} deleted"), Instant::now()));
    }

    fn credential_delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(name) = self.cred_delete.clone() else {
            return;
        };
        let mut open = true;
        let mut delete = false;
        let mut cancel = false;
        egui::Window::new("Delete credential")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label(format!("Delete “{name}”?"));
                ui.label(
                    egui::RichText::new(
                        "The secret is removed from the vault and devices using it are set to prompt.",
                    )
                    .small()
                    .color(muted()),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button(
                            egui::RichText::new("Delete")
                                .color(Color32::from_rgb(224, 108, 117)),
                        )
                        .clicked()
                    {
                        delete = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if delete {
            self.delete_credential(&name);
            self.cred_delete = None;
        } else if cancel || !open {
            self.cred_delete = None;
        }
    }

    fn vault_prompt_dialog(&mut self, ctx: &egui::Context) {
        if !self.vault_prompt {
            return;
        }
        let mut open = true;
        let mut dismiss = false;
        egui::Window::new("Unlock vault")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.label("Enter the master password to unlock stored credentials.");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.vault_password)
                        .password(true)
                        .hint_text("Master password")
                        .desired_width(300.0),
                );
                let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button("Unlock").clicked() || enter {
                        self.try_unlock_vault();
                    }
                    if ui.button("Later").clicked() {
                        dismiss = true;
                    }
                });
                if let Some(error) = &self.vault_error {
                    ui.colored_label(Color32::from_rgb(224, 108, 117), error);
                }
            });
        if dismiss || !open {
            self.vault_prompt = false;
            self.pending_vault_connect = None;
        }
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some((message, shown_at)) = self.toast.clone() else {
            return;
        };
        if shown_at.elapsed() >= Duration::from_millis(1800) {
            self.toast = None;
            return;
        }
        let font = FontId::monospace(15.0);
        let galley = ctx.fonts_mut(|fonts| fonts.layout_no_wrap(message, font, text_c()));
        let size = galley.size() + Vec2::new(44.0, 22.0);
        let screen = ctx.content_rect();
        let rect =
            Rect::from_center_size(Pos2::new(screen.center().x, screen.bottom() - 70.0), size);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("opsterm-toast"),
        ));
        painter.rect_filled(rect, 0.0, Color32::from_rgb(0x14, 0x14, 0x14));
        let bar = Color32::from_rgb(0x56, 0xB6, 0xC2);
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
            0.0,
            bar,
        );
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.max.x - 3.0, rect.min.y),
                Vec2::new(3.0, rect.height()),
            ),
            0.0,
            bar,
        );
        painter.galley(rect.center() - galley.size() / 2.0, galley, text_c());
        ctx.request_repaint_after(Duration::from_millis(60));
    }

    fn queue_connection(
        &mut self,
        mut request: connections::Request,
        previous: Option<Arc<dyn ConnectionProvider>>,
    ) {
        if request.target.is_none() {
            let mut tab = Session::new(
                request.title.clone(),
                Box::new(connections::OpeningProvider),
                request.dims.cols as usize,
                request.dims.rows as usize,
                request.saved.as_ref().map(|s| s.id.clone()),
                request.origin.clone(),
            );
            tab.opening = true;
            tab.broadcast_selected = !self.broadcast;
            request.target = Some(tab.id);
            self.terminal_focus = Some(tab.id);
            self.sessions.push(tab);
            self.active = self.sessions.len() - 1;
        } else if let Some(tab) = self
            .sessions
            .iter_mut()
            .find(|s| Some(s.id) == request.target)
        {
            tab.opening = true;
            tab.exited = false;
            tab.connection_error = None;
        }
        self.flash = format!("queued connection to {}…", request.title);
        self.queued_connections
            .push_back(connections::Queued { request, previous });
        self.start_queued_connections();
    }

    fn start_queued_connections(&mut self) {
        while self.pending_connections.len() < connections::MAX_WORKERS {
            let Some(queued) = self.queued_connections.pop_front() else {
                break;
            };
            if let Some(target) = queued.request.target {
                if !self.sessions.iter().any(|s| s.id == target) {
                    continue;
                }
            }
            let target = queued.request.target;
            match connections::Pending::start(queued.request, queued.previous) {
                Ok(pending) => {
                    if let Some(ctx) = &self.context {
                        pending.task.bind(ctx);
                    }
                    self.pending_connections.push(pending);
                }
                Err(e) => {
                    if let Some(tab) = self.sessions.iter_mut().find(|s| Some(s.id) == target) {
                        tab.opening = false;
                        tab.exited = true;
                        tab.reconnect_tried = true;
                        tab.connection_error = Some(e.clone());
                    }
                    self.flash = e;
                }
            }
        }
    }

    fn open_telnet_console(&mut self, spec: TelnetSpec, title: String) {
        // An external console has no inventory credentials to unlock. Protected
        // inventory actions still request the vault through their normal path.
        self.vault_prompt = false;
        self.queue_connection(
            connections::Request {
                title,
                origin: Origin::Telnet(spec),
                saved: None,
                secret: None,
                target: None,
                dims: TerminalDimensions { cols: 80, rows: 24 },
            },
            None,
        );
    }

    fn new_local(&mut self) {
        self.queue_connection(
            connections::Request {
                title: "Local Shell".into(),
                origin: Origin::Local,
                saved: None,
                secret: None,
                target: None,
                dims: TerminalDimensions { cols: 80, rows: 24 },
            },
            None,
        );
    }

    fn spawn_ssh(&mut self, spec: SshSpec, title: String) {
        self.queue_connection(
            connections::Request {
                title,
                origin: Origin::Ssh(spec),
                saved: None,
                secret: None,
                target: None,
                dims: TerminalDimensions { cols: 80, rows: 24 },
            },
            None,
        );
    }

    fn poll_connections(&mut self) {
        for pending in std::mem::take(&mut self.pending_connections) {
            let result = match pending.task.poll() {
                Ok(None) => {
                    self.pending_connections.push(pending);
                    continue;
                }
                Ok(Some(result)) => result,
                Err(e) => Err(e.into()),
            };
            let mut opened = match result {
                Ok(provider) => provider,
                Err(e) => {
                    self.flash = format!("{}: {e}", pending.request.title);
                    if let Some(tab) = self
                        .sessions
                        .iter_mut()
                        .find(|s| Some(s.id) == pending.request.target)
                    {
                        tab.opening = false;
                        tab.exited = true;
                        tab.reconnect_tried = true;
                        tab.connection_error = Some(e.to_string());
                    }
                    continue;
                }
            };
            let request = pending.request;
            let replace = if let Some(target) = request.target {
                let Some(index) = self.sessions.iter().position(|s| s.id == target) else {
                    continue; // OpenedProvider cleans up a result for a closed tab.
                };
                Some(index)
            } else {
                None
            };
            let mut session = Session::new(
                request.title,
                opened.0.take().expect("opened provider"),
                request.dims.cols as usize,
                request.dims.rows as usize,
                request.saved.as_ref().map(|s| s.id.clone()),
                request.origin,
            );
            session.io_status = opened.1.take();
            if let Some(ctx) = &self.context {
                session.wake.bind(ctx);
            }
            if let Some(saved) = request.saved.as_ref() {
                if saved
                    .settings_override
                    .as_ref()
                    .and_then(|v| v.get("backspace"))
                    .and_then(|v| v.as_str())
                    == Some("bs")
                {
                    session.backspace = 0x08;
                }
            }
            if let Some(index) = replace {
                session.id = self.sessions[index].id;
                session.broadcast_selected = self.sessions[index].broadcast_selected;
                self.sessions[index] = session;
            } else {
                // A connection finishing later must not silently join a live broadcast.
                session.broadcast_selected = !self.broadcast;
                self.sessions.push(session);
                self.active = self.sessions.len() - 1;
            }
            let target = request
                .target
                .unwrap_or_else(|| self.sessions.last().unwrap().id);
            self.flash = "connection opened".into();
            if let Some(saved) = request.saved {
                self.apply_pager(&saved, target);
                if let Some(script) = saved
                    .settings_override
                    .as_ref()
                    .and_then(|o| o.get("loginScript"))
                    .and_then(|v| v.as_str())
                {
                    self.start_runner_for(target, script);
                }
            }
        }
        self.start_queued_connections();
    }

    fn open_saved(&mut self, saved: &SavedSession) {
        if let Some(index) = self.sessions.iter().position(|s| {
            s.saved_id.as_deref() == Some(saved.id.as_str()) && (s.opening || !s.exited)
        }) {
            self.active = index;
            self.terminal_focus = Some(self.sessions[index].id);
            return;
        }
        self.open_saved_new(saved);
    }

    fn open_saved_new(&mut self, saved: &SavedSession) {
        let credential_ref = saved.credential_ref.clone().filter(|r| !r.is_empty());
        if let Some(reference) = &credential_ref {
            if self.vault.is_none() && vault::is_locked_for(reference) {
                self.pending_vault_connect = Some(saved.clone());
                self.vault_prompt = true;
                self.vault_error = None;
                return;
            }
        }
        let secret = credential_ref
            .as_ref()
            .and_then(|reference| self.vault.as_ref()?.get_secret(reference));
        let username = saved.username.clone().or_else(|| {
            saved
                .credential_ref
                .as_deref()
                .and_then(|r| self.db.get_credential(r).ok().flatten())
                .and_then(|c| c.username)
        });
        let origin = match saved.protocol.as_str() {
            "ssh" | "" => Origin::Ssh(SshSpec {
                host: saved.host.clone(),
                port: saved.port,
                username,
                identity_file: saved.identity_file.clone(),
                proxy_jump: saved.proxy_jump.clone(),
                name: Some(saved.name.clone()),
                term: Some("xterm-256color".into()),
                connect_timeout: Some(20),
                server_alive_interval: Some(60),
                x11_forwarding: false,
                forward_agent: false,
                compression: false,
                certificate_file: None,
                pkcs11_provider: None,
                engine: Some(
                    saved
                        .settings_override
                        .as_ref()
                        .and_then(|o| o.get("engine"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("openssh")
                        .to_string(),
                ),
                password: None,
            }),
            "telnet" => Origin::Telnet(TelnetSpec {
                host: saved.host.clone(),
                port: if saved.port == 0 { 23 } else { saved.port },
                username,
                term: Some("xterm-256color".into()),
                password: None,
            }),
            "serial" => Origin::Serial(serial_spec_from_saved(saved)),
            "local" => Origin::Local,
            other => {
                self.flash = format!("{other} is not supported in egui");
                return;
            }
        };
        self.queue_connection(
            connections::Request {
                title: saved.name.clone(),
                origin,
                saved: Some(saved.clone()),
                secret,
                target: None,
                dims: TerminalDimensions { cols: 80, rows: 24 },
            },
            None,
        );
    }

    /// Send the vendor pager-disable command shortly after connecting.
    fn apply_pager(&self, saved: &SavedSession, target: u64) {
        if let Some(cmd) = pager_command(&saved.settings_override) {
            if let Some(sess) = self.sessions.iter().find(|s| s.id == target) {
                let prov = sess.provider();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    let mut bytes = cmd.into_bytes();
                    bytes.push(b'\r');
                    let _ = prov.write(&bytes);
                });
            }
        }
    }

    fn connect_quick(&mut self) {
        let spec = match quick_connect::parse(&self.quick) {
            Ok(spec) => spec,
            Err(error) => {
                self.flash = error;
                return;
            }
        };
        let title = spec
            .username
            .as_ref()
            .map(|u| format!("{u}@{}", spec.host))
            .unwrap_or_else(|| spec.host.clone());
        self.spawn_ssh(spec, title);
        self.quick.clear();
    }

    fn send_command(&mut self, with_enter: bool) {
        if self.close_input_blocked || self.close_confirmation.active() {
            return;
        }
        let mut bytes = command_input::normalize(&self.command).into_bytes();
        if with_enter && !bytes.ends_with(b"\r") {
            bytes.push(b'\r');
        }
        if bytes.is_empty() {
            return;
        }
        if self.dispatch_command(&bytes) && with_enter {
            self.command.clear();
        }
    }

    fn dispatch_command(&mut self, bytes: &[u8]) -> bool {
        let targets: Vec<_> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                !s.exited
                    && s.provider.is_alive()
                    && if self.broadcast {
                        s.broadcast_selected
                    } else {
                        *i == self.active
                    }
            })
            .map(|(_, s)| s)
            .collect();
        if targets.is_empty() {
            self.flash = "No connected target selected; command kept in editor".into();
            return false;
        }
        let mut sent = 0;
        let mut failures = Vec::new();
        for target in targets {
            match target.provider.write(bytes) {
                Ok(()) => sent += 1,
                Err(e) => failures.push(format!("{}: {e}", target.title)),
            }
        }
        self.flash = if failures.is_empty() {
            format!("Queued for {sent} session(s)")
        } else {
            format!("Queued for {sent}; failed: {}", failures.join("; "))
        };
        failures.is_empty()
    }

    fn send_raw(&mut self, text: &str) {
        let mut bytes = command_input::normalize(text).into_bytes();
        if !bytes.ends_with(b"\r") {
            bytes.push(b'\r');
        }
        self.dispatch_command(&bytes);
    }

    /// Terminal grid + keyboard input + mouse selection, drawn inside the card.
    fn terminal_pane(&mut self, ui: &mut egui::Ui, idx: usize) {
        if self.sessions.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("No sessions — open a local shell with +").color(muted()),
                );
            });
            return;
        }
        let idx = idx.min(self.sessions.len() - 1);
        if self.sessions[idx.min(self.sessions.len() - 1)].opening {
            let idx = idx.min(self.sessions.len() - 1);
            let title = self.sessions[idx].title.clone();
            let saved = self.sessions[idx]
                .saved_id
                .as_deref()
                .and_then(|id| self.devices.iter().find(|d| d.id == id));
            let subtitle = saved
                .map(|s| format!("{} · {}:{}", s.protocol.to_uppercase(), s.host, s.port))
                .unwrap_or_else(|| "Starting terminal…".into());
            let mut cancel = false;
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.32).max(12.0));
                ui.spinner();
                ui.add_space(12.0);
                ui.heading(title);
                ui.label(egui::RichText::new(subtitle).color(muted()));
                ui.label(egui::RichText::new("Connecting…").color(muted()));
                ui.add_space(8.0);
                cancel = ui.button("Cancel").clicked();
            });
            if cancel {
                self.close_tab(idx);
            }
            return;
        }
        if let Some(error) = self.sessions[idx.min(self.sessions.len() - 1)]
            .connection_error
            .clone()
        {
            let idx = idx.min(self.sessions.len() - 1);
            let mut retry = false;
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.30).max(12.0));
                ui.heading("Connection failed");
                ui.add_space(8.0);
                ui.label(egui::RichText::new(error).color(muted()));
                ui.add_space(12.0);
                retry = ui.button("Try again").clicked();
            });
            if retry {
                self.reconnect_at(idx);
            }
            return;
        }
        let ctx = ui.ctx().clone();
        let active = idx.min(self.sessions.len() - 1);
        let font = FontId::monospace(self.font_size);
        let measured = ui
            .painter()
            .layout_no_wrap("M".to_owned(), font.clone(), text_c())
            .size();
        let cell_w = ui.fonts_mut(|f| f.glyph_width(&font, 'M')).max(2.0);
        let cell_h = if measured.y.is_finite() && measured.y >= 4.0 {
            measured.y
        } else {
            17.0
        };
        let avail = ui.available_size();
        let cols = ((avail.x / cell_w).floor() as usize).clamp(2, 400);
        let rows = ((avail.y / cell_h).floor() as usize).clamp(1, 200);

        self.sessions[active].resize(cols, rows);
        self.sessions[active].cell_width = cell_w;
        self.sessions[active].cell_height = cell_h;

        let origin = ui.min_rect().min;
        let grid = Rect::from_min_size(
            origin,
            Vec2::new(cols as f32 * cell_w, rows as f32 * cell_h),
        );
        let resp = ui.allocate_rect(grid, egui::Sense::click_and_drag());
        if !self.close_input_blocked
            && active == self.active
            && self.terminal_focus == Some(self.sessions[active].id)
        {
            resp.request_focus();
            self.terminal_focus = None;
        }
        if resp.has_focus() {
            ctx.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    resp.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                );
            });
        }

        // DECSET 1004 focus reporting, expected by vim/neovim, tmux and
        // shell-integration scripts.
        let focused = resp.has_focus();
        let wants_focus = self.sessions[active]
            .term
            .mode()
            .contains(TermMode::FOCUS_IN_OUT);
        let previous_focus = self.sessions[active].reported_focus;
        if wants_focus && focused != previous_focus {
            let sequence: &[u8] = if focused { b"\x1b[I" } else { b"\x1b[O" };
            let _ = self.sessions[active].write(sequence);
        }
        self.sessions[active].reported_focus = focused;

        let scroll_pixels = self.sessions[active].scroll_pixels;
        let mut display_offset = self.sessions[active]
            .term
            .renderable_content()
            .display_offset as i32;
        let cell_of = |pos: Pos2| -> (usize, i32) {
            let c =
                (((pos.x - origin.x) / cell_w).floor() as i64).clamp(0, cols as i64 - 1) as usize;
            let r = (((pos.y - origin.y - scroll_pixels) / cell_h).floor() as i32)
                .clamp(if scroll_pixels > 0.0 { -1 } else { 0 }, rows as i32 - 1);
            (c, r)
        };
        let cell_at = |pos: Pos2, offset: i32| -> Point {
            let (c, r) = cell_of(pos);
            Point::new(Line(r - offset), Column(c))
        };
        let pointer = ctx.input(|i| i.pointer.clone());
        let pointer_pos = pointer.interact_pos();
        let shift_held = ctx.input(|i| i.modifiers.shift);
        let mode = *self.sessions[active].term.mode();
        let mouse_mode = mode.intersects(TermMode::MOUSE_MODE);
        // Shift is the standard escape hatch: it selects locally even while a
        // full-screen application has mouse reporting enabled.
        let report_mouse = mouse_mode && !shift_held;
        let sgr_mouse = mode.contains(TermMode::SGR_MOUSE);

        if resp.hovered() {
            let ratio = self.terminal_scroll_speed / self.scroll_speed.max(1.0);
            let delta = ctx.input_mut(|i| {
                let value = i.smooth_scroll_delta.y;
                i.smooth_scroll_delta.y = 0.0;
                value
            });
            if report_mouse && delta != 0.0 {
                if let Some(pos) = pointer_pos.filter(|pos| grid.contains(*pos)) {
                    let (column, row) = cell_of(pos);
                    let button = if delta > 0.0 { 64 } else { 65 };
                    let steps = ((delta.abs() / 40.0).ceil() as usize).clamp(1, 3);
                    let sequence = mouse_sequence(button, column, row, false, sgr_mouse, &ctx);
                    for _ in 0..steps {
                        let _ = self.sessions[active].write(&sequence);
                    }
                }
            } else if delta != 0.0
                && mode.contains(TermMode::ALT_SCREEN)
                && mode.contains(TermMode::ALTERNATE_SCROLL)
            {
                // Alternate-scroll: full-screen TUIs without mouse reporting
                // (ncdu, htop, less inside tmux) expect wheel as arrow keys.
                let key = if delta > 0.0 {
                    egui::Key::ArrowUp
                } else {
                    egui::Key::ArrowDown
                };
                let steps = ((delta.abs() / 40.0).ceil() as usize).clamp(1, 3);
                if let Some(sequence) = self.sessions[active].key_bytes(key, egui::Modifiers::NONE)
                {
                    for _ in 0..steps {
                        let _ = self.sessions[active].write(&sequence);
                    }
                }
            } else {
                self.sessions[active].scroll_by_pixels(delta * ratio, cell_h);
            }
        }
        if let Some(pos) = pointer_pos {
            if pointer.primary_pressed() && grid.contains(pos) {
                resp.request_focus();
                self.active = active;
                if report_mouse {
                    let (column, row) = cell_of(pos);
                    let sequence = mouse_sequence(0, column, row, false, sgr_mouse, &ctx);
                    let _ = self.sessions[active].write(&sequence);
                } else {
                    let session = &mut self.sessions[active];
                    let cell = cell_at(pos, display_offset);
                    if shift_held && session.sel_anchor.is_some() {
                        session.sel_head = Some(cell);
                        session.sel_mode = SelMode::Char;
                        session.selecting = true;
                    } else {
                        session.sel_anchor = Some(cell);
                        session.sel_head = Some(cell);
                        session.sel_mode = SelMode::Char;
                        session.selecting = true;
                    }
                    session.sel_scroll_at = None;
                }
            }
            if report_mouse
                && grid.contains(pos)
                && (mode.contains(TermMode::MOUSE_MOTION)
                    || (mode.contains(TermMode::MOUSE_DRAG) && pointer.primary_down()))
            {
                let (column, row) = cell_of(pos);
                if self.sessions[active].mouse_cell != Some((column, row)) {
                    self.sessions[active].mouse_cell = Some((column, row));
                    let button = if pointer.primary_down() { 32 } else { 32 + 3 };
                    let sequence = mouse_sequence(button, column, row, false, sgr_mouse, &ctx);
                    let _ = self.sessions[active].write(&sequence);
                }
            } else if !grid.contains(pos) {
                self.sessions[active].mouse_cell = None;
            }
            if self.sessions[active].selecting {
                let top = origin.y;
                let bottom = origin.y + rows as f32 * cell_h;
                let session = &mut self.sessions[active];
                if pos.y < top || pos.y > bottom {
                    let now = Instant::now();
                    if session
                        .sel_scroll_at
                        .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(45))
                    {
                        let delta = if pos.y < top { -1 } else { 1 };
                        session
                            .term
                            .scroll_display(alacritty_terminal::grid::Scroll::Delta(delta));
                        session.sel_scroll_at = Some(now);
                    }
                    ctx.request_repaint_after(Duration::from_millis(16));
                    display_offset = session.term.renderable_content().display_offset as i32;
                } else {
                    session.sel_scroll_at = None;
                }
                session.sel_head = Some(cell_at(pos, display_offset));
            }
            if pointer.primary_released() {
                if report_mouse {
                    let (column, row) = cell_of(pos);
                    let sequence = mouse_sequence(0, column, row, true, sgr_mouse, &ctx);
                    let _ = self.sessions[active].write(&sequence);
                } else if self.sessions[active].selecting {
                    let session = &mut self.sessions[active];
                    session.selecting = false;
                    session.sel_scroll_at = None;
                    if session.sel_mode == SelMode::Char && session.sel_anchor == session.sel_head {
                        session.clear_selection();
                    }
                }
                if !report_mouse && self.ui_settings.copy_on_select {
                    let text = self.sessions[active].selection_text();
                    self.copy_to_clipboard(&ctx, text);
                }
            }
        }
        if let Some(pos) = pointer_pos {
            if resp.triple_clicked() {
                let (_, r) = cell_of(pos);
                let line = r - display_offset;
                let session = &mut self.sessions[active];
                session.sel_anchor = Some(Point::new(Line(line), Column(0)));
                session.sel_head = Some(Point::new(Line(line), Column(cols.saturating_sub(1))));
                session.sel_mode = SelMode::Line;
                session.selecting = false;
            } else if resp.double_clicked() {
                let (c, r) = cell_of(pos);
                let line = r - display_offset;
                let session = &mut self.sessions[active];
                let cell = Point::new(Line(line), Column(c));
                session.sel_anchor = Some(cell);
                session.sel_head = Some(cell);
                session.sel_mode = SelMode::Word;
                session.selecting = false;
                let word = session.selection_text();
                self.copy_to_clipboard(&ctx, word);
            }
        }
        if resp.clicked() {
            resp.request_focus();
            self.active = active;
        }
        if resp.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Text);
        }

        let sel = self.sessions[active].selection();
        let hover = if resp.hovered() && self.sessions[active].sel_anchor.is_none() {
            pointer_pos.filter(|pos| grid.contains(*pos)).map(|pos| {
                let cell = cell_at(pos, display_offset);
                let (start, end) = self.sessions[active].word_bounds(cell.line.0, cell.column.0);
                (
                    Point::new(cell.line, Column(start)),
                    Point::new(cell.line, Column(end)),
                )
            })
        } else {
            None
        };

        terminal_render::paint(
            ui,
            &self.sessions[active],
            terminal_render::Paint {
                origin,
                colors: self.ui_settings.terminal_appearance.colors(bg(), text_c()),
                font: font.clone(),
                cell: Vec2::new(cell_w, cell_h),
                selection: sel,
                hover,
                rules: if self.highlight { &self.hl_rules } else { &[] },
                search: &self.search,
            },
        );

        let has_selection = self.sessions[active].sel_anchor.is_some();
        let mut menu_copy = false;
        let mut menu_select_all = false;
        let mut menu_clear = false;
        resp.context_menu(|ui| {
            if ui
                .add_enabled(has_selection, egui::Button::new("Copy"))
                .clicked()
            {
                menu_copy = true;
                ui.close();
            }
            if ui.button("Select all").clicked() {
                menu_select_all = true;
                ui.close();
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("Clear selection"))
                .clicked()
            {
                menu_clear = true;
                ui.close();
            }
        });
        if menu_copy {
            let text = self.sessions[active].selection_text();
            self.copy_to_clipboard(&ctx, text);
        }
        if menu_select_all {
            let session = &mut self.sessions[active];
            session.sel_anchor = Some(Point::new(
                Line(-display_offset - i32::from(session.scroll_pixels > 0.0)),
                Column(0),
            ));
            session.sel_head = Some(Point::new(
                Line(rows as i32 - 1 - display_offset),
                Column(cols.saturating_sub(1)),
            ));
            session.sel_mode = SelMode::Char;
        }
        if menu_clear {
            self.sessions[active].clear_selection();
        }

        if resp.has_focus()
            && active == self.active
            && !self.close_input_blocked
            && !self.close_confirmation.active()
        {
            let mut out: Vec<u8> = Vec::new();
            let mut copy = false;
            let mut interrupted = false;
            let mut select_all = false;
            ctx.input(|i| {
                for (event_index, event) in i.events.iter().enumerate() {
                    match event {
                        egui::Event::Text(t) => {
                            if !(i.modifiers.ctrl || i.modifiers.command) {
                                // Alt+<printable> is the readline word-motion
                                // family (Alt+B/F/D...), i.e. ESC-prefixed.
                                if i.modifiers.alt && !i.modifiers.ctrl {
                                    out.push(0x1b);
                                }
                                out.extend_from_slice(t.as_bytes());
                            }
                        }
                        egui::Event::Paste(t) => {
                            // On Linux, plain Ctrl+V must reach the application
                            // as 0x16 (vim visual-block, readline quoted-insert).
                            // Paste is Ctrl+Shift+V / Shift+Insert, as in Tabby.
                            if cfg!(target_os = "linux")
                                && i.modifiers.ctrl
                                && !i.modifiers.shift
                                && !i.modifiers.alt
                            {
                                out.push(0x16);
                            } else {
                                out.extend_from_slice(&paste_bytes(
                                    self.sessions[active].term.mode(),
                                    t.clone(),
                                ));
                            }
                        }
                        egui::Event::Cut => {
                            // egui-winit turns Ctrl+X into Cut on Linux, but a
                            // terminal must deliver 0x18 (vim decrement, emacs
                            // prefix). Selection cutting is not a terminal verb.
                            out.push(0x18);
                        }
                        egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                            if !(i.modifiers.ctrl || i.modifiers.command) {
                                out.extend_from_slice(text.as_bytes());
                            }
                        }
                        egui::Event::Copy => {
                            // winit emits Copy instead of a C press. A following
                            // key event (including Ctrl release) retains the event
                            // modifiers even when the frame's final state is empty.
                            let modifiers = i.events[event_index + 1..]
                                .iter()
                                .find_map(|event| {
                                    if let egui::Event::Key { modifiers, .. } = event {
                                        Some(*modifiers)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(i.modifiers);
                            if sel.is_some() {
                                copy = true;
                            } else if (modifiers.ctrl || modifiers.command)
                                && !modifiers.shift
                                && !interrupted
                            {
                                out.push(0x03);
                                interrupted = true;
                            }
                        }
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            if (modifiers.ctrl || modifiers.command) && *key == egui::Key::C {
                                if sel.is_some() || modifiers.shift {
                                    copy = true;
                                } else if !interrupted {
                                    out.push(0x03);
                                    interrupted = true;
                                }
                            } else if (modifiers.ctrl || modifiers.command)
                                && modifiers.shift
                                && *key == egui::Key::A
                            {
                                // Tabby/xterm convention: Ctrl+Shift+A selects,
                                // leaving Ctrl+A (0x01) to readline/vim.
                                select_all = true;
                            } else if modifiers.shift
                                && matches!(key, egui::Key::PageUp | egui::Key::PageDown)
                            {
                                let scroll = if *key == egui::Key::PageUp {
                                    alacritty_terminal::grid::Scroll::PageUp
                                } else {
                                    alacritty_terminal::grid::Scroll::PageDown
                                };
                                self.sessions[active].term.scroll_display(scroll);
                                self.sessions[active].scroll_pixels = 0.0;
                            } else if modifiers.shift && *key == egui::Key::Insert {
                                if let Some(text) = clipboard_text() {
                                    out.extend_from_slice(&paste_bytes(
                                        self.sessions[active].term.mode(),
                                        text,
                                    ));
                                }
                            } else if (modifiers.ctrl || modifiers.command)
                                && *key == egui::Key::Insert
                            {
                                copy = true;
                            } else if let Some(bytes) =
                                self.sessions[active].key_bytes(*key, *modifiers)
                            {
                                out.extend_from_slice(&bytes);
                            }
                        }
                        _ => {}
                    }
                }
            });
            if select_all {
                let session = &mut self.sessions[active];
                session.sel_anchor = Some(Point::new(
                    Line(-display_offset - i32::from(session.scroll_pixels > 0.0)),
                    Column(0),
                ));
                session.sel_head = Some(Point::new(
                    Line(rows as i32 - 1 - display_offset),
                    Column(cols.saturating_sub(1)),
                ));
                session.sel_mode = SelMode::Char;
            }
            if copy {
                let text = self.sessions[active].selection_text();
                self.copy_to_clipboard(&ctx, text);
            }
            if !out.is_empty() {
                self.sessions[active]
                    .term
                    .scroll_display(alacritty_terminal::grid::Scroll::Bottom);
                self.sessions[active].scroll_pixels = 0.0;
                if let Err(error) = self.sessions[active].write(&out) {
                    self.flash = error.to_string();
                }
                self.sessions[active].clear_selection();
            }
        }
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

impl App {
    fn refresh_devices(&mut self) {
        match self.db.list_sessions() {
            Ok(devices) => self.devices = devices,
            Err(error) => {
                self.flash = format!("Could not refresh devices: {error}");
                return;
            }
        }
        self.refilter_devices();
    }

    fn refresh_folders(&mut self) {
        match self.db.list_folders() {
            Ok(folders) => self.folders = folders,
            Err(error) => self.flash = format!("Could not refresh folders: {error}"),
        }
    }

    fn folder_choices(&self) -> Vec<device_list::FolderChoice> {
        fn walk(
            folders: &[Folder],
            parent: Option<&str>,
            depth: usize,
            out: &mut Vec<device_list::FolderChoice>,
        ) {
            for folder in folders
                .iter()
                .filter(|folder| folder.parent_id.as_deref() == parent)
            {
                out.push((
                    format!("{}{}", "  ".repeat(depth), folder.name),
                    Some(folder.id.clone()),
                ));
                walk(folders, Some(&folder.id), depth + 1, out);
            }
        }
        let mut choices = vec![("No folder".to_string(), None)];
        walk(&self.folders, None, 0, &mut choices);
        choices
    }

    fn commit_folder(&mut self) {
        let Some(editor) = self.folder_editor.as_ref() else {
            return;
        };
        let input = FolderInput {
            id: editor.id.clone(),
            parent_id: editor.parent_id.clone(),
            name: editor.name.trim().to_string(),
            sort_order: None,
        };
        match self.db.upsert_folder(&input) {
            Ok(folder) => {
                self.folder_editor = None;
                self.flash = format!("Saved folder {}", folder.name);
                self.refresh_folders();
                self.refresh_devices();
            }
            Err(error) => {
                if let Some(editor) = self.folder_editor.as_mut() {
                    editor.error = Some(error.to_string());
                }
            }
        }
    }

    fn set_device_folder(&mut self, index: usize, folder_id: Option<String>) {
        let Some(device) = self.devices.get(index).cloned() else {
            return;
        };
        let input = SavedSessionInput {
            id: Some(device.id.clone()),
            folder_id: folder_id.clone(),
            name: device.name.clone(),
            host: device.host.clone(),
            port: device.port,
            protocol: device.protocol.clone(),
            username: device.username.clone(),
            ssh_alias: device.ssh_alias.clone(),
            identity_file: device.identity_file.clone(),
            proxy_jump: device.proxy_jump.clone(),
            tags: device.tags.clone(),
            description: device.description.clone(),
            notes: device.notes.clone(),
            settings_override: device.settings_override.clone(),
            credential_ref: device.credential_ref.clone(),
            is_favorite: device.is_favorite,
            sort_order: Some(device.sort_order),
        };
        match self.db.upsert_session(&input) {
            Ok(_) => {
                self.flash = format!("Moved {}", device.name);
                self.refresh_devices();
            }
            Err(error) => self.flash = format!("Could not move device: {error}"),
        }
    }

    fn delete_confirmed_folder(&mut self) {
        let Some(folder) = self.folder_delete.take() else {
            return;
        };
        match self.db.delete_folder(&folder.id) {
            Ok(()) => {
                self.flash = format!("Deleted folder {}", folder.name);
                self.refresh_folders();
                self.refresh_devices();
            }
            Err(error) => self.flash = format!("Could not delete folder: {error}"),
        }
    }

    fn refilter_devices(&mut self) {
        let needle = self.dev_filter.trim().to_lowercase();
        self.device_indices = self
            .devices
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                needle.is_empty()
                    || s.name.to_lowercase().contains(&needle)
                    || s.host.to_lowercase().contains(&needle)
                    || s.tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&needle))
            })
            .map(|(index, _)| index)
            .collect();
        self.applied_filter.clone_from(&self.dev_filter);
    }

    fn ssh_devices(&self) -> Vec<SavedSession> {
        self.devices
            .iter()
            .filter(|s| s.protocol == "ssh")
            .cloned()
            .collect()
    }

    fn spec_for(&self, saved: &SavedSession) -> SshSpec {
        let cred_user = saved
            .credential_ref
            .as_deref()
            .and_then(|r| self.db.get_credential(r).ok().flatten())
            .and_then(|c| c.username);
        SshSpec {
            host: saved.host.clone(),
            port: saved.port,
            username: saved.username.clone().or(cred_user),
            identity_file: saved.identity_file.clone(),
            proxy_jump: saved.proxy_jump.clone(),
            name: Some(saved.name.clone()),
            term: Some("xterm-256color".into()),
            connect_timeout: Some(20),
            server_alive_interval: Some(60),
            x11_forwarding: false,
            forward_agent: false,
            compression: false,
            certificate_file: None,
            pkcs11_provider: None,
            engine: Some("openssh".into()),
            password: None,
        }
    }

    fn queue_sftp(&mut self, action: remote_files::Action) {
        if self.sftp_pending.is_some() {
            self.sftp_msg = "an SFTP operation is still running".into();
            return;
        }
        let devs = self.ssh_devices();
        let Some(saved) = devs.get(self.sftp_session) else {
            self.sftp_msg = "no SSH device".into();
            return;
        };
        let request = remote_files::Request {
            device_id: saved.id.clone(),
            path: self.sftp_path.clone(),
            action,
        };
        let credential_ref = saved.credential_ref.clone().filter(|r| !r.is_empty());
        if let Some(reference) = &credential_ref {
            if self.vault.is_none() && vault::is_locked_for(reference) {
                self.sftp_msg = "Vault is locked — unlock it in Tools → Credentials first".into();
                return;
            }
        }
        let secret = credential_ref
            .as_ref()
            .and_then(|reference| self.vault.as_ref()?.get_secret(reference));
        match remote_files::Pending::start(self.spec_for(saved), request, credential_ref, secret) {
            Ok(pending) => {
                self.sftp_entries.clear();
                self.sftp_location = None;
                self.sftp_pending = Some(pending);
                self.sftp_msg = "SFTP operation running…".into();
            }
            Err(e) => self.sftp_msg = e,
        }
    }

    fn poll_sftp(&mut self) {
        let Some(pending) = self.sftp_pending.take() else {
            return;
        };
        let result = match pending.task.poll() {
            Ok(None) => {
                self.sftp_pending = Some(pending);
                return;
            }
            Ok(Some(result)) => result,
            Err(e) => Err(e),
        };
        let selected = self
            .ssh_devices()
            .get(self.sftp_session)
            .map(|s| s.id.clone());
        if selected.as_deref() != Some(&pending.device_id) || self.sftp_path != pending.path {
            self.sftp_msg =
                "previous SFTP operation finished; browse to refresh this directory".into();
            return;
        }
        match result {
            Ok(entries) => {
                self.sftp_msg = format!("{} entries", entries.len());
                self.sftp_entries = entries;
                self.sftp_location = Some((pending.device_id, pending.path));
            }
            Err(e) => self.sftp_msg = e,
        }
    }

    fn sftp_browse(&mut self) {
        self.queue_sftp(remote_files::Action::List);
    }

    fn sftp_remove(&mut self, path: &str, is_dir: bool) {
        self.queue_sftp(remote_files::Action::Remove(path.to_owned(), is_dir));
    }

    fn sftp_mkdir(&mut self) {
        let name = self.sftp_new.trim().to_string();
        if name.is_empty() || self.sftp_pending.is_some() {
            return;
        }
        let target = format!("{}/{}", self.sftp_path.trim_end_matches('/'), name);
        self.queue_sftp(remote_files::Action::Mkdir(target));
        if self.sftp_pending.is_some() {
            self.sftp_new.clear();
        }
    }

    fn sftp_download(&mut self, remote: &str) {
        let devs = self.ssh_devices();
        let Some(saved) = devs.get(self.sftp_session) else {
            return;
        };
        let spec = self.spec_for(saved);
        let name = remote.rsplit('/').next().unwrap_or("file");
        let local = format!("/tmp/{name}");
        match security::build_scp_argv(&spec, false, &[remote.to_string(), local.clone()]) {
            Ok(argv) => {
                self.sftp_msg = format!("downloading → {local}");
                std::thread::spawn(move || {
                    let _ = std::process::Command::new(&argv[0])
                        .args(&argv[1..])
                        .status();
                });
            }
            Err(e) => self.sftp_msg = e.to_string(),
        }
    }

    fn tunnel_start(&mut self) {
        let devs = self.ssh_devices();
        let Some(saved) = devs.get(self.t_session) else {
            return;
        };
        let spec = TunnelSpec {
            kind: self.t_kind.clone(),
            bind_address: self.t_bind.clone(),
            bind_port: self.t_bind_port,
            dest_host: if self.t_kind == "dynamic" {
                None
            } else {
                Some(self.t_dest.clone())
            },
            dest_port: if self.t_kind == "dynamic" {
                None
            } else {
                Some(self.t_dest_port)
            },
            ssh_host: saved.host.clone(),
            ssh_port: saved.port,
            ssh_user: saved.username.clone(),
            identity_file: saved.identity_file.clone(),
            label: Some(saved.name.clone()),
        };
        let _ = self.tunnels.start(spec);
    }

    fn apply_vendor(&mut self, id: &str) -> Option<(String, String, String)> {
        let v = VENDORS.iter().find(|v| v.label == id)?;
        // seed commands if not already present
        let existing = self.db.list_commands().unwrap_or_default();
        for (name, content) in v.commands {
            if !existing.iter().any(|c| c.name == *name) {
                let _ = self.db.upsert_command(&opsterm_core::domain::CommandInput {
                    id: None,
                    group_id: None,
                    name: (*name).to_string(),
                    content: (*content).to_string(),
                    vendor_profile: Some(v.label.to_string()),
                    execute_mode: "insert".into(),
                    risk_level: "low".into(),
                    variables: serde_json::json!({}),
                    tags: vec![],
                });
            }
        }
        self.commands = self.db.list_commands().unwrap_or_default();
        // seed highlight rules
        for (kw, col) in v.highlights {
            if !self
                .hl_rules
                .iter()
                .any(|(k, _, _)| k.eq_ignore_ascii_case(kw))
            {
                self.hl_rules.push(((*kw).to_string(), *col, true));
            }
        }
        Some((
            v.pager.to_string(),
            v.backspace.to_string(),
            v.login.to_string(),
        ))
    }

    fn move_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.sessions.len() || to >= self.sessions.len() {
            return;
        }
        let sess = self.sessions.remove(from);
        self.sessions.insert(to, sess);
        self.active = if self.active == from {
            to
        } else if from < self.active && self.active <= to {
            self.active - 1
        } else if to <= self.active && self.active < from {
            self.active + 1
        } else {
            self.active
        };
    }

    fn risky_session_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|session| session.close_is_risky())
            .count()
    }

    fn request_close_tab(&mut self, index: usize) {
        let Some(session) = self.sessions.get(index) else {
            return;
        };
        self.close_input_blocked = true;
        self.close_blocked_frame = self.context.as_ref().map(|ctx| ctx.cumulative_frame_nr());
        let risky = session.close_is_risky();
        if !risky {
            self.close_tab(index);
            self.terminal_focus = self.sessions.get(self.active).map(|session| session.id);
            if let Some(ctx) = &self.context {
                close_confirmation::discard_background_keyboard(ctx);
            }
            return;
        }
        self.close_confirmation
            .request(close_confirmation::Target::Tab {
                id: session.id,
                title: session.title.clone(),
            });
        if let Some(ctx) = &self.context {
            close_confirmation::discard_background_keyboard(ctx);
        }
    }

    fn protect_window_close(&mut self, ctx: &egui::Context) {
        if !self.allow_close
            && self.close_blocked_frame != Some(ctx.cumulative_frame_nr())
            && ctx.input(|i| i.viewport().close_requested())
        {
            // A simultaneous Enter must never reach a terminal or editor,
            // including when an idle workspace closes without a prompt.
            self.close_blocked_frame = Some(ctx.cumulative_frame_nr());
            let sessions = self.risky_session_count();
            if sessions > 0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.close_confirmation
                    .request(close_confirmation::Target::Application { sessions });
            }
        }
    }

    fn confirm_close(&mut self, ctx: &egui::Context) {
        let Some(target) = self.close_confirmation.show(ctx) else {
            return;
        };
        match target {
            close_confirmation::Target::Tab { id, .. } => {
                if let Some(index) = self.sessions.iter().position(|session| session.id == id) {
                    self.close_tab(index);
                }
                self.terminal_focus = self.sessions.get(self.active).map(|session| session.id);
            }
            close_confirmation::Target::Application { .. } => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            close_confirmation::Target::Restart { path, .. } => match update::restart(&path) {
                Ok(()) => std::process::exit(0),
                Err(error) => self.updater.error = Some(error),
            },
        }
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.sessions.len() {
            return;
        }
        let id = self.sessions[index].id;
        self.sessions.remove(index);
        self.runners.retain(|r| r.target != id);
        self.queued_connections
            .retain(|q| q.request.target != Some(id));
        if index < self.active {
            self.active -= 1;
        } else if self.active >= self.sessions.len() {
            self.active = self.sessions.len().saturating_sub(1);
        }
        trim_heap();
    }

    fn export_output(&mut self) {
        let Some(sess) = self.sessions.get(self.active) else {
            return;
        };
        let text = sess.term_text();
        let title: String = sess
            .title
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || ".-_".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let dir = data_dir().join("exports");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{title}_{}.txt", chrono_like_timestamp()));
        match std::fs::write(&path, text) {
            Ok(()) => self.flash = format!("exported → {}", path.display()),
            Err(e) => self.flash = format!("export failed: {e}"),
        }
    }

    fn run_dns(&mut self) {
        let host = self.dns_host.trim().to_string();
        if host.is_empty() {
            return;
        }
        let (tx, rx): (Sender<String>, Receiver<String>) = channel();
        self.dns_rx = Some(rx);
        self.dns_out = format!("resolving {host}…");
        std::thread::spawn(move || {
            let out = std::process::Command::new("/usr/bin/nslookup")
                .arg(&host)
                .output()
                .or_else(|_| {
                    std::process::Command::new("/usr/bin/getent")
                        .args(["hosts", &host])
                        .output()
                })
                .or_else(|_| {
                    std::process::Command::new("/bin/getent")
                        .args(["hosts", &host])
                        .output()
                });
            let text = match out {
                Ok(o) => {
                    let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
                    t.push_str(&String::from_utf8_lossy(&o.stderr));
                    t
                }
                Err(e) => format!("lookup failed: {e}"),
            };
            let _ = tx.send(text);
        });
    }

    fn run_trace(&mut self) {
        let host = self.trace_host.trim().to_string();
        if host.is_empty() {
            return;
        }
        let (tx, rx): (Sender<String>, Receiver<String>) = channel();
        self.trace_rx = Some(rx);
        self.trace_out = format!("traceroute {host}…");
        std::thread::spawn(move || {
            let out = std::process::Command::new("/usr/bin/traceroute")
                .args(["-m", "20", "-w", "2", &host])
                .output()
                .or_else(|_| {
                    std::process::Command::new("/usr/bin/tracepath")
                        .args(["-m", "20", &host])
                        .output()
                });
            let text = match out {
                Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
                Err(e) => format!("traceroute failed: {e}"),
            };
            let _ = tx.send(text);
        });
    }

    fn run_ping(&mut self) {
        let host = self.ping_host.trim().to_string();
        if host.is_empty() {
            return;
        }
        let count = self.ping_count.max(1);
        let (tx, rx): (Sender<String>, Receiver<String>) = channel();
        self.ping_rx = Some(rx);
        self.ping_out = format!("pinging {host}…");
        std::thread::spawn(move || {
            let out = std::process::Command::new("/bin/ping")
                .args(["-c", &count.to_string(), "-W", "2", &host])
                .output();
            let text = match out {
                Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
                Err(e) => format!("ping failed: {e}"),
            };
            let _ = tx.send(text);
        });
    }

    fn save_workspace(&mut self) {
        if self.ws_name.trim().is_empty() {
            self.ws_msg = "enter a name".into();
            return;
        }
        let devices: Vec<String> = self
            .sessions
            .iter()
            .filter_map(|s| s.saved_id.clone())
            .collect();
        let layout = serde_json::json!({ "devices": devices });
        let input = WorkspaceInput {
            id: None,
            name: self.ws_name.clone(),
            description: None,
            layout,
        };
        if self.db.upsert_workspace(&input).is_ok() {
            self.ws_msg = format!("saved {} device ref(s)", devices.len());
            self.workspaces = self.db.list_workspaces().unwrap_or_default();
        }
    }

    fn restore_workspace(&mut self, layout: &serde_json::Value) {
        let ids: Vec<String> = layout
            .get("devices")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let all = self.db.list_sessions().unwrap_or_default();
        let mut n = 0;
        for id in ids {
            if let Some(saved) = all.iter().find(|s| s.id == id) {
                self.open_saved_new(saved);
                n += 1;
            }
        }
        self.ws_msg = format!("queued {n} session(s) for restore");
    }

    fn reconnect_active(&mut self) {
        self.reconnect_at(self.active);
    }

    fn reconnect_at(&mut self, idx: usize) {
        let Some(sess) = self.sessions.get(idx) else {
            return;
        };
        if self
            .pending_connections
            .iter()
            .any(|p| p.request.target == Some(sess.id))
            || self
                .queued_connections
                .iter()
                .any(|p| p.request.target == Some(sess.id))
        {
            return;
        }
        let target = sess.id;
        let title = sess.title.clone();
        let origin = sess.origin.clone();
        let saved = sess
            .saved_id
            .as_deref()
            .and_then(|id| self.db.get_session(id).ok());
        let secret = saved
            .as_ref()
            .and_then(|saved| saved.credential_ref.as_deref())
            .and_then(|reference| self.vault.as_ref()?.get_secret(reference));
        let dims = TerminalDimensions {
            cols: sess.cols as u16,
            rows: sess.rows as u16,
        };
        let previous = sess.provider();
        let request = connections::Request {
            title,
            origin,
            saved,
            secret,
            target: Some(target),
            dims,
        };
        self.runners.retain(|r| r.target != target);
        self.queue_connection(request, Some(previous));
    }

    fn pump_sessions(&mut self) {
        // Poll lifecycle even for panes that are not rendered, independently of
        // the output budget. Rotate the starting pane to avoid starvation.
        for session in &mut self.sessions {
            if !session.opening {
                session.exited = !session.provider.is_alive();
            }
        }
        let count = self.sessions.len();
        if count == 0 {
            return;
        }
        let started = Instant::now();
        for _ in 0..count {
            let index = self.pump_cursor % count;
            self.sessions[index].pump();
            self.pump_cursor = (index + 1) % count;
            if started.elapsed() >= Duration::from_millis(4) {
                break;
            }
        }
    }

    /// Drain terminal replies (DA/DSR, OSC 10/11, window size) and route them
    /// back to the PTY. These are not cosmetic: Codex/opencode use OSC 10/11 to
    /// learn the real background/foreground, and many TUIs wait for DA/DSR.
    fn handle_session_events(&mut self, ctx: &egui::Context) {
        if self.sessions.is_empty() {
            return;
        }
        let colors = self.ui_settings.terminal_appearance.colors(bg(), text_c());
        let mut bell = false;
        for index in 0..self.sessions.len() {
            let events = self.sessions[index].drain_events();
            if events.is_empty() {
                continue;
            }
            let (cols, rows) = (self.sessions[index].cols, self.sessions[index].rows);
            let (cell_width, cell_height) = (
                self.sessions[index].cell_width.max(1.0) as u16,
                self.sessions[index].cell_height.max(1.0) as u16,
            );
            for event in events {
                match event {
                    Event::PtyWrite(text) => {
                        let _ = self.sessions[index].write(text.as_bytes());
                    }
                    Event::ColorRequest(color_index, format) => {
                        let rgb = terminal_query_color(&colors, color_index);
                        let reply = format(rgb);
                        let _ = self.sessions[index].write(reply.as_bytes());
                    }
                    Event::TextAreaSizeRequest(format) => {
                        let reply = format(WindowSize {
                            num_lines: rows as u16,
                            num_cols: cols as u16,
                            cell_width,
                            cell_height,
                        });
                        let _ = self.sessions[index].write(reply.as_bytes());
                    }
                    Event::ClipboardStore(_, text) => {
                        if !text.is_empty() && text.len() <= 4 * 1024 * 1024 {
                            ctx.copy_text(text);
                            self.toast =
                                Some(("Copied from terminal to clipboard".into(), Instant::now()));
                        }
                    }
                    Event::Bell => bell = true,
                    // OSC 52 reads need a clipboard backend that can read the
                    // system clipboard; unsupported reads are ignored safely.
                    Event::ClipboardLoad(_, _) => {}
                    _ => {}
                }
            }
        }
        if bell {
            self.last_bell = Some(Instant::now());
        }
    }

    fn start_runner(&mut self, script: &str) {
        if let Some(target) = self.sessions.get(self.active).map(|s| s.id) {
            self.start_runner_for(target, script);
        }
    }

    fn start_runner_for(&mut self, target: u64, script: &str) {
        let steps = parse_expect(script);
        if steps.is_empty() {
            return;
        }
        if steps.len() > 1024 {
            self.flash = "script has too many steps".into();
            return;
        }
        for step in &steps {
            if let Step::WaitFor(pattern, _) = step {
                if let Err(e) = regex::Regex::new(pattern) {
                    self.flash = format!("invalid regex: {e}");
                    return;
                }
            }
        }
        if self.runners.iter().any(|r| r.target == target) {
            self.flash = "a script is already running on this session".into();
            return;
        }
        self.runners.push(Runner {
            target,
            steps,
            index: 0,
            deadline: None,
        });
    }

    fn tick_runner(&mut self) {
        for mut r in std::mem::take(&mut self.runners) {
            let Some(session) = self.sessions.iter().find(|s| s.id == r.target) else {
                self.flash = "script stopped: target session closed".into();
                continue;
            };
            if session.exited || !session.provider.is_alive() {
                self.flash = "script stopped: target session disconnected".into();
                continue;
            }
            let text = session.term_text();
            let now = Instant::now();
            let mut failed = false;
            // Bound work per frame, including scripts with many send steps.
            for _ in 0..64 {
                match r.steps.get(r.index) {
                    None => break,
                    Some(Step::Send(data, enter)) => {
                        let mut bytes = data.as_bytes().to_vec();
                        if *enter {
                            bytes.push(b'\r');
                        }
                        if let Err(e) = session.provider.write(&bytes) {
                            self.flash = format!("script write failed: {e}");
                            failed = true;
                            break;
                        }
                        r.index += 1;
                    }
                    Some(Step::Sleep(ms)) => {
                        let dl = *r
                            .deadline
                            .get_or_insert(now + Duration::from_millis((*ms).min(600_000)));
                        if now < dl {
                            break;
                        }
                        r.deadline = None;
                        r.index += 1;
                    }
                    Some(Step::WaitFor(pattern, ms)) => {
                        let dl = *r
                            .deadline
                            .get_or_insert(now + Duration::from_millis((*ms).min(600_000)));
                        match regex::Regex::new(pattern) {
                            Ok(re) if re.is_match(&text) => {
                                r.deadline = None;
                                r.index += 1;
                            }
                            Ok(_) if now < dl => break,
                            Ok(_) => {
                                self.flash = format!("script timeout waiting for {pattern}");
                                failed = true;
                                break;
                            }
                            Err(e) => {
                                self.flash = format!("invalid regex: {e}");
                                failed = true;
                                break;
                            }
                        }
                    }
                }
            }
            if !failed && r.index < r.steps.len() {
                self.runners.push(r);
            }
        }
    }

    fn save_editor(&mut self, ed: &EditorState) -> bool {
        let original = match ed
            .id
            .as_deref()
            .map(|id| self.db.get_session(id))
            .transpose()
        {
            Ok(original) => original,
            Err(error) => {
                self.flash = format!("Could not load device: {error}");
                return false;
            }
        };
        let mut ov = original
            .as_ref()
            .and_then(|s| s.settings_override.as_ref())
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let had_backspace = ov.contains_key("backspace");
        for key in ["pager", "backspace", "loginScript"] {
            ov.remove(key);
        }
        if ed.pager != "none" {
            ov.insert("pager".into(), serde_json::json!(ed.pager));
        }
        if ed.backspace == "bs" || had_backspace {
            ov.insert("backspace".into(), serde_json::json!(ed.backspace));
        }
        if !ed.login_script.trim().is_empty() {
            ov.insert("loginScript".into(), serde_json::json!(ed.login_script));
        }
        let settings_override = if ov.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(ov))
        };
        let input = SavedSessionInput {
            id: ed.id.clone(),
            folder_id: ed.folder_id.clone(),
            name: ed.name.clone(),
            host: ed.host.clone(),
            port: ed.port,
            protocol: ed.protocol.clone(),
            username: if ed.username.is_empty() {
                None
            } else {
                Some(ed.username.clone())
            },
            ssh_alias: original.as_ref().and_then(|s| s.ssh_alias.clone()),
            identity_file: original.as_ref().and_then(|s| s.identity_file.clone()),
            proxy_jump: original.as_ref().and_then(|s| s.proxy_jump.clone()),
            tags: original
                .as_ref()
                .map(|s| s.tags.clone())
                .unwrap_or_default(),
            description: original.as_ref().and_then(|s| s.description.clone()),
            notes: original.as_ref().and_then(|s| s.notes.clone()),
            settings_override,
            credential_ref: if ed.credential.is_empty() {
                None
            } else {
                Some(ed.credential.clone())
            },
            is_favorite: original.as_ref().is_some_and(|s| s.is_favorite),
            sort_order: original.as_ref().map(|s| s.sort_order),
        };
        match self.db.upsert_session(&input) {
            Ok(saved) => {
                self.flash = format!("Saved {}", saved.name);
                self.refresh_devices();
                true
            }
            Err(error) => {
                self.flash = format!("Could not save device: {error}");
                false
            }
        }
    }

    fn import_csv(&mut self) {
        let mut n = 0;
        for line in self.import_text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split(',').map(str::trim).collect();
            if parts.len() < 2 {
                continue;
            }
            let input = SavedSessionInput {
                id: None,
                folder_id: None,
                name: parts[0].to_string(),
                host: parts[1].to_string(),
                port: parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(22u16),
                protocol: parts
                    .get(3)
                    .filter(|p| !p.is_empty())
                    .unwrap_or(&"ssh")
                    .to_string(),
                username: parts
                    .get(4)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                ssh_alias: None,
                identity_file: None,
                proxy_jump: None,
                tags: vec![],
                description: None,
                notes: None,
                settings_override: None,
                credential_ref: parts
                    .get(5)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                is_favorite: false,
                sort_order: None,
            };
            if self.db.upsert_session(&input).is_ok() {
                n += 1;
            }
        }
        self.refresh_devices();
        self.import_msg = format!("Imported {n} device(s) from CSV");
    }
}

fn install_fonts(ctx: &egui::Context, terminal_path: Option<&std::path::Path>) {
    let mut defs = egui::FontDefinitions::empty();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".local/share")));
    let mut sans = Vec::new();
    for dir in data_home.iter() {
        sans.push(dir.join("fonts/Inter-Regular.ttf"));
    }
    if let Some(home) = &home {
        sans.push(home.join(".fonts/Inter-Regular.ttf"));
    }
    for path in [
        "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
        "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        sans.push(std::path::PathBuf::from(path));
    }
    let mono: Vec<_> = terminal_path
        .map(std::path::Path::to_path_buf)
        .into_iter()
        .collect();
    for (key, family, paths) in [
        ("ot-sans", egui::FontFamily::Proportional, sans),
        ("ot-mono", egui::FontFamily::Monospace, mono),
    ] {
        if let Some(bytes) = paths.iter().find_map(|path| std::fs::read(path).ok()) {
            defs.font_data.insert(
                key.to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            defs.families
                .entry(family)
                .or_default()
                .insert(0, key.to_owned());
        }
    }
    let defaults = egui::FontDefinitions::default();
    for (family, names) in defaults.families {
        if defs.families.contains_key(&family) {
            continue;
        }
        for name in &names {
            if let Some(data) = defaults.font_data.get(name) {
                defs.font_data.insert(name.clone(), data.clone());
            }
        }
        defs.families.insert(family, names);
    }
    ctx.set_fonts(defs);
}

fn line_highlights(text: &str, rules: &[(String, [u8; 3], bool)]) -> Vec<(usize, usize, Color32)> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for (kw, rgb, enabled) in rules {
        if !*enabled || kw.is_empty() {
            continue;
        }
        let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let kwl = kw.to_lowercase();
        let mut from = 0;
        while let Some(pos) = lower[from..].find(&kwl) {
            let start = from + pos;
            let end = start + kwl.len() - 1;
            out.push((start, end, color));
            from = start + kwl.len();
        }
    }
    out
}

fn pager_command(ov: &Option<serde_json::Value>) -> Option<String> {
    let o = ov.as_ref()?;
    match o.get("pager")?.as_str()? {
        "huawei" => Some("screen-length 0 temporary".into()),
        "cisco" => Some("terminal length 0".into()),
        "juniper" => Some("set cli screen-length 0".into()),
        "mikrotik" => Some("/terminal print".into()),
        "custom" => o
            .get("pagerCommand")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        _ => None,
    }
}

fn clipboard_text() -> Option<String> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    clipboard.get_text().ok()
}

/// Convert clipboard text into terminal paste bytes, respecting bracketed
/// paste so shells can distinguish a paste from typed input.
fn paste_bytes(mode: &TermMode, text: String) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        let mut out = Vec::with_capacity(text.len() + 12);
        out.extend_from_slice(b"\x1b[200~");
        out.extend_from_slice(text.replace("\r\n", "\n").as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        command_input::normalize(&text).into_bytes()
    }
}

/// Encode an xterm mouse report (SGR 1006 or legacy X10 encoding).
fn mouse_sequence(
    button: u8,
    column: usize,
    row: i32,
    release: bool,
    sgr: bool,
    ctx: &egui::Context,
) -> Vec<u8> {
    let modifiers = ctx.input(|i| i.modifiers);
    let mut code = button
        + u8::from(modifiers.shift) * 4
        + u8::from(modifiers.alt) * 8
        + u8::from(modifiers.ctrl || modifiers.command) * 16;
    let x = column + 1;
    let y = (row + 1).max(1) as usize;
    if sgr {
        if release {
            format!("\x1b[<{code};{x};{y}m").into_bytes()
        } else {
            format!("\x1b[<{code};{x};{y}M").into_bytes()
        }
    } else {
        if release {
            code = 3;
        }
        let mut out = vec![0x1b, b'[', b'M'];
        out.push(32 + code);
        out.push((32 + x).min(255) as u8);
        out.push((32 + y).min(255) as u8);
        out
    }
}

/// Map an OSC 10/11/4 color index to the actual palette in use.
fn terminal_query_color(colors: &appearance::Colors, index: usize) -> Rgb {
    let color = match index {
        0..=255 => colors.resolve(AnsiColor::Indexed(index as u8), false),
        256 => colors.resolve(AnsiColor::Named(NamedColor::Foreground), false),
        257 => colors.resolve(AnsiColor::Named(NamedColor::Background), true),
        258 => colors.resolve(AnsiColor::Named(NamedColor::Cursor), false),
        _ => colors.foreground,
    };
    Rgb {
        r: color.r(),
        g: color.g(),
        b: color.b(),
    }
}

/// xterm modifier parameter: 1 + Shift(1) + Alt(2) + Ctrl(4).
fn modifier_code(mods: egui::Modifiers) -> u8 {
    let mut code = 1;
    if mods.shift {
        code += 1;
    }
    if mods.alt {
        code += 2;
    }
    if mods.ctrl || mods.command {
        code += 4;
    }
    code
}

/// Cursor/Home/End style final-byte sequences. DECCKM (`?1h`, used by ncurses
/// applications such as ncdu, htop and `less`) switches the no-modifier form
/// from CSI to SS3. Sending CSI while the application expects SS3 is why arrow
/// keys appeared to do nothing or jump unpredictably.
fn cursor_key(final_byte: char, application: bool, mods: egui::Modifiers) -> Vec<u8> {
    let code = modifier_code(mods);
    if code == 1 {
        let introducer = if application { b'O' } else { b'[' };
        vec![0x1b, introducer, final_byte as u8]
    } else {
        format!("\x1b[1;{code}{final_byte}").into_bytes()
    }
}

fn tilde_key(number: u16, mods: egui::Modifiers) -> Vec<u8> {
    let code = modifier_code(mods);
    if code == 1 {
        format!("\x1b[{number}~").into_bytes()
    } else {
        format!("\x1b[{number};{code}~").into_bytes()
    }
}

fn function_key(number: u8, mods: egui::Modifiers) -> Option<Vec<u8>> {
    let code = modifier_code(mods);
    match number {
        1..=4 => {
            let final_byte = (b'P' + (number - 1)) as char;
            Some(if code == 1 {
                vec![0x1b, b'O', final_byte as u8]
            } else {
                format!("\x1b[1;{code}{final_byte}").into_bytes()
            })
        }
        5..=24 => {
            let number = match number {
                5 => 15,
                6 => 17,
                7 => 18,
                8 => 19,
                9 => 20,
                10 => 21,
                11 => 23,
                12 => 24,
                13 => 25,
                14 => 26,
                15 => 28,
                16 => 29,
                17 => 31,
                18 => 32,
                19 => 33,
                20 => 34,
                21 => 42,
                22 => 43,
                23 => 44,
                _ => 45,
            };
            Some(tilde_key(number, mods))
        }
        _ => None,
    }
}

fn ctrl_letter(key: egui::Key) -> Option<u8> {
    use egui::Key::*;
    Some(match key {
        A => 0x01,
        B => 0x02,
        C => 0x03,
        D => 0x04,
        E => 0x05,
        F => 0x06,
        G => 0x07,
        H => 0x08,
        I => 0x09,
        J => 0x0a,
        K => 0x0b,
        L => 0x0c,
        M => 0x0d,
        N => 0x0e,
        O => 0x0f,
        P => 0x10,
        Q => 0x11,
        R => 0x12,
        S => 0x13,
        T => 0x14,
        U => 0x15,
        V => 0x16,
        W => 0x17,
        X => 0x18,
        Y => 0x19,
        Z => 0x1a,
        _ => return None,
    })
}

fn key_to_bytes(key: egui::Key, mods: egui::Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    use egui::Key::*;
    let ctrl = mods.ctrl || mods.command;
    let app_cursor = mode.contains(TermMode::APP_CURSOR);

    if ctrl {
        if let Some(byte) = ctrl_letter(key) {
            let mut out = Vec::with_capacity(2);
            if mods.alt {
                out.push(0x1b);
            }
            out.push(byte);
            return Some(out);
        }
    }

    // Tabby/xterm 5 compatibility: desktop shells bind Alt+Left/Right only in
    // the Ctrl+Arrow word-jump form, so keep those sequences on Linux.
    #[cfg(target_os = "linux")]
    if mods.alt && !ctrl && !mods.shift {
        match key {
            ArrowLeft => return Some(b"\x1b[1;5D".to_vec()),
            ArrowRight => return Some(b"\x1b[1;5C".to_vec()),
            _ => {}
        }
    }

    Some(match key {
        Enter => {
            if mods.alt {
                b"\x1b\r".to_vec()
            } else {
                vec![b'\r']
            }
        }
        Backspace => vec![0x7f],
        Tab => {
            if mods.shift {
                b"\x1b[Z".to_vec()
            } else if mods.alt {
                b"\x1b\t".to_vec()
            } else {
                vec![b'\t']
            }
        }
        Escape => vec![0x1b],
        Space if ctrl => vec![0x00],
        ArrowUp => cursor_key('A', app_cursor, mods),
        ArrowDown => cursor_key('B', app_cursor, mods),
        ArrowRight => cursor_key('C', app_cursor, mods),
        ArrowLeft => cursor_key('D', app_cursor, mods),
        Home => cursor_key('H', app_cursor, mods),
        End => cursor_key('F', app_cursor, mods),
        Insert => tilde_key(2, mods),
        Delete => tilde_key(3, mods),
        PageUp => tilde_key(5, mods),
        PageDown => tilde_key(6, mods),
        F1 => function_key(1, mods)?,
        F2 => function_key(2, mods)?,
        F3 => function_key(3, mods)?,
        F4 => function_key(4, mods)?,
        F5 => function_key(5, mods)?,
        F6 => function_key(6, mods)?,
        F7 => function_key(7, mods)?,
        F8 => function_key(8, mods)?,
        F9 => function_key(9, mods)?,
        F10 => function_key(10, mods)?,
        F11 => function_key(11, mods)?,
        F12 => function_key(12, mods)?,
        F13 => function_key(13, mods)?,
        F14 => function_key(14, mods)?,
        F15 => function_key(15, mods)?,
        F16 => function_key(16, mods)?,
        F17 => function_key(17, mods)?,
        F18 => function_key(18, mods)?,
        F19 => function_key(19, mods)?,
        F20 => function_key(20, mods)?,
        F21 => function_key(21, mods)?,
        F22 => function_key(22, mods)?,
        F23 => function_key(23, mods)?,
        F24 => function_key(24, mods)?,
        Backslash if ctrl => vec![0x1c],
        CloseBracket if ctrl => vec![0x1d],
        // Ctrl+^ / Ctrl+_ share physical keys with 6 / - on many layouts.
        Num6 if ctrl => vec![0x1e],
        Minus if ctrl => vec![0x1f],
        _ => return None,
    })
}

#[cfg(target_os = "linux")]
fn trim_heap() {
    extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(target_os = "linux"))]
fn trim_heap() {}

fn apply_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = bg();
    visuals.window_fill = panel();
    visuals.extreme_bg_color = bg();
    visuals.faint_bg_color = card();
    visuals.selection.bg_fill = Color32::from_rgb(0x28, 0x28, 0x28);
    visuals.selection.stroke = Stroke::new(1.0, text_c());
    visuals.hyperlink_color = accent();
    visuals.override_text_color = Some(text_c());
    visuals.widgets.inactive.bg_fill = card();
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text_c());
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border());
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x28, 0x28, 0x28);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x28, 0x28, 0x28);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, accent());
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x60, 0x60, 0x60));
    visuals.widgets.active.bg_fill = Color32::from_rgb(0x32, 0x32, 0x32);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0x32, 0x32, 0x32);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, accent());
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border());
    visuals.window_corner_radius = CornerRadius::same(8);
    let r = CornerRadius::same(4);
    visuals.widgets.noninteractive.corner_radius = r;
    visuals.widgets.inactive.corner_radius = r;
    visuals.widgets.hovered.corner_radius = r;
    visuals.widgets.active.corner_radius = r;
    visuals.widgets.open.corner_radius = r;
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|st| {
        st.spacing.item_spacing = Vec2::new(8.0, 5.0);
        st.spacing.button_padding = Vec2::new(11.0, 6.0);
        st.spacing.interact_size.y = 30.0;
        st.text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(13.0));
        st.text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(15.0));
        st.text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.5));
        st.text_styles
            .insert(egui::TextStyle::Monospace, FontId::monospace(15.0));
        st.text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(20.0));
    });
}

fn bar_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(panel())
        .inner_margin(Margin::symmetric(10, 4))
        .stroke(Stroke::new(1.0, border()))
}

fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(panel())
        .inner_margin(Margin::symmetric(10, 8))
        .stroke(Stroke::new(1.0, border()))
}

fn card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(card())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(0))
        .inner_margin(Margin::symmetric(12, 8))
}

impl App {
    fn show(&mut self, ui: &mut egui::Ui) {
        if !self.fonts_ready {
            self.install_selected_fonts(ui.ctx());
        }
        if self.styled_theme != Some(self.theme_idx) {
            apply_style(ui.ctx());
            self.styled_theme = Some(self.theme_idx);
        }
        let ctx = ui.ctx().clone();
        self.context = Some(ctx.clone());
        self.close_confirmation.restore_previous_focus(&ctx);
        self.protect_window_close(&ctx);
        if self.close_confirmation.active() {
            self.close_blocked_frame = Some(ctx.cumulative_frame_nr());
        }
        self.close_input_blocked = self.close_confirmation.active()
            || self.close_blocked_frame == Some(ctx.cumulative_frame_nr());
        let close_drawn_early = self.close_input_blocked;
        if close_drawn_early {
            self.confirm_close(&ctx);
            close_confirmation::discard_background_keyboard(&ctx);
        }
        if (self.scroll_speed - self.applied_scroll_speed).abs() > 0.1 {
            ctx.options_mut(|options| {
                options.input_options.line_scroll_speed = self.scroll_speed * 2.0;
            });
            self.applied_scroll_speed = self.scroll_speed;
        }
        self.updater.maybe_auto_check();
        if self.updater.poll() {
            ctx.request_repaint();
        }
        if self.updater.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(at) = self.heap_trim_at {
            if Instant::now() >= at {
                self.heap_trim_at = None;
                trim_heap();
            }
        }
        if self.runtime_rss_mib == 0 || self.last_memory_check.elapsed() >= Duration::from_secs(1) {
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                self.runtime_rss_mib = status
                    .lines()
                    .find_map(|line| line.strip_prefix("VmRSS:"))
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0)
                    / 1024;
            }
            self.last_memory_check = Instant::now();
            self.check_vault_expiry();
        }
        if self
            .sessions
            .iter()
            .any(|s| s.bell.swap(false, Ordering::Relaxed))
        {
            self.last_bell = Some(Instant::now());
        }
        for s in &self.sessions {
            s.wake.bind(&ctx);
            if let Some(status) = &s.io_status {
                status.bind(&ctx);
            }
        }
        for p in &self.pending_connections {
            p.task.bind(&ctx);
        }
        if let Some(p) = &self.sftp_pending {
            p.task.bind(&ctx);
        }

        if !self.close_input_blocked && !self.close_confirmation.active() {
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::T)
            }) {
                self.new_local();
            }
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::P)
            }) {
                self.composer_open = !self.composer_open;
            }
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::W)
            }) {
                self.request_close_tab(self.active);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Comma)) {
                self.show_settings = true;
            }
        }
        let mut zoom_changed = false;
        ctx.input(|i| {
            if i.modifiers.ctrl || i.modifiers.command {
                if i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus) {
                    self.font_size = (self.font_size + 1.0).min(30.0);
                    zoom_changed = true;
                } else if i.key_pressed(egui::Key::Minus) {
                    self.font_size = (self.font_size - 1.0).max(10.0);
                    zoom_changed = true;
                } else if i.key_pressed(egui::Key::Num0) {
                    self.font_size = DEFAULT_TERM_FONT;
                    zoom_changed = true;
                }
            }
        });
        if zoom_changed {
            self.save_ui_settings();
        }
        self.poll_connections();
        self.pump_sessions();
        self.handle_session_events(&ctx);
        ctx.request_repaint_after(Duration::from_secs(1));
        if self.auto_reconnect {
            for i in 0..self.sessions.len() {
                if self.sessions[i].exited && !self.sessions[i].reconnect_tried {
                    self.sessions[i].reconnect_tried = true;
                    self.reconnect_at(i);
                }
            }
        }
        self.tick_runner();
        if !self.runners.is_empty()
            || self.dns_rx.is_some()
            || self.trace_rx.is_some()
            || self.ping_rx.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if let Some(rx) = self.dns_rx.take() {
            match rx.try_recv() {
                Ok(text) => self.dns_out = text,
                Err(_) => self.dns_rx = Some(rx),
            }
        }
        if let Some(rx) = self.trace_rx.take() {
            match rx.try_recv() {
                Ok(text) => self.trace_out = text,
                Err(_) => self.trace_rx = Some(rx),
            }
        }
        if let Some(rx) = self.ping_rx.take() {
            match rx.try_recv() {
                Ok(text) => self.ping_out = text,
                Err(_) => self.ping_rx = Some(rx),
            }
        }
        self.poll_sftp();

        // Top bar: brand + tabs + actions.
        // macOS-style menu bar.
        egui::Panel::top("menubar")
            .exact_size(46.0)
            .frame(
                egui::Frame::NONE
                    .fill(panel())
                    .inner_margin(Margin::symmetric(14, 8))
                    .stroke(Stroke::new(1.0, border())),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        egui::RichText::new("OpsTerm")
                            .size(17.0)
                            .strong()
                            .color(text_c()),
                    );
                    ui.add_space(10.0);
                    ui.separator();
                    ui.menu_button("File", |ui| {
                        if ui.button("Refresh devices").clicked() {
                            self.refresh_devices();
                            ui.close();
                        }
                        if ui.button("New Device").clicked() {
                            self.editor = Some(EditorState::new());
                            ui.close();
                        }
                        if ui.button("Import devices").clicked() {
                            self.import_open = true;
                            ui.close();
                        }
                        if ui.button("Workspaces").clicked() {
                            self.show_ws = true;
                            self.workspaces = self.db.list_workspaces().unwrap_or_default();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Settings…").clicked() {
                            self.show_settings = true;
                            ui.close();
                        }
                    });
                    ui.menu_button("View", |ui| {
                        ui.checkbox(&mut self.split, "Split (2 panes)");
                        ui.checkbox(&mut self.grid, "Grid (2x2)");
                        ui.checkbox(&mut self.composer_open, "Command composer (Ctrl+Shift+P)");
                        ui.checkbox(&mut self.show_scratch, "Scratchpad");
                        ui.checkbox(&mut self.highlight, "Keyword highlight");
                        if ui
                            .checkbox(&mut self.auto_reconnect, "Auto-reconnect")
                            .changed()
                        {
                            self.save_ui_settings();
                        }
                        ui.checkbox(&mut self.search_open, "Find bar");
                        ui.separator();
                        if ui.button("Highlight rules…").clicked() {
                            self.show_hl = true;
                            ui.close();
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Font");
                            if ui.small_button("−").clicked() {
                                self.font_size = (self.font_size - 1.0).max(10.0);
                                self.save_ui_settings();
                            }
                            ui.label(format!("{:.0}", self.font_size));
                            if ui.small_button("+").clicked() {
                                self.font_size = (self.font_size + 1.0).min(30.0);
                                self.save_ui_settings();
                            }
                        });
                        ui.menu_button("Theme", |ui| {
                            for (index, name) in THEME_NAMES.iter().enumerate() {
                                if ui
                                    .selectable_label(self.theme_idx == index, *name)
                                    .clicked()
                                {
                                    self.theme_idx = index;
                                    set_theme(theme_for(index));
                                    self.save_ui_settings();
                                    ui.close();
                                }
                            }
                        });
                    });
                    ui.menu_button("Session", |ui| {
                        if ui.button("New local shell").clicked() {
                            self.new_local();
                            ui.close();
                        }
                        if ui.button("Reconnect").clicked() {
                            self.reconnect_active();
                            ui.close();
                        }
                        let log_on = self
                            .sessions
                            .get(self.active)
                            .map(|s| s.log_path.is_some())
                            .unwrap_or(false);
                        if ui
                            .button(if log_on {
                                "Stop logging"
                            } else {
                                "Start logging"
                            })
                            .clicked()
                        {
                            if let Some(s) = self.sessions.get_mut(self.active) {
                                s.toggle_log();
                            }
                            ui.close();
                        }
                        if ui.button("Export output…").clicked() {
                            self.export_output();
                            ui.close();
                        }
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("Check for updates…").clicked() {
                            self.updater.show = true;
                            self.updater.start_check(true);
                            ui.close();
                        }
                        if ui.button("Keyboard shortcuts").clicked() {
                            self.show_help = true;
                            ui.close();
                        }
                    });
                    ui.menu_button("Tools", |ui| {
                        if ui.button("SFTP browser").clicked() {
                            self.show_sftp = true;
                            ui.close();
                        }
                        if ui.button("Tunnels").clicked() {
                            self.show_tunnels = true;
                            ui.close();
                        }
                        if ui.button("Runbooks").clicked() {
                            self.show_runbooks = true;
                            self.runbooks = self.db.list_runbooks().unwrap_or_default();
                            ui.close();
                        }
                        if ui.button("Subnet calculator").clicked() {
                            self.show_subnet = true;
                            ui.close();
                        }
                        if ui.button("Ping").clicked() {
                            self.show_ping = true;
                            ui.close();
                        }
                        if ui.button("Traceroute").clicked() {
                            self.show_trace = true;
                            ui.close();
                        }
                        if ui.button("DNS lookup").clicked() {
                            self.show_dns = true;
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Credentials…").clicked() {
                            self.show_credentials = true;
                            ui.close();
                        }
                        if self.vault.is_some() && ui.button("Lock vault").clicked() {
                            self.lock_vault();
                            ui.close();
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let connect = ui
                            .add_enabled(
                                !self.quick.trim().is_empty(),
                                egui::Button::new(
                                    egui::RichText::new("Connect").strong().color(bg()),
                                )
                                .fill(accent()),
                            )
                            .clicked();
                        let width = ui.available_width().clamp(100.0, 340.0);
                        let q = ui.add_sized(
                            [width, 30.0],
                            egui::TextEdit::singleline(&mut self.quick)
                                .id(egui::Id::new("quick-connect"))
                                .hint_text("user@host  ·  Quick connect"),
                        );
                        if connect
                            || (q.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            self.connect_quick();
                        }
                    });
                });
            });

        egui::Panel::top("tabbar")
            .exact_size(42.0)
            .frame(bar_frame())
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let mut remove = None;
                    let mut reorder = None;
                    egui::ScrollArea::horizontal()
                        .id_salt("session-tabs")
                        .max_width((ui.available_width() - 112.0).max(80.0))
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for i in 0..self.sessions.len() {
                                    let selected = i == self.active;
                                    let session = &self.sessions[i];
                                    let symbol = if session.opening {
                                        "…"
                                    } else if session.exited {
                                        "○"
                                    } else if session.log_path.is_some() {
                                        "●"
                                    } else {
                                        "›"
                                    };
                                    let label = format!("{symbol} {}", session.title);
                                    let color = if selected { text_c() } else { muted() };
                                    let response = ui
                                        .push_id(("tab", session.id), |ui| {
                                            ui.add(
                                                egui::Button::selectable(
                                                    selected,
                                                    egui::RichText::new(label).color(color),
                                                )
                                                .sense(egui::Sense::click_and_drag())
                                                .corner_radius(3)
                                                .min_size(Vec2::new(112.0, 28.0)),
                                            )
                                        })
                                        .inner;
                                    if selected {
                                        ui.painter().hline(
                                            response.rect.x_range(),
                                            response.rect.bottom() - 1.0,
                                            Stroke::new(2.0, accent()),
                                        );
                                    }
                                    response.dnd_set_drag_payload(session.id);
                                    if response.clicked() {
                                        self.active = i;
                                        self.terminal_focus = Some(self.sessions[i].id);
                                    }
                                    if let Some(payload) = response.dnd_release_payload::<u64>() {
                                        if *payload != self.sessions[i].id {
                                            reorder = Some((*payload, self.sessions[i].id));
                                        }
                                    }
                                    let (close_rect, close_response) = ui.allocate_exact_size(
                                        Vec2::splat(20.0),
                                        egui::Sense::click(),
                                    );
                                    let close_hovered = close_response.hovered();
                                    if close_hovered {
                                        ui.painter().rect_filled(
                                            close_rect,
                                            3.0,
                                            Color32::from_rgb(0x4A, 0x1E, 0x24),
                                        );
                                    }
                                    ui.painter().text(
                                        close_rect.center(),
                                        Align2::CENTER_CENTER,
                                        "×",
                                        FontId::proportional(15.0),
                                        if close_hovered {
                                            Color32::from_rgb(0xFF, 0x6B, 0x7A)
                                        } else {
                                            muted()
                                        },
                                    );
                                    if close_response
                                        .on_hover_text(format!("Close {}", self.sessions[i].title))
                                        .clicked()
                                    {
                                        remove = Some(self.sessions[i].id);
                                    }
                                }
                            });
                        });
                    if let Some((from, to)) = reorder {
                        if let (Some(from), Some(to)) = (
                            self.sessions.iter().position(|s| s.id == from),
                            self.sessions.iter().position(|s| s.id == to),
                        ) {
                            self.move_tab(from, to);
                        }
                    }
                    if let Some(id) = remove {
                        if let Some(i) = self.sessions.iter().position(|s| s.id == id) {
                            self.request_close_tab(i);
                        }
                    }
                    if ui
                        .button("+ Terminal")
                        .on_hover_text("New local shell (Ctrl+Shift+T)")
                        .clicked()
                    {
                        self.new_local();
                    }
                });
            });

        // Command / Button bar (from the command library).
        if self.composer_open && !self.commands.is_empty() {
            egui::Panel::top("buttons")
                .exact_size(34.0)
                .frame(bar_frame())
                .show(ui, |ui| {
                    egui::ScrollArea::horizontal()
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                let mut send = None;
                                for c in &self.commands {
                                    let label = c.name.clone();
                                    let content = c.content.clone();
                                    if ui.small_button(label).clicked() {
                                        send = Some(content);
                                    }
                                }
                                if let Some(content) = send {
                                    self.send_raw(&content);
                                }
                            });
                        });
                });
        }

        egui::Panel::bottom("status")
            .exact_size(36.0)
            .frame(
                egui::Frame::NONE
                    .fill(panel())
                    .inner_margin(Margin::symmetric(12, 3))
                    .stroke(Stroke::new(1.0, border())),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(session) = self.sessions.get(self.active) {
                        let state = if session.opening {
                            "Connecting"
                        } else if session.exited {
                            "Disconnected"
                        } else {
                            "Connected"
                        };
                        let state_color = if session.opening {
                            accent()
                        } else if session.exited {
                            muted()
                        } else {
                            ok()
                        };
                        let (dot, _) =
                            ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot.center(), 3.0, state_color);
                        ui.label(egui::RichText::new(state).size(13.0).color(muted()));
                        ui.add_sized(
                            [130.0, 20.0],
                            egui::Label::new(egui::RichText::new(&session.title).size(13.0))
                                .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(format!("{}×{}", session.cols, session.rows))
                                .size(13.0)
                                .color(muted()),
                        );
                        let secs = session.connected_at.elapsed().as_secs();
                        ui.label(
                            egui::RichText::new(format!("{:02}:{:02}", secs / 60, secs % 60))
                                .size(13.0)
                                .color(muted()),
                        );
                    } else {
                        ui.label(egui::RichText::new("Ready").size(13.0).color(muted()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .selectable_label(self.composer_open, "Command")
                            .on_hover_text("Show command composer (Ctrl+Shift+P)")
                            .clicked()
                        {
                            self.composer_open = !self.composer_open;
                        }
                        if ui.button("Reconnect").clicked() {
                            self.reconnect_active();
                        }
                        ui.label(
                            egui::RichText::new(format!("{} MiB", self.runtime_rss_mib))
                                .size(13.0)
                                .color(muted()),
                        )
                        .on_hover_text("Process resident memory (RSS)");
                        if self
                            .last_bell
                            .is_some_and(|t| t.elapsed() < Duration::from_secs(3))
                        {
                            ui.colored_label(accent(), "Bell");
                        }
                        ui.label(
                            egui::RichText::new(format!("{} tabs", self.sessions.len()))
                                .size(13.0)
                                .color(muted()),
                        );
                    });
                });
            });

        // Bottom: editor-like command input panel.
        if self.composer_open {
            egui::Panel::bottom("cmdinput")
                .exact_size(104.0)
                .frame(card_frame())
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("COMMAND").small().color(muted()));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let can_send = !self.command.trim().is_empty()
                                && self.sessions.iter().enumerate().any(|(i, s)| {
                                    !s.exited
                                        && !s.opening
                                        && if self.broadcast {
                                            s.broadcast_selected
                                        } else {
                                            i == self.active
                                        }
                                });
                            if ui
                                .add_enabled(
                                    can_send,
                                    egui::Button::new(
                                        egui::RichText::new("Send").strong().color(bg()),
                                    )
                                    .fill(accent()),
                                )
                                .on_hover_text("Enter sends; Shift+Enter adds a line")
                                .clicked()
                            {
                                self.send_command(true);
                            }
                            if ui
                                .add_enabled(can_send, egui::Button::new("Insert"))
                                .on_hover_text("Send text without executing")
                                .clicked()
                            {
                                self.send_command(false);
                            }
                            ui.add_space(6.0);
                            if ui
                                .selectable_label(self.broadcast, "Broadcast")
                                .on_hover_text("Send to selected targets")
                                .clicked()
                            {
                                self.broadcast = !self.broadcast;
                            }
                            if ui.small_button("Targets").clicked() {
                                self.show_targets = true;
                            }
                            if self.broadcast {
                                let n = self
                                    .sessions
                                    .iter()
                                    .filter(|s| s.broadcast_selected && !s.exited && !s.opening)
                                    .count();
                                ui.label(
                                    egui::RichText::new(format!("{n} targets"))
                                        .small()
                                        .color(Color32::from_rgb(229, 192, 123)),
                                );
                            } else if let Some(s) = self.sessions.get(self.active) {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("To: {}", s.title))
                                            .small()
                                            .color(muted()),
                                    )
                                    .truncate(),
                                );
                            }
                        });
                    });
                    if command_input::show(ui, &mut self.command) {
                        self.send_command(true);
                    }
                });
        }

        // Left: devices.
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(244.0)
            .min_size(190.0)
            .max_size(360.0)
            .frame(panel_frame())
            .show(ui, |ui| {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("DEVICES").small().color(muted()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button("+ New")
                            .on_hover_text("Add a saved device")
                            .clicked()
                        {
                            self.editor = Some(EditorState::new());
                        }
                        if ui
                            .small_button("+ Folder")
                            .on_hover_text("Create a folder")
                            .clicked()
                        {
                            self.folder_editor = Some(FolderEditorState::new(None));
                        }
                    });
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.dev_filter)
                        .hint_text("Search devices…")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{} devices", self.devices.len()))
                            .small()
                            .color(muted()),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Refresh").clicked() {
                            self.refresh_folders();
                            self.refresh_devices();
                        }
                    });
                });
                ui.add_space(4.0);
                if self.applied_filter != self.dev_filter {
                    self.refilter_devices();
                }
                let active_saved = self
                    .sessions
                    .get(self.active)
                    .and_then(|s| s.saved_id.as_deref());
                let folder_choices = self.folder_choices();
                let action = if self.device_indices.is_empty() {
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new(if self.devices.is_empty() {
                            "No devices yet"
                        } else {
                            "No matching devices"
                        })
                        .color(muted()),
                    );
                    ui.label(
                        egui::RichText::new(if self.devices.is_empty() {
                            "Add a device to connect in one click."
                        } else {
                            "Try a different name, host, or tag."
                        })
                        .small()
                        .color(muted()),
                    );
                    None
                } else if self.dev_filter.trim().is_empty() {
                    device_list::show_tree(
                        ui,
                        &self.devices,
                        &self.device_indices,
                        &self.folders,
                        active_saved,
                        &folder_choices,
                    )
                } else {
                    device_list::show(
                        ui,
                        &self.devices,
                        &self.device_indices,
                        active_saved,
                        &folder_choices,
                    )
                };
                if let Some(action) = action {
                    match action {
                        device_list::Action::Activate(i) => {
                            let device = self.devices[i].clone();
                            self.open_saved(&device);
                        }
                        device_list::Action::OpenNew(i) => {
                            let device = self.devices[i].clone();
                            self.open_saved_new(&device);
                        }
                        device_list::Action::Edit(i) => {
                            self.editor = Some(EditorState::from_saved(&self.devices[i]))
                        }
                        device_list::Action::PickFolder(i) => {
                            self.folder_pick = Some(FolderPick {
                                index: i,
                                name: self.devices[i].name.clone(),
                                filter: String::new(),
                            });
                        }
                        device_list::Action::NewSubfolder(parent_id) => {
                            self.folder_editor = Some(FolderEditorState::new(Some(parent_id)));
                        }
                        device_list::Action::RenameFolder(id) => {
                            if let Some(folder) =
                                self.folders.iter().find(|folder| folder.id == id).cloned()
                            {
                                self.folder_editor = Some(FolderEditorState::rename(&folder));
                            }
                        }
                        device_list::Action::DeleteFolder(id) => {
                            if let Some(folder) =
                                self.folders.iter().find(|folder| folder.id == id).cloned()
                            {
                                self.folder_delete = Some(folder);
                            }
                        }
                    }
                }
            });

        // Right: scratchpad (temporary notes for copy/paste).
        if self.show_scratch {
            egui::Panel::right("scratch")
                .resizable(true)
                .default_size(300.0)
                .frame(panel_frame())
                .show(ui, |ui| {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("SCRATCHPAD").small().color(muted()));
                    ui.add_space(4.0);
                    let add = ui.add(
                        egui::TextEdit::multiline(&mut self.scratch_input)
                            .hint_text("paste / type a note…")
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .font(FontId::monospace(12.5)),
                    );
                    if (ui.button("Add note").clicked()
                        || (add.has_focus()
                            && ctx.input(|i| {
                                i.key_pressed(egui::Key::Enter)
                                    && (i.modifiers.ctrl || i.modifiers.command)
                            })))
                        && !self.scratch_input.trim().is_empty()
                    {
                        self.scratch.push(self.scratch_input.trim_end().to_string());
                        self.scratch_input.clear();
                        save_scratch(&self.scratch);
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    let mut delete: Option<usize> = None;
                    let mut copy: Option<String> = None;
                    let mut send: Option<String> = None;
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.scratch.is_empty() {
                                ui.label(
                                    egui::RichText::new("No notes yet").small().color(muted()),
                                );
                            }
                            for i in 0..self.scratch.len() {
                                let note = self.scratch[i].clone();
                                egui::Frame::NONE
                                    .fill(card())
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(Margin::symmetric(9, 7))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(
                                            egui::RichText::new(&note).monospace().color(text_c()),
                                        );
                                        ui.horizontal(|ui| {
                                            if ui.small_button("Copy").clicked() {
                                                copy = Some(note.clone());
                                            }
                                            if ui.small_button("Send ⏎").clicked() {
                                                send = Some(note.clone());
                                            }
                                            if ui.small_button("Delete").clicked() {
                                                delete = Some(i);
                                            }
                                        });
                                    });
                                ui.add_space(3.0);
                            }
                        });
                    if let Some(text) = copy {
                        self.copy_to_clipboard(&ctx, text);
                    }
                    if let Some(text) = send {
                        if let Some(sess) = self.sessions.get(self.active) {
                            let mut bytes = text.into_bytes();
                            bytes.push(b'\r');
                            if let Err(error) = sess.write(&bytes) {
                                self.flash = error.to_string();
                            }
                        }
                    }
                    if let Some(i) = delete {
                        self.scratch.remove(i);
                        save_scratch(&self.scratch);
                    }
                });
        }

        // Status strip.
        // Centre: terminal inside rounded card(s).
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::NONE.fill(bg()).inner_margin(Margin::same(0)))
            .show(ui, |ui| {
                let make_card = || {
                    egui::Frame::NONE
                        .fill(bg())
                        .inner_margin(Margin::symmetric(12, 10))
                };
                if self.grid && self.sessions.len() >= 2 {
                    ui.columns(2, |cols| {
                        let n = self.sessions.len();
                        for (ci, col) in cols.iter_mut().enumerate() {
                            let idx_a = ci * 2;
                            let idx_b = ci * 2 + 1;
                            let h = (col.available_height() - 6.0) / 2.0;
                            let w = col.available_width();
                            if idx_a < n {
                                col.allocate_ui_with_layout(
                                    Vec2::new(w, h),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        make_card().show(ui, |ui| {
                                            ui.set_min_size(ui.available_size());
                                            self.terminal_pane(ui, idx_a);
                                        });
                                    },
                                );
                            }
                            if idx_b < n {
                                col.allocate_ui_with_layout(
                                    Vec2::new(w, h),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        make_card().show(ui, |ui| {
                                            ui.set_min_size(ui.available_size());
                                            self.terminal_pane(ui, idx_b);
                                        });
                                    },
                                );
                            }
                        }
                    });
                    return;
                }
                if self.split && self.sessions.len() >= 2 {
                    let total_w = ui.available_width();
                    let h = ui.available_height();
                    let w1 = (total_w * self.split_ratio).clamp(120.0, total_w - 126.0);
                    let w2 = (total_w - w1 - 6.0).max(120.0);
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(w1, h),
                            Layout::top_down(Align::Min),
                            |ui| {
                                make_card().show(ui, |ui| {
                                    ui.set_min_size(ui.available_size());
                                    self.terminal_pane(ui, 0);
                                });
                            },
                        );
                        let div = ui.allocate_response(Vec2::new(6.0, h), egui::Sense::drag());
                        if div.dragged() {
                            self.split_ratio =
                                (self.split_ratio + div.drag_delta().x / total_w).clamp(0.2, 0.8);
                        }
                        ui.allocate_ui_with_layout(
                            Vec2::new(w2, h),
                            Layout::top_down(Align::Min),
                            |ui| {
                                make_card().show(ui, |ui| {
                                    ui.set_min_size(ui.available_size());
                                    self.terminal_pane(ui, 1);
                                });
                            },
                        );
                    });
                    return;
                }
                make_card().show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    if self.search_open {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("FIND").small().color(muted()));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.search)
                                    .hint_text("search output…")
                                    .desired_width(260.0),
                            );
                            if ui.button("✕").clicked() {
                                self.search.clear();
                                self.search_open = false;
                            }
                        });
                        ui.separator();
                    }
                    self.terminal_pane(ui, self.active.min(self.sessions.len().saturating_sub(1)));
                });
            });

        if self.last_flash != self.flash {
            self.last_flash.clone_from(&self.flash);
            self.flash_until = Instant::now() + Duration::from_secs(6);
        }
        if !self.flash.is_empty()
            && self.flash != "connection opened"
            && !self.flash.starts_with("queued connection")
            && Instant::now() < self.flash_until
        {
            egui::Area::new(egui::Id::new("notification"))
                .anchor(
                    egui::Align2::RIGHT_BOTTOM,
                    Vec2::new(-16.0, if self.composer_open { -138.0 } else { -42.0 }),
                )
                .show(&ctx, |ui| {
                    egui::Frame::NONE
                        .fill(card())
                        .stroke(Stroke::new(1.0, border()))
                        .corner_radius(6)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.set_max_width(380.0);
                            ui.label(&self.flash);
                        });
                });
            ctx.request_repaint_after(self.flash_until.saturating_duration_since(Instant::now()));
        }

        // Device editor window. State is taken out, edited, and put back so
        // typed text persists across frames (a clone would revert every key).
        if let Some(mut ed) = self.editor.take() {
            let mut close = false;
            let mut save = false;
            let title = if ed.id.is_some() {
                "Edit Device"
            } else {
                "New Device"
            };
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut ed.name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Folder");
                        let choices = self.folder_choices();
                        let selected = choices
                            .iter()
                            .find(|(_, id)| id.as_deref() == ed.folder_id.as_deref())
                            .map(|(label, _)| label.clone())
                            .unwrap_or_else(|| "No folder".into());
                        egui::ComboBox::from_id_salt("ed-folder")
                            .selected_text(selected)
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for (label, id) in &choices {
                                    if ui.selectable_label(ed.folder_id == *id, label).clicked() {
                                        ed.folder_id = id.clone();
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Vendor");
                        egui::ComboBox::from_id_salt("ed-vendor")
                            .selected_text(
                                VENDORS
                                    .get(self.vendor_idx)
                                    .map(|v| v.label)
                                    .unwrap_or("(none)"),
                            )
                            .show_ui(ui, |ui| {
                                for (i, v) in VENDORS.iter().enumerate() {
                                    if ui.selectable_label(self.vendor_idx == i, v.label).clicked()
                                    {
                                        self.vendor_idx = i;
                                        if let Some((p, b, l)) = self.apply_vendor(v.label) {
                                            ed.pager = p;
                                            ed.backspace = b;
                                            if ed.login_script.trim().is_empty() {
                                                ed.login_script = l;
                                            }
                                        }
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label(if ed.protocol == "serial" {
                            "Device"
                        } else {
                            "Host"
                        });
                        ui.text_edit_singleline(&mut ed.host);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Port");
                        ui.add(egui::DragValue::new(&mut ed.port).range(1..=65535));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Protocol");
                        egui::ComboBox::from_id_salt("ed-proto")
                            .selected_text(ed.protocol.to_uppercase())
                            .show_ui(ui, |ui| {
                                for p in ["ssh", "telnet", "serial"] {
                                    ui.selectable_value(
                                        &mut ed.protocol,
                                        p.to_string(),
                                        p.to_uppercase(),
                                    );
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Username");
                        ui.text_edit_singleline(&mut ed.username);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Credential");
                        let creds = self.db.list_credentials().unwrap_or_default();
                        let selected = if ed.credential.is_empty() {
                            "(prompt each time)".to_string()
                        } else {
                            ed.credential.clone()
                        };
                        egui::ComboBox::from_id_salt("ed-cred")
                            .selected_text(selected)
                            .width(260.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut ed.credential,
                                    String::new(),
                                    "(prompt each time)",
                                );
                                for c in &creds {
                                    let label = if c.tags.is_empty() {
                                        c.name.clone()
                                    } else {
                                        format!("{}  #{}", c.name, c.tags.join(" #"))
                                    };
                                    ui.selectable_value(&mut ed.credential, c.name.clone(), label);
                                }
                            });
                        if let Some(c) = creds.iter().find(|c| c.name == ed.credential) {
                            let who = c.username.clone().unwrap_or_else(|| "-".into());
                            ui.label(
                                egui::RichText::new(format!("user {who}"))
                                    .small()
                                    .color(muted()),
                            );
                            if !c.tags.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!("#{}", c.tags.join(" #")))
                                        .small()
                                        .color(accent()),
                                );
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Pager");
                        egui::ComboBox::from_id_salt("ed-pager")
                            .selected_text(ed.pager.clone())
                            .show_ui(ui, |ui| {
                                for p in ["none", "huawei", "cisco", "juniper", "mikrotik"] {
                                    ui.selectable_value(&mut ed.pager, p.to_string(), p);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Backspace");
                        egui::ComboBox::from_id_salt("ed-bs")
                            .selected_text(ed.backspace.clone())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut ed.backspace,
                                    "del".to_string(),
                                    "DEL (0x7F)",
                                );
                                ui.selectable_value(
                                    &mut ed.backspace,
                                    "bs".to_string(),
                                    "BS (0x08)",
                                );
                            });
                    });
                    ui.label(
                        egui::RichText::new("Login script (expect): send/sendline/waitfor/sleep")
                            .small()
                            .color(muted()),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut ed.login_script)
                            .desired_rows(4)
                            .desired_width(440.0)
                            .font(FontId::monospace(12.0)),
                    );
                    if let Some(error) = &ed.error {
                        ui.colored_label(Color32::from_rgb(224, 108, 117), error);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            save = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if save {
                if self.save_editor(&ed) {
                    close = true;
                } else {
                    ed.error = Some(self.flash.clone());
                }
            }
            if close {
                self.editor = None;
            } else {
                self.editor = Some(ed);
            }
        }

        // Import window.
        if self.import_open {
            let mut open = true;
            egui::Window::new("Import devices")
                .collapsible(false)
                .resizable(true)
                .open(&mut open)
                .show(&ctx, |ui| {
                    ui.label(
                        egui::RichText::new(
                            "CSV rows: name,host,port,protocol,username,credential",
                        )
                        .small()
                        .color(muted()),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut self.import_text)
                            .desired_rows(5)
                            .desired_width(480.0)
                            .font(FontId::monospace(12.0)),
                    );
                    if ui.button("Import CSV").clicked() {
                        self.import_csv();
                    }
                    if !self.import_msg.is_empty() {
                        ui.label(egui::RichText::new(&self.import_msg).color(ok()));
                    }
                });
            if !open {
                self.import_open = false;
            }
        }

        // Tunnels window.
        if self.show_tunnels {
            let mut open = true;
            let devs = self.ssh_devices();
            egui::Window::new("Tunnels")
                .open(&mut open)
                .resizable(true)
                .default_width(600.0)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Device");
                        let sel = devs
                            .get(self.t_session)
                            .map(|d| d.name.clone())
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("tun-dev")
                            .selected_text(sel)
                            .show_ui(ui, |ui| {
                                for (i, d) in devs.iter().enumerate() {
                                    ui.selectable_value(&mut self.t_session, i, &d.name);
                                }
                            });
                        ui.label("Kind");
                        egui::ComboBox::from_id_salt("tun-kind")
                            .selected_text(self.t_kind.clone())
                            .show_ui(ui, |ui| {
                                for k in ["local", "remote", "dynamic"] {
                                    ui.selectable_value(&mut self.t_kind, k.to_string(), k);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Bind");
                        ui.add(egui::TextEdit::singleline(&mut self.t_bind).desired_width(140.0));
                        ui.add(egui::DragValue::new(&mut self.t_bind_port).range(1..=65535));
                        if self.t_kind != "dynamic" {
                            ui.label("Dest");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.t_dest).desired_width(140.0),
                            );
                            ui.add(egui::DragValue::new(&mut self.t_dest_port).range(1..=65535));
                        }
                        if ui.button("Start tunnel").clicked() {
                            self.tunnel_start();
                        }
                    });
                    ui.separator();
                    let views = self.tunnels.list();
                    if views.is_empty() {
                        ui.label(egui::RichText::new("No tunnels running").color(muted()));
                    }
                    for v in views {
                        ui.horizontal(|ui| {
                            ui.label(if v.running { "●" } else { "✕" });
                            ui.label(format!(
                                "{} {} {} via {}",
                                v.kind.to_uppercase(),
                                v.bind,
                                v.dest.clone().unwrap_or_default(),
                                v.ssh_host
                            ));
                            if ui.small_button("Stop").clicked() {
                                let _ = self.tunnels.stop(&v.id);
                            }
                        });
                    }
                });
            if !open {
                self.show_tunnels = false;
            }
        }

        // SFTP window.
        if self.show_sftp {
            let mut open = true;
            let devs = self.ssh_devices();
            let selection_before = (self.sftp_session, self.sftp_path.clone());
            let selected_location = devs
                .get(self.sftp_session)
                .map(|d| (d.id.clone(), self.sftp_path.clone()));
            if self.sftp_location != selected_location {
                self.sftp_entries.clear();
            }
            let entries = self.sftp_entries.clone();
            let busy = self.sftp_pending.is_some();
            let mut delete: Option<(String, bool)> = None;
            let mut download: Option<String> = None;
            egui::Window::new("SFTP")
                .open(&mut open)
                .resizable(true)
                .default_width(660.0)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Device");
                        let sel = devs
                            .get(self.sftp_session)
                            .map(|d| d.name.clone())
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("sftp-dev")
                            .selected_text(sel)
                            .show_ui(ui, |ui| {
                                for (i, d) in devs.iter().enumerate() {
                                    ui.selectable_value(&mut self.sftp_session, i, &d.name);
                                }
                            });
                        if ui.button("Browse").clicked() {
                            self.sftp_browse();
                        }
                        ui.label("Path");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.sftp_path).desired_width(220.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Up").clicked() {
                            self.sftp_path = normalize_path(&self.sftp_path);
                            let p = self.sftp_path.trim_end_matches('/').to_string();
                            self.sftp_path = match p.rfind('/') {
                                Some(0) | None => "/".to_string(),
                                Some(i) => p[..i].to_string(),
                            };
                            self.sftp_browse();
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.sftp_new)
                                .hint_text("new folder")
                                .desired_width(160.0),
                        );
                        if ui.button("+ Folder").clicked() {
                            self.sftp_mkdir();
                        }
                    });
                    ui.separator();
                    let same_directory =
                        selection_before == (self.sftp_session, self.sftp_path.clone());
                    ui.add_enabled_ui(!busy && same_directory, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(340.0)
                            .show(ui, |ui| {
                                for e in &entries {
                                    ui.horizontal(|ui| {
                                        ui.label(if e.is_dir { "📁" } else { "📄" });
                                        let label = e
                                            .path
                                            .rsplit('/')
                                            .next()
                                            .unwrap_or(&e.name)
                                            .to_string();
                                        if ui.selectable_label(false, label).clicked() && e.is_dir {
                                            self.sftp_path = normalize_path(&e.path);
                                            self.sftp_browse();
                                        }
                                        if !e.is_dir && ui.small_button("Download").clicked() {
                                            download = Some(e.path.clone());
                                        }
                                        if ui.small_button("Delete").clicked() {
                                            delete = Some((e.path.clone(), e.is_dir));
                                        }
                                    });
                                }
                            });
                    });
                    if !self.sftp_msg.is_empty() {
                        ui.label(egui::RichText::new(&self.sftp_msg).small().color(muted()));
                    }
                });
            if selection_before != (self.sftp_session, self.sftp_path.clone()) {
                self.sftp_entries.clear();
            }
            if let Some((path, is_dir)) = delete {
                self.sftp_remove(&path, is_dir);
            }
            if let Some(remote) = download {
                self.sftp_download(&remote);
            }
            if !open {
                self.show_sftp = false;
            }
        }

        // Runbooks window (expect scripts).
        if self.show_runbooks {
            let mut open = true;
            let list = self.runbooks.clone();
            let mut load: Option<Runbook> = None;
            let mut delete: Option<String> = None;
            let mut run = false;
            let mut save = false;
            egui::Window::new("Runbooks")
                .open(&mut open)
                .resizable(true)
                .default_width(600.0)
                .show(&ctx, |ui| {
                    ui.label(
                        egui::RichText::new(
                            "Scripts: send / sendline / waitfor <regex> [ms] / sleep",
                        )
                        .small()
                        .color(muted()),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for rb in &list {
                                let selected = self.rb_id.as_deref() == Some(rb.id.as_str());
                                if ui.selectable_label(selected, &rb.name).clicked() {
                                    load = Some(rb.clone());
                                }
                            }
                            if list.is_empty() {
                                ui.label(
                                    egui::RichText::new("No runbooks yet")
                                        .small()
                                        .color(muted()),
                                );
                            }
                        });
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.add(egui::TextEdit::singleline(&mut self.rb_name).desired_width(240.0));
                    });
                    ui.add(
                        egui::TextEdit::multiline(&mut self.rb_script)
                            .desired_rows(6)
                            .desired_width(f32::INFINITY)
                            .font(FontId::monospace(12.0)),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            save = true;
                        }
                        if ui.button("Run ▶").clicked() {
                            run = true;
                        }
                        if ui.button("Delete").clicked() {
                            if let Some(id) = &self.rb_id {
                                delete = Some(id.clone());
                            }
                        }
                    });
                });
            if let Some(rb) = load {
                self.rb_id = Some(rb.id.clone());
                self.rb_name = rb.name.clone();
                self.rb_script = rb.script.clone();
            }
            if save {
                let input = RunbookInput {
                    id: self.rb_id.clone(),
                    name: self.rb_name.clone(),
                    description: None,
                    script: self.rb_script.clone(),
                };
                if let Ok(rb) = self.db.upsert_runbook(&input) {
                    self.rb_id = Some(rb.id);
                    self.runbooks = self.db.list_runbooks().unwrap_or_default();
                }
            }
            if run {
                let script = self.rb_script.clone();
                self.start_runner(&script);
            }
            if let Some(id) = delete {
                let _ = self.db.delete_runbook(&id);
                self.runbooks = self.db.list_runbooks().unwrap_or_default();
                self.rb_id = None;
                self.rb_name.clear();
                self.rb_script.clear();
            }
            if !open {
                self.show_runbooks = false;
            }
        }

        // Subnet calculator window.
        if self.show_subnet {
            let mut open = true;
            egui::Window::new("Subnet calculator")
                .open(&mut open)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("IP");
                        ui.add(egui::TextEdit::singleline(&mut self.sub_ip).desired_width(160.0));
                        ui.label("/");
                        ui.add(egui::DragValue::new(&mut self.sub_prefix).range(0..=32));
                    });
                    if let Some(info) = subnet_calc(&self.sub_ip, self.sub_prefix) {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(info).monospace().color(text_c()));
                    } else {
                        ui.label(egui::RichText::new("invalid IP").color(muted()));
                    }
                });
            if !open {
                self.show_subnet = false;
            }
        }

        // Ping window.
        if self.show_ping {
            let mut open = true;
            egui::Window::new("Ping")
                .open(&mut open)
                .resizable(true)
                .default_width(520.0)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Host");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.ping_host).desired_width(200.0),
                        );
                        ui.label("count");
                        ui.add(egui::DragValue::new(&mut self.ping_count).range(1..=20));
                        if ui.button("Ping").clicked() {
                            self.run_ping();
                        }
                    });
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&self.ping_out)
                                    .monospace()
                                    .color(text_c()),
                            );
                        });
                });
            if !open {
                self.show_ping = false;
            }
        }

        // Traceroute window.
        if self.show_trace {
            let mut open = true;
            egui::Window::new("Traceroute")
                .open(&mut open)
                .resizable(true)
                .default_width(560.0)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Host");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.trace_host).desired_width(220.0),
                        );
                        if ui.button("Trace").clicked() {
                            self.run_trace();
                        }
                    });
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&self.trace_out)
                                    .monospace()
                                    .color(text_c()),
                            );
                        });
                });
            if !open {
                self.show_trace = false;
            }
        }

        // DNS lookup window.
        if self.show_dns {
            let mut open = true;
            egui::Window::new("DNS lookup")
                .open(&mut open)
                .resizable(true)
                .default_width(520.0)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Host");
                        ui.add(egui::TextEdit::singleline(&mut self.dns_host).desired_width(220.0));
                        if ui.button("Lookup").clicked() {
                            self.run_dns();
                        }
                    });
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&self.dns_out)
                                    .monospace()
                                    .color(text_c()),
                            );
                        });
                });
            if !open {
                self.show_dns = false;
            }
        }

        // Broadcast targets window.
        if self.show_targets {
            let mut open = true;
            egui::Window::new("Broadcast targets")
                .open(&mut open)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("All").clicked() {
                            for s in &mut self.sessions {
                                s.broadcast_selected = true;
                            }
                        }
                        if ui.small_button("None").clicked() {
                            for s in &mut self.sessions {
                                s.broadcast_selected = false;
                            }
                        }
                    });
                    for session in &mut self.sessions {
                        ui.add_enabled(
                            !session.exited,
                            egui::Checkbox::new(&mut session.broadcast_selected, &session.title),
                        );
                    }
                    if self.sessions.is_empty() {
                        ui.label(egui::RichText::new("No sessions").small().color(muted()));
                    }
                });
            if !open {
                self.show_targets = false;
            }
        }

        // Help / shortcuts window.
        if self.show_help {
            let mut open = true;
            egui::Window::new("Keyboard shortcuts")
                .open(&mut open)
                .resizable(false)
                .default_width(460.0)
                .show(&ctx, |ui| {
                    for (k, v) in [
                        ("Ctrl + =", "Zoom in"),
                        ("Ctrl + -", "Zoom out"),
                        ("Ctrl + 0", "Reset zoom"),
                        ("Ctrl + ,", "Open settings"),
                        ("Ctrl + C", "Copy selection (or ETX if none)"),
                        ("Ctrl + A", "Select all"),
                        ("Ctrl + V", "Paste"),
                        ("Ctrl + Shift + C", "Copy selection"),
                        ("Double-click", "Select + copy a word"),
                        ("Triple-click", "Select the whole line"),
                        ("Drag", "Select range (auto-scrolls at the edges)"),
                        ("Shift + Click", "Extend the selection"),
                        ("Right-click", "Copy / select all / clear"),
                        ("Tools → Credentials", "Credential vault"),
                        ("Mouse wheel", "Scroll terminal history"),
                        ("Shift + Page Up/Down", "Scroll history by a page"),
                        ("Enter (COMMAND)", "Send to session"),
                        ("Shift+Enter", "New line in COMMAND"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(k).monospace().color(text_c()));
                            ui.label(egui::RichText::new(v).color(muted()));
                        });
                    }
                });
            if !open {
                self.show_help = false;
            }
        }

        // Workspace window.
        if self.show_ws {
            let mut open = true;
            let list = self.workspaces.clone();
            let mut restore: Option<serde_json::Value> = None;
            let mut delete: Option<String> = None;
            let mut save = false;
            egui::Window::new("Workspaces")
                .open(&mut open)
                .resizable(true)
                .default_width(460.0)
                .show(&ctx, |ui| {
                    ui.label(
                        egui::RichText::new("Save the current open devices, restore later")
                            .small()
                            .color(muted()),
                    );
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.ws_name)
                                .hint_text("workspace name")
                                .desired_width(200.0),
                        );
                        if ui.button("Save current").clicked() {
                            save = true;
                        }
                    });
                    ui.separator();
                    for w in &list {
                        ui.horizontal(|ui| {
                            ui.label(&w.name);
                            if ui.small_button("Restore").clicked() {
                                restore = Some(w.layout.clone());
                            }
                            if ui.small_button("Delete").clicked() {
                                delete = Some(w.id.clone());
                            }
                        });
                    }
                    if list.is_empty() {
                        ui.label(
                            egui::RichText::new("No workspaces yet")
                                .small()
                                .color(muted()),
                        );
                    }
                    if !self.ws_msg.is_empty() {
                        ui.label(egui::RichText::new(&self.ws_msg).small().color(ok()));
                    }
                });
            if save {
                self.save_workspace();
            }
            if let Some(layout) = restore {
                self.restore_workspace(&layout);
            }
            if let Some(id) = delete {
                let _ = self.db.delete_workspace(&id);
                self.workspaces = self.db.list_workspaces().unwrap_or_default();
            }
            if !open {
                self.show_ws = false;
            }
        }

        // Highlight rules editor.
        if self.show_hl {
            let mut open = true;
            let mut remove: Option<usize> = None;
            let mut add = false;
            egui::Window::new("Highlight rules")
                .open(&mut open)
                .default_width(440.0)
                .show(&ctx, |ui| {
                    ui.label(
                        egui::RichText::new("keyword (case-insensitive) → colour")
                            .small()
                            .color(muted()),
                    );
                    for idx in 0..self.hl_rules.len() {
                        ui.horizontal(|ui| {
                            let (kw, col, en) = &mut self.hl_rules[idx];
                            ui.checkbox(en, "");
                            ui.add(egui::TextEdit::singleline(kw).desired_width(150.0));
                            let mut c = egui::Color32::from_rgb(col[0], col[1], col[2]);
                            if ui.color_edit_button_srgba(&mut c).changed() {
                                *col = [c.r(), c.g(), c.b()];
                            }
                            if ui.small_button("✕").clicked() {
                                remove = Some(idx);
                            }
                        });
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.hl_new)
                                .hint_text("new keyword")
                                .desired_width(150.0),
                        );
                        let mut c = egui::Color32::from_rgb(
                            self.hl_color[0],
                            self.hl_color[1],
                            self.hl_color[2],
                        );
                        if ui.color_edit_button_srgba(&mut c).changed() {
                            self.hl_color = [c.r(), c.g(), c.b()];
                        }
                        if ui.button("Add").clicked() {
                            add = true;
                        }
                    });
                });
            if add && !self.hl_new.trim().is_empty() {
                self.hl_rules
                    .push((self.hl_new.trim().to_string(), self.hl_color, true));
                self.hl_new.clear();
            }
            if let Some(i) = remove {
                self.hl_rules.remove(i);
            }
            if !open {
                self.show_hl = false;
            }
        }

        self.folder_windows(&ctx);
        self.update_window(&ctx);
        if !close_drawn_early {
            self.confirm_close(&ctx);
        }
        if !self.close_input_blocked && !self.close_confirmation.active() {
            self.close_confirmation.remember_background_focus(&ctx);
        }
    }

    fn folder_windows(&mut self, ctx: &egui::Context) {
        let parent_label = self
            .folder_editor
            .as_ref()
            .and_then(|editor| editor.parent_id.as_ref())
            .and_then(|id| self.folders.iter().find(|folder| &folder.id == id))
            .map(|folder| folder.name.clone());
        let mut save = false;
        let mut cancel = false;
        if let Some(editor) = self.folder_editor.as_mut() {
            let title = if editor.id.is_some() {
                "Rename folder"
            } else {
                "New folder"
            };
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .default_width(320.0)
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Parent: {}",
                            parent_label.as_deref().unwrap_or("top level")
                        ))
                        .small()
                        .color(muted()),
                    );
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut editor.name)
                            .hint_text("folder name")
                            .desired_width(f32::INFINITY),
                    );
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        save = true;
                    }
                    if let Some(error) = &editor.error {
                        ui.label(
                            egui::RichText::new(error)
                                .small()
                                .color(egui::Color32::from_rgb(224, 108, 117)),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!editor.name.trim().is_empty(), egui::Button::new("Save"))
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
        }
        if save {
            self.commit_folder();
        }
        if cancel {
            self.folder_editor = None;
        }

        if let Some(folder) = self.folder_delete.clone() {
            let mut open = true;
            let mut delete = false;
            let mut cancel_delete = false;
            egui::Window::new("Delete folder")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(360.0)
                .show(ctx, |ui| {
                    ui.label(format!("Delete “{}”?", folder.name));
                    ui.label(
                        egui::RichText::new(
                            "Subfolders are deleted too. Devices inside them are kept but moved to No folder.",
                        )
                        .small()
                        .color(muted()),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                egui::RichText::new("Delete")
                                    .color(egui::Color32::from_rgb(224, 108, 117)),
                            )
                            .clicked()
                        {
                            delete = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_delete = true;
                        }
                    });
                });
            if delete {
                self.delete_confirmed_folder();
            } else if cancel_delete || !open {
                self.folder_delete = None;
            }
        }

        self.settings_window(ctx);
        self.credentials_window(ctx);
        self.credential_delete_dialog(ctx);
        self.vault_prompt_dialog(ctx);
        self.folder_picker_window(ctx);
        self.draw_toast(ctx);
        if ctx.input(|i| i.is_scrolling()) {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn update_window(&mut self, ctx: &egui::Context) {
        if !self.updater.show {
            return;
        }
        let mut open = true;
        let mut save = false;
        let mut check = false;
        let mut install = false;
        let mut restart_now = false;
        let risky_sessions = self.risky_session_count();
        let updater = &mut self.updater;
        egui::Window::new("Software update")
            .open(&mut open)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(format!("OpsTerm v{}", update::CURRENT_VERSION));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Update manifest URL")
                        .small()
                        .color(muted()),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut updater.manifest_url)
                            .desired_width(330.0)
                            .hint_text("https://… or file:///path/manifest.json"),
                    );
                    if ui.button("Save").clicked() {
                        save = true;
                    }
                    if ui
                        .add_enabled(!updater.busy(), egui::Button::new("Check now"))
                        .clicked()
                    {
                        check = true;
                    }
                });
                if ui
                    .checkbox(&mut updater.auto_check, "Check automatically on startup")
                    .changed()
                {
                    save = true;
                }
                if !updater.configured() {
                    ui.label(
                        egui::RichText::new("Set a manifest URL to enable update checks.")
                            .small()
                            .color(muted()),
                    );
                }
                ui.separator();
                if updater.checking {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Checking for updates…");
                    });
                }
                if updater.installing {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Downloading and verifying…");
                    });
                }
                if !updater.message.is_empty() {
                    ui.label(egui::RichText::new(&updater.message).color(ok()));
                }
                if let Some(error) = &updater.error {
                    ui.label(
                        egui::RichText::new(error).color(egui::Color32::from_rgb(224, 108, 117)),
                    );
                }
                if let Some(available) = &updater.available {
                    ui.add_space(6.0);
                    let title = if available.rebuilt {
                        format!("New build of v{} is available", available.version)
                    } else {
                        format!("Version {} is available", available.version)
                    };
                    ui.label(egui::RichText::new(title).strong().color(text_c()));
                    ui.label(
                        egui::RichText::new(format!(
                            "Download size: {:.1} MiB",
                            available.asset.size as f64 / 1_048_576.0
                        ))
                        .small()
                        .color(muted()),
                    );
                    if !available.notes.trim().is_empty() {
                        egui::ScrollArea::vertical()
                            .max_height(140.0)
                            .show(ui, |ui| {
                                ui.label(&available.notes);
                            });
                    }
                    if updater.installed.is_some() {
                        if ui.button("Restart now").clicked() {
                            restart_now = true;
                        }
                    } else if ui
                        .add_enabled(!updater.busy(), egui::Button::new("Install update"))
                        .clicked()
                    {
                        install = true;
                    }
                }
                if let Some(path) = &updater.installed {
                    ui.label(
                        egui::RichText::new(format!("Installed at {}", path.display()))
                            .small()
                            .color(muted()),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Updates are verified with Ed25519 before installation.")
                        .small()
                        .color(muted()),
                );
            });
        if save {
            updater.save_config();
        }
        if check {
            updater.start_check(true);
        }
        if install {
            updater.start_install();
        }
        if restart_now {
            if let Some(path) = updater.installed.clone() {
                if risky_sessions > 0 {
                    self.close_confirmation
                        .request(close_confirmation::Target::Restart {
                            path,
                            sessions: risky_sessions,
                        });
                    close_confirmation::discard_background_keyboard(ctx);
                } else {
                    match update::restart(&path) {
                        Ok(()) => std::process::exit(0),
                        Err(error) => updater.error = Some(error),
                    }
                }
            }
        }
        if !open {
            updater.show = false;
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        // Driver limits (often 16K) needlessly widen the CPU and GPU font atlas.
        // This changes packing, not glyph size, antialiasing or font quality.
        input.max_texture_side = Some(input.max_texture_side.unwrap_or(2048).min(2048));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

fn chrono_like_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}

fn scratch_path() -> std::path::PathBuf {
    data_dir().join("scratchpad.json")
}

fn load_scratch() -> Vec<String> {
    std::fs::read_to_string(scratch_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_scratch(notes: &[String]) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string_pretty(notes) {
        let _ = std::fs::write(scratch_path(), text);
    }
}

fn data_dir() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/share")
        });
    base.join("org.opsterm.app")
}

fn main() -> eframe::Result<()> {
    #[cfg(target_os = "linux")]
    std::env::set_var("MALLOC_ARENA_MAX", "2");
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(code) = update::cli(&args) {
        std::process::exit(code);
    }
    let launch = match startup::parse(&args) {
        Ok(startup::Launch::Help) => {
            print!("{}", startup::HELP);
            return Ok(());
        }
        Ok(startup::Launch::Version) => {
            println!("OpsTerm v{}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Ok(launch) => launch,
        Err(error) => {
            eprintln!("OpsTerm: {error}");
            std::process::exit(2);
        }
    };
    let backend = display_backend::parse(std::env::var("OPSTERM_DISPLAY_BACKEND").ok().as_deref())
        .unwrap_or_else(|error| {
            eprintln!("OpsTerm: {error}");
            std::process::exit(2);
        });
    let window_title = match &launch {
        startup::Launch::Telnet { title, .. } => format!("{title} — OpsTerm"),
        _ => "OpsTerm".into(),
    };
    let db = Database::open(&data_dir().join("opsterm.db")).expect("open database");
    let mut app = App::new(db);
    match launch {
        startup::Launch::Telnet { spec, title } => app.open_telnet_console(spec, title),
        _ => app.new_local(),
    }
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([760.0, 500.0])
            .with_title(window_title),
        ..Default::default()
    };
    match backend {
        display_backend::Backend::Auto => {}
        display_backend::Backend::X11 => {
            options.event_loop_builder = Some(Box::new(|builder| {
                use winit::platform::x11::EventLoopBuilderExtX11;
                builder.with_x11();
            }));
        }
        display_backend::Backend::Wayland => {
            options.event_loop_builder = Some(Box::new(|builder| {
                use winit::platform::wayland::EventLoopBuilderExtWayland;
                builder.with_wayland();
            }));
        }
    }
    eframe::run_native(
        "OpsTerm",
        options,
        Box::new(|cc| {
            app.install_selected_fonts(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct TestProvider {
        reader: Mutex<Option<Box<dyn Read + Send>>>,
        writes: Arc<Mutex<Vec<u8>>>,
        alive: Arc<AtomicBool>,
    }

    impl ConnectionProvider for TestProvider {
        fn write(&self, bytes: &[u8]) -> opsterm_core::error::AppResult<()> {
            self.writes.lock().unwrap().extend_from_slice(bytes);
            Ok(())
        }
        fn resize(&self, _: TerminalDimensions) -> opsterm_core::error::AppResult<()> {
            Ok(())
        }
        fn kill(&self) -> opsterm_core::error::AppResult<()> {
            self.alive.store(false, Ordering::SeqCst);
            Ok(())
        }
        fn take_reader(&self) -> opsterm_core::error::AppResult<Box<dyn Read + Send>> {
            Ok(self.reader.lock().unwrap().take().unwrap())
        }
        fn is_alive(&self) -> bool {
            self.alive.load(Ordering::SeqCst)
        }
    }

    fn test_app() -> App {
        let mut app = App::new(Database::open_in_memory().unwrap());
        for s in &app.sessions {
            let _ = s.provider.kill();
        }
        app.sessions.clear();
        app.vault_prompt = false;
        app.active = 0;
        app
    }

    fn test_session(reader: impl Read + Send + 'static) -> (Session, Arc<Mutex<Vec<u8>>>) {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let p = TestProvider {
            reader: Mutex::new(Some(Box::new(reader))),
            writes: writes.clone(),
            alive: Arc::new(AtomicBool::new(true)),
        };
        (
            Session::new("test".into(), Box::new(p), 80, 24, None, Origin::Local),
            writes,
        )
    }

    #[test]
    fn gns3_startup_opens_only_named_telnet_tab_without_inventory_credentials() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut app = test_app();
        app.vault_prompt = true;
        app.open_telnet_console(
            TelnetSpec {
                host: "127.0.0.1".into(),
                port,
                username: None,
                term: Some("xterm-256color".into()),
                password: None,
            },
            "GNS3 R1".into(),
        );
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.sessions[0].title, "GNS3 R1");
        assert!(!app.vault_prompt);
        assert!(matches!(&app.sessions[0].origin, Origin::Telnet(spec) if spec.port == port));
        assert_eq!(app.db.list_sessions().unwrap().len(), 0);
    }

    #[test]
    fn broadcast_selection_follows_device_after_reorder() {
        let mut app = test_app();
        let (a, wa) = test_session(std::io::empty());
        let (b, wb) = test_session(std::io::empty());
        app.sessions.extend([a, b]);
        app.broadcast = true;
        app.sessions[0].broadcast_selected = true;
        app.sessions[1].broadcast_selected = false;
        app.move_tab(0, 1);
        app.command = "show version".into();
        app.send_command(true);
        assert_eq!(&*wa.lock().unwrap(), b"show version\r");
        assert!(wb.lock().unwrap().is_empty());
    }

    #[test]
    fn command_buttons_respect_broadcast_targets() {
        let mut app = test_app();
        let (a, wa) = test_session(std::io::empty());
        let (b, wb) = test_session(std::io::empty());
        app.sessions.extend([a, b]);
        app.broadcast = true;
        app.sessions[0].broadcast_selected = true;
        app.sessions[1].broadcast_selected = false;
        app.send_raw("show version");
        assert_eq!(&*wa.lock().unwrap(), b"show version\r");
        assert!(wb.lock().unwrap().is_empty());
    }

    #[test]
    fn drag_selection_starts_at_the_press_point_and_updates_live() {
        let mut app = test_app();
        let (session, _) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 500.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![]), |ui| app.show(ui));
        let start = Pos2::new(430.0, 250.0);
        let _ = ctx.run_ui(
            input(vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]),
            |ui| app.show(ui),
        );
        let anchor = app.sessions[0].sel_anchor.expect("anchor at press point");
        assert!(app.sessions[0].selecting, "drag selection starts on press");
        let end = Pos2::new(620.0, 250.0);
        let _ = ctx.run_ui(input(vec![egui::Event::PointerMoved(end)]), |ui| {
            app.show(ui)
        });
        assert_ne!(
            app.sessions[0].sel_head,
            Some(anchor),
            "selection must follow the pointer before the drag threshold"
        );
        let _ = ctx.run_ui(
            input(vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]),
            |ui| app.show(ui),
        );
        assert!(!app.sessions[0].selecting);
        assert!(app.sessions[0].selection().is_some());
    }

    #[test]
    fn word_selection_expands_to_the_whole_token() {
        let (mut session, _) = test_session(std::io::empty());
        session
            .processor
            .advance(&mut session.term, b"ip route 10.10.42.1/24\r\n");
        session.sel_mode = SelMode::Word;
        session.sel_anchor = Some(Point::new(Line(0), Column(12)));
        session.sel_head = session.sel_anchor;
        assert_eq!(session.selection_text(), "10.10.42.1/24");
    }

    #[test]
    fn reversed_drag_normalizes_the_selection() {
        let (mut session, _) = test_session(std::io::empty());
        session.sel_mode = SelMode::Char;
        session.sel_anchor = Some(Point::new(Line(1), Column(5)));
        session.sel_head = Some(Point::new(Line(0), Column(2)));
        let (start, end) = session.selection().expect("selection");
        assert_eq!((start.line.0, start.column.0), (0, 2));
        assert_eq!((end.line.0, end.column.0), (1, 5));
    }

    #[test]
    fn line_selection_covers_the_whole_row() {
        let (mut session, _) = test_session(std::io::empty());
        session
            .processor
            .advance(&mut session.term, b"hostname core-1\r\n");
        session.sel_mode = SelMode::Line;
        session.sel_anchor = Some(Point::new(Line(0), Column(3)));
        session.sel_head = session.sel_anchor;
        let (start, end) = session.selection().expect("selection");
        assert_eq!(start.column.0, 0);
        assert_eq!(end.column.0, session.cols - 1);
        assert!(session.selection_text().starts_with("hostname core-1"));
    }

    #[test]
    fn device_backspace_override_is_respected() {
        let (mut session, _) = test_session(std::io::empty());
        session.backspace = 0x08;
        assert_eq!(
            session.key_bytes(egui::Key::Backspace, egui::Modifiers::NONE),
            Some(vec![0x08])
        );
    }

    #[test]
    fn ctrl_c_copy_event_without_selection_interrupts_the_terminal() {
        let mut app = test_app();
        let (session, writes) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let input = |events, modifiers| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 500.0))),
            events,
            modifiers,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![], egui::Modifiers::NONE), |ui| app.show(ui));
        let pos = Pos2::new(450.0, 250.0);
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                input(
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    egui::Modifiers::NONE,
                ),
                |ui| app.show(ui),
            );
        }
        assert!(ctx.memory(|m| m.focused().is_some()));
        let _ = ctx.run_ui(
            input(vec![egui::Event::Copy], egui::Modifiers::CTRL),
            |ui| app.show(ui),
        );
        assert_eq!(&*writes.lock().unwrap(), &[0x03]);
        writes.lock().unwrap().clear();
        // Native backends can batch release of C and Ctrl into one frame.
        let _ = ctx.run_ui(
            input(
                vec![
                    egui::Event::Copy,
                    egui::Event::Key {
                        key: egui::Key::C,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::CTRL,
                    },
                ],
                egui::Modifiers::NONE,
            ),
            |ui| app.show(ui),
        );
        assert_eq!(
            &*writes.lock().unwrap(),
            &[0x03],
            "quick Ctrl+C lost its event modifiers"
        );
        writes.lock().unwrap().clear();
        let _ = ctx.run_ui(
            input(
                vec![
                    egui::Event::Copy,
                    egui::Event::Key {
                        key: egui::Key::ControlLeft,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::CTRL,
                    },
                ],
                egui::Modifiers::NONE,
            ),
            |ui| app.show(ui),
        );
        assert_eq!(
            &*writes.lock().unwrap(),
            &[0x03],
            "native Copy plus Ctrl release lost its modifiers"
        );
        writes.lock().unwrap().clear();
        app.sessions[0].sel_anchor = Some(Point::new(Line(0), Column(0)));
        app.sessions[0].sel_head = Some(Point::new(Line(0), Column(1)));
        let _ = ctx.run_ui(
            input(vec![egui::Event::Copy], egui::Modifiers::CTRL),
            |ui| app.show(ui),
        );
        assert!(
            writes.lock().unwrap().is_empty(),
            "copying selection must not interrupt"
        );
        app.sessions[0].sel_anchor = None;
        app.sessions[0].sel_head = None;
        let modifiers = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![egui::Event::Copy], modifiers), |ui| app.show(ui));
        assert!(
            writes.lock().unwrap().is_empty(),
            "Ctrl+Shift+C must only copy"
        );
        let _ = ctx.run_ui(
            input(
                vec![
                    egui::Event::Copy,
                    egui::Event::Key {
                        key: egui::Key::C,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers,
                    },
                ],
                egui::Modifiers::NONE,
            ),
            |ui| app.show(ui),
        );
        assert!(
            writes.lock().unwrap().is_empty(),
            "quick Ctrl+Shift+C must not interrupt"
        );
    }

    #[test]
    fn tab_and_arrows_stay_with_the_focused_terminal() {
        let mut app = test_app();
        let (session, writes) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 500.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![]), |ui| app.show(ui));
        let pos = Pos2::new(450.0, 250.0);
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                input(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]),
                |ui| app.show(ui),
            );
        }
        let _ = ctx.run_ui(input(vec![]), |ui| app.show(ui));
        let focused = ctx.memory(|memory| memory.focused());
        assert!(focused.is_some(), "terminal should have keyboard focus");
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = ctx.run_ui(
            input(vec![key(egui::Key::Tab), key(egui::Key::ArrowDown)]),
            |ui| app.show(ui),
        );
        assert_eq!(ctx.memory(|memory| memory.focused()), focused);
        assert_eq!(&*writes.lock().unwrap(), b"\t\x1b[B");
    }

    #[test]
    fn unsent_command_is_preserved_without_a_target() {
        let mut app = test_app();
        app.command = "show version".into();
        app.send_command(true);
        assert_eq!(app.command, "show version");
    }

    #[test]
    fn closing_an_earlier_tab_keeps_active_device_and_broadcast_selection() {
        let mut app = test_app();
        let (a, _) = test_session(std::io::empty());
        let (b, wb) = test_session(std::io::empty());
        let (c, wc) = test_session(std::io::empty());
        app.sessions.extend([a, b, c]);
        app.active = 1;
        let active_id = app.sessions[1].id;
        app.sessions[0].broadcast_selected = false;
        app.sessions[2].broadcast_selected = false;
        app.close_tab(0);
        assert_eq!(app.sessions[app.active].id, active_id);
        app.broadcast = true;
        app.send_raw("show version");
        assert_eq!(&*wb.lock().unwrap(), b"show version\r");
        assert!(wc.lock().unwrap().is_empty());
    }

    #[test]
    fn moving_a_background_tab_keeps_active_device() {
        let mut app = test_app();
        for _ in 0..3 {
            app.sessions.push(test_session(std::io::empty()).0);
        }
        app.active = 1;
        let active_id = app.sessions[1].id;
        app.move_tab(0, 2);
        assert_eq!(app.sessions[app.active].id, active_id);
        app.move_tab(2, 0);
        assert_eq!(app.sessions[app.active].id, active_id);
    }

    fn hold_connection_workers(app: &mut App) -> Vec<Sender<()>> {
        let mut releases = Vec::new();
        for _ in 0..connections::MAX_WORKERS {
            let (release, wait) = channel();
            releases.push(release);
            app.pending_connections.push(connections::Pending {
                request: connections::Request {
                    title: "busy worker".into(),
                    origin: Origin::Local,
                    saved: None,
                    secret: None,
                    target: None,
                    dims: TerminalDimensions { cols: 80, rows: 24 },
                },
                task: background::Task::spawn(move || {
                    let _ = wait.recv();
                    Err("controlled test completion".into())
                })
                .unwrap(),
            });
        }
        releases
    }

    #[test]
    fn device_click_opens_its_tab_before_a_worker_is_available() {
        let mut app = test_app();
        let _release = hold_connection_workers(&mut app);
        let saved = saved_ssh(&app.db, "immediate", None);
        let start = Instant::now();
        app.open_saved(&saved);
        assert!(start.elapsed() < Duration::from_millis(50));
        assert_eq!(
            app.sessions.len(),
            1,
            "clicked device must get its tab immediately"
        );
        assert_eq!(
            app.sessions[app.active].saved_id.as_deref(),
            Some(saved.id.as_str())
        );
        assert_eq!(app.queued_connections.len(), 1);
    }

    #[test]
    fn repeated_device_click_focuses_the_existing_tab_without_another_connection() {
        let mut app = test_app();
        let _release = hold_connection_workers(&mut app);
        let a = saved_ssh(&app.db, "A", None);
        let b = saved_ssh(&app.db, "B", None);
        app.open_saved(&a);
        app.open_saved(&b);
        app.open_saved(&a);
        assert_eq!(app.sessions.len(), 2);
        assert_eq!(
            app.sessions[app.active].saved_id.as_deref(),
            Some(a.id.as_str())
        );
        assert_eq!(app.queued_connections.len(), 2);
    }

    #[test]
    fn selecting_a_device_allows_typing_without_an_extra_terminal_click() {
        let mut app = test_app();
        app.fonts_ready = true;
        let saved = saved_ssh(&app.db, "Keyboard device", None);
        app.refresh_devices();
        app.sessions.push(test_session(std::io::empty()).0);
        let (mut device, writes) = test_session(std::io::empty());
        device.saved_id = Some(saved.id);
        app.sessions.push(device);
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1180.0, 760.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![]), |ui| app.show(ui));
        let pos = Pos2::new(45.0, 208.0);
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                input(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]),
                |ui| app.show(ui),
            );
        }
        assert_eq!(app.active, 1);
        let _ = ctx.run_ui(input(vec![egui::Event::Text("login-test".into())]), |ui| {
            app.show(ui)
        });
        assert_eq!(&*writes.lock().unwrap(), b"login-test");
    }

    #[test]
    fn missing_saved_password_is_reported_before_a_telnet_socket_is_opened() {
        let mut app = test_app();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut saved = saved_ssh(&app.db, "Missing credential", None);
        saved.protocol = "telnet".into();
        saved.port = listener.local_addr().unwrap().port();
        saved.username = Some("test-user".into());
        saved.credential_ref = Some(format!("absent-{}-{}", std::process::id(), saved.id));
        app.open_saved(&saved);
        poll_until_connections_finish(&mut app);
        assert!(
            listener.accept().is_err(),
            "must not open a passwordless connection after keyring failure"
        );
        assert!(app.sessions[0].connection_error.is_some());
        assert!(app.sessions[0].exited);
    }

    #[test]
    fn local_sessions_do_not_require_an_unrelated_saved_password() {
        let mut app = test_app();
        let mut saved = saved_ssh(&app.db, "Local fixture", None);
        saved.protocol = "local".into();
        saved.credential_ref = Some(format!("absent-{}-{}", std::process::id(), saved.id));
        app.open_saved(&saved);
        poll_until_connections_finish(&mut app);
        assert!(app.sessions[0].connection_error.is_none());
        assert!(!app.sessions[0].opening);
    }

    #[test]
    fn completing_an_open_does_not_steal_focus_or_change_its_tab_id() {
        let mut app = test_app();
        let _release = hold_connection_workers(&mut app);
        let a = saved_ssh(&app.db, "A", None);
        let b = saved_ssh(&app.db, "B", None);
        app.open_saved(&a);
        let target = app.sessions[0].id;
        app.open_saved(&b);
        let active = app.sessions[app.active].id;
        let request = app.queued_connections.pop_front().unwrap().request;
        app.pending_connections.push(connections::Pending {
            request,
            task: background::Task::spawn(|| {
                Ok(connections::OpenedProvider(
                    Some(Box::new(TestProvider {
                        reader: Mutex::new(Some(Box::new(std::io::empty()))),
                        writes: Arc::new(Mutex::new(Vec::new())),
                        alive: Arc::new(AtomicBool::new(true)),
                    })),
                    None,
                ))
            })
            .unwrap(),
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.sessions[0].opening {
            app.poll_connections();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(app.sessions[0].id, target);
        assert_eq!(app.sessions[app.active].id, active);
        assert_eq!(app.sessions.len(), 2);
    }

    #[test]
    fn idle_workspace_does_not_schedule_sixty_frames_per_second() {
        let mut app = test_app();
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let mut delay = Duration::ZERO;
        for frame in 0..8 {
            let output = ctx.run_ui(
                egui::RawInput {
                    time: Some(frame as f64 * 2.0),
                    ..Default::default()
                },
                |ui| app.show(ui),
            );
            delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        }
        assert!(
            delay >= Duration::from_millis(250),
            "idle repaint interval: {delay:?}"
        );
    }

    #[test]
    fn terminal_paints_selected_palette_and_dims_highlights_once() {
        let (mut session, _) = test_session(std::io::empty());
        session
            .processor
            .advance(&mut session.term, b"\x1b[34mDIRECTORY\x1b[0m ALERT");
        let style = appearance::Appearance {
            palette: appearance::Palette::Soft,
            brightness: 80.0,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            terminal_render::paint(
                ui,
                &session,
                terminal_render::Paint {
                    origin: Pos2::ZERO,
                    colors: style.colors(bg(), text_c()),
                    font: FontId::monospace(14.0),
                    cell: Vec2::new(10.0, 20.0),
                    selection: None,
                    hover: None,
                    rules: &[("ALERT".into(), [200, 150, 100], true)],
                    search: "",
                },
            );
        });
        let color = |sample: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == sample => {
                        Some(text.galley.job.sections[0].format.color)
                    }
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(color("DIRECTORY"), Color32::from_rgb(90, 116, 144));
        assert_eq!(color("ALERT"), Color32::from_rgb(160, 120, 80));
    }

    #[test]
    fn appearance_choices_survive_settings_save_and_reload() {
        let mut app = test_app();
        let value = serde_json::json!({
            "terminal_appearance": {"font": "builtin", "palette": "soft", "brightness": 80.0}
        });
        app.db.save_named_setting("ui.egui", &value).unwrap();
        app.load_ui_settings();
        app.save_ui_settings();
        let saved = app
            .db
            .load_named_setting::<serde_json::Value>("ui.egui")
            .unwrap()
            .unwrap();
        assert_eq!(saved["terminal_appearance"]["palette"], "soft");
        assert_eq!(saved["terminal_appearance"]["font"], "builtin");
        assert_eq!(saved["terminal_appearance"]["brightness"], 80.0);
    }

    #[test]
    fn legacy_settings_gain_original_terminal_appearance_defaults() {
        let settings: UiSettings =
            serde_json::from_value(serde_json::json!({"font_size": 16.0})).unwrap();
        let value = serde_json::to_value(settings).unwrap();
        assert_eq!(value["terminal_appearance"]["palette"], "original");
        assert_eq!(value["terminal_appearance"]["font"], "system");
        assert_eq!(value["terminal_appearance"]["brightness"], 100.0);
    }

    #[test]
    fn select_all_copies_the_partially_visible_history_row() {
        let mut app = test_app();
        app.fonts_ready = true;
        app.sessions.push(test_session(std::io::empty()).0);
        app.terminal_focus = Some(app.sessions[0].id);
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 500.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![]), |ui| app.show(ui));
        let session = &mut app.sessions[0];
        let lines: String = (0..200).map(|i| format!("HISTORY_{i:03}\r\n")).collect();
        session
            .processor
            .advance(&mut session.term, lines.as_bytes());
        session
            .term
            .scroll_display(alacritty_terminal::grid::Scroll::Delta(1));
        session.scroll_pixels = 7.0;
        let preceding: String = session.row_text(-2).iter().collect();
        let _ = ctx.run_ui(
            input(vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            }]),
            |ui| app.show(ui),
        );
        assert_eq!(app.sessions[0].selection().unwrap().0.line.0, -2);
        assert!(app.sessions[0].selection_text().contains(preceding.trim()));
    }

    #[test]
    fn mouse_wheel_is_reported_to_apps_that_enable_mouse_tracking() {
        let mut app = test_app();
        let (mut session, writes) = test_session(std::io::empty());
        // Same modes opencode/opentui enables at startup.
        session.processor.advance(
            &mut session.term,
            b"\x1b[?1049h\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h",
        );
        app.sessions.push(session);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 500.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![]), |ui| app.show(ui));
        let _ = ctx.run_ui(input(vec![]), |ui| app.show(ui));
        let pos = Pos2::new(450.0, 250.0);
        let _ = ctx.run_ui(
            input(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: Vec2::new(0.0, 1.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]),
            |ui| app.show(ui),
        );
        let written = writes.lock().unwrap().clone();
        assert!(
            written.windows(5).any(|w| w == b"\x1b[<64"),
            "expected an SGR wheel-up report, got {:?}",
            String::from_utf8_lossy(&written)
        );
    }

    #[test]
    fn plain_ctrl_a_and_ctrl_v_stay_with_the_terminal() {
        let mut app = test_app();
        let (session, writes) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let input = |events, modifiers| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 500.0))),
            events,
            modifiers,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![], egui::Modifiers::NONE), |ui| app.show(ui));
        let pos = Pos2::new(450.0, 250.0);
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                input(
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    egui::Modifiers::NONE,
                ),
                |ui| app.show(ui),
            );
        }
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        let _ = ctx.run_ui(
            input(vec![key(egui::Key::A)], egui::Modifiers::CTRL),
            |ui| app.show(ui),
        );
        assert_eq!(&*writes.lock().unwrap(), &[0x01]);
        writes.lock().unwrap().clear();
        let _ = ctx.run_ui(
            input(
                vec![egui::Event::Paste("echo hi".into())],
                egui::Modifiers::CTRL,
            ),
            |ui| app.show(ui),
        );
        assert_eq!(&*writes.lock().unwrap(), &[0x16]);
        writes.lock().unwrap().clear();
        let _ = ctx.run_ui(
            input(
                vec![egui::Event::Paste("echo hi".into())],
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            ),
            |ui| app.show(ui),
        );
        assert_eq!(&*writes.lock().unwrap(), b"echo hi");
    }

    #[test]
    fn fractional_scroll_clamps_at_history_edges_without_reverse_dead_zone() {
        let (mut session, _) = test_session(std::io::empty());
        session
            .processor
            .advance(&mut session.term, "line\r\n".repeat(200).as_bytes());
        session.scroll_by_pixels(7.0, 20.0);
        assert_eq!(session.term.grid().display_offset(), 0);
        assert!((session.scroll_pixels - 7.0).abs() < 0.001);
        session.scroll_by_pixels(14.0, 20.0);
        assert_eq!(session.term.grid().display_offset(), 1);
        assert!((session.scroll_pixels - 1.0).abs() < 0.001);
        session.scroll_by_pixels(-7.0, 20.0);
        assert_eq!(session.term.grid().display_offset(), 0);
        assert!((session.scroll_pixels - 14.0).abs() < 0.001);
        session.scroll_by_pixels(1e6, 20.0);
        assert_eq!(
            session.term.grid().display_offset(),
            session.term.history_size()
        );
        assert_eq!(session.scroll_pixels, 0.0);
        session.scroll_by_pixels(-7.0, 20.0);
        assert_eq!(
            session.term.grid().display_offset(),
            session.term.history_size() - 1
        );
        assert!((session.scroll_pixels - 13.0).abs() < 0.001);
        session.scroll_by_pixels(-1e6, 20.0);
        assert_eq!(session.term.grid().display_offset(), 0);
        assert_eq!(session.scroll_pixels, 0.0);
    }

    #[test]
    fn regression_borderless_buttons_have_visible_hover_feedback() {
        let ctx = egui::Context::default();
        apply_style(&ctx);
        let visuals = ctx.style_of(egui::Theme::Dark).visuals.clone();
        assert_ne!(
            visuals.widgets.inactive.fg_stroke,
            visuals.widgets.hovered.fg_stroke
        );
    }

    #[test]
    fn regression_font_atlas_does_not_use_driver_maximum_width() {
        let mut app = test_app();
        let mut input = egui::RawInput {
            max_texture_side: Some(16384),
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &egui::Context::default(), &mut input);
        assert!(input.max_texture_side.unwrap() <= 2048);
        input.max_texture_side = Some(1024);
        eframe::App::raw_input_hook(&mut app, &egui::Context::default(), &mut input);
        assert_eq!(input.max_texture_side, Some(1024));
    }

    #[test]
    fn regression_terminal_paints_fractional_scroll_between_rows() {
        let (mut session, _) = test_session(std::io::empty());
        session
            .processor
            .advance(&mut session.term, b"FRACTIONAL_SCROLL");
        session.scroll_pixels = 7.0;
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            terminal_render::paint(
                ui,
                &session,
                terminal_render::Paint {
                    colors: appearance::Appearance::default().colors(bg(), text_c()),
                    origin: Pos2::new(30.0, 40.0),
                    font: FontId::monospace(14.0),
                    cell: Vec2::new(10.0, 20.0),
                    selection: None,
                    hover: None,
                    rules: &[],
                    search: "",
                },
            );
        });
        let y = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().contains("FRACTIONAL_SCROLL") => {
                    Some(text.pos.y)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(y, 47.0);
    }

    #[test]
    fn history_is_bounded_after_large_output() {
        let (mut session, _) = test_session(std::io::empty());
        session.processor.advance(
            &mut session.term,
            "output line\r\n".repeat(12_000).as_bytes(),
        );
        assert_eq!(session.term.history_size(), SCROLLBACK_LINES);
    }

    #[test]
    fn finite_output_burst_schedules_its_remaining_chunks_without_idle_delay() {
        let (mut session, _) = test_session(std::io::empty());
        let ctx = egui::Context::default();
        session.wake.bind(&ctx);
        for frame in 0..8 {
            let _ = ctx.run_ui(
                egui::RawInput {
                    time: Some(frame as f64 * 2.0),
                    ..Default::default()
                },
                |ui| ui.ctx().request_repaint_after(Duration::from_secs(1)),
            );
        }
        let (tx, rx) = sync_channel(OUTPUT_QUEUE_CHUNKS);
        session.rx = rx;
        for _ in 0..OUTPUT_QUEUE_CHUNKS {
            tx.send(b"line\r\n".to_vec()).unwrap();
        }
        drop(tx);
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(16.0),
                ..Default::default()
            },
            |ui| {
                ui.ctx().request_repaint_after(Duration::from_secs(1));
                session.pump();
            },
        );
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
                <= Duration::from_millis(16)
        );
    }

    #[test]
    fn terminal_text_is_batched_instead_of_one_shape_per_character() {
        let mut app = test_app();
        app.fonts_ready = true;
        app.sessions.push(test_session(std::io::empty()).0);
        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1180.0, 760.0))),
            ..Default::default()
        };
        let _ = ctx.run_ui(input(), |ui| app.show(ui));
        let session = &mut app.sessions[0];
        let line = format!("{}\r\n", "x".repeat(session.cols - 1));
        session
            .processor
            .advance(&mut session.term, line.repeat(50).as_bytes());
        let output = ctx.run_ui(input(), |ui| app.show(ui));
        let shapes = output
            .shapes
            .iter()
            .filter(|s| matches!(s.shape, egui::Shape::Text(_)))
            .count();
        assert!(
            shapes < 150,
            "too many glyph shapes for uniform terminal rows: {shapes}"
        );
    }

    #[test]
    fn batched_terminal_preserves_ansi_colors_and_wide_character_columns() {
        let (mut session, _) = test_session(std::io::empty());
        session
            .processor
            .advance(&mut session.term, "\x1b[31mRED\x1b[0m 界Z".as_bytes());
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            terminal_render::paint(
                ui,
                &session,
                terminal_render::Paint {
                    colors: appearance::Appearance::default().colors(bg(), text_c()),
                    origin: Pos2::ZERO,
                    font: FontId::monospace(14.0),
                    cell: Vec2::new(10.0, 20.0),
                    selection: None,
                    hover: None,
                    rules: &[],
                    search: "",
                },
            );
        });
        let text = |value: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == value => Some(text),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(
            text("RED").galley.job.sections[0].format.color,
            Color32::from_rgb(255, 107, 122)
        );
        assert_eq!(text("界").pos.x, 40.0);
        assert_eq!(text("Z").pos.x, 60.0);
    }

    #[test]
    fn cached_device_library_refreshes_after_edits() {
        let mut app = test_app();
        let saved = saved_ssh(&app.db, "Router", None);
        assert!(app.devices.is_empty());
        app.refresh_devices();
        assert_eq!(app.devices.len(), 1);
        let mut editor = EditorState::from_saved(&saved);
        editor.name = "Renamed".into();
        assert!(app.save_editor(&editor));
        assert_eq!(app.devices[0].name, "Renamed");
        app.dev_filter = "nothing matches".into();
        app.refilter_devices();
        assert!(app.device_indices.is_empty());
    }

    #[test]
    fn folder_choices_nest_and_devices_move_between_folders() {
        let mut app = test_app();
        let root = app
            .db
            .upsert_folder(&FolderInput {
                id: None,
                parent_id: None,
                name: "SWITCH".into(),
                sort_order: None,
            })
            .unwrap();
        let role = app
            .db
            .upsert_folder(&FolderInput {
                id: None,
                parent_id: Some(root.id.clone()),
                name: "CORE".into(),
                sort_order: None,
            })
            .unwrap();
        app.refresh_folders();
        let choices = app.folder_choices();
        assert_eq!(choices[0], ("No folder".to_string(), None));
        assert_eq!(choices[1].0, "SWITCH");
        assert_eq!(choices[2].0, "  CORE");
        assert_eq!(choices[2].1, Some(role.id.clone()));

        saved_ssh(&app.db, "Router", None);
        app.refresh_devices();
        app.set_device_folder(0, Some(role.id.clone()));
        assert_eq!(app.devices[0].folder_id.as_deref(), Some(role.id.as_str()));
        app.set_device_folder(0, None);
        assert_eq!(app.devices[0].folder_id, None);
    }

    #[test]
    fn device_row_name_and_host_both_activate_the_device() {
        let mut app = test_app();
        saved_ssh(&app.db, "Router", None);
        app.refresh_devices();
        for y in [12.0, 32.0] {
            let ctx = egui::Context::default();
            let input = |events| egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, 200.0))),
                events,
                ..Default::default()
            };
            let output = ctx.run_ui(input(vec![]), |ui| {
                device_list::show(ui, &app.devices, &app.device_indices, None, &[]);
            });
            assert!(
                output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == "Router" && text.pos.x < 25.0
                )),
                "device names must align with the left row inset"
            );
            let mut selected = None;
            let pos = Pos2::new(40.0, y);
            for pressed in [true, false] {
                let _ = ctx.run_ui(
                    input(vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]),
                    |ui| {
                        if let Some(device_list::Action::Activate(index)) =
                            device_list::show(ui, &app.devices, &app.device_indices, None, &[])
                        {
                            selected = Some(index);
                        }
                    },
                );
            }
            assert_eq!(selected, Some(0), "row click at y={y} was lost");
        }
    }

    #[test]
    fn clicking_a_tab_changes_the_active_session() {
        let mut app = test_app();
        app.sessions.push(test_session(std::io::empty()).0);
        app.sessions.push(test_session(std::io::empty()).0);
        app.active = 1;
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 500.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input(vec![]), |ui| app.show(ui));
        let pos = Pos2::new(35.0, 63.0);
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                input(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]),
                |ui| app.show(ui),
            );
        }
        assert_eq!(app.active, 0);
        let moved_id = app.sessions[0].id;
        for events in [
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerMoved(Pos2::new(190.0, 63.0))],
            vec![egui::Event::PointerButton {
                pos: Pos2::new(190.0, 63.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            let _ = ctx.run_ui(input(events), |ui| app.show(ui));
        }
        assert_eq!(
            app.sessions[1].id, moved_id,
            "drag must still reorder the tab"
        );
        assert_eq!(app.sessions[app.active].id, moved_id);
    }

    #[test]
    fn invalid_editor_save_reports_error_without_inserting_device() {
        let mut app = test_app();
        assert!(!app.save_editor(&EditorState::new()));
        assert!(app.flash.contains("Could not save"));
        assert!(app.db.list_sessions().unwrap().is_empty());
    }

    #[test]
    fn workspace_renders_at_minimum_size_with_many_tabs() {
        let mut app = test_app();
        for _ in 0..12 {
            app.sessions.push(test_session(std::io::empty()).0);
        }
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 500.0))),
                    ..Default::default()
                },
                |ui| {
                    app.show(ui);
                    assert!(
                        ui.min_rect().right() <= 761.0,
                        "workspace overflow: {:?}",
                        ui.min_rect()
                    );
                },
            );
        }
    }

    #[test]
    fn scrolled_terminal_paints_history_in_the_viewport() {
        let mut app = test_app();
        app.sessions.push(test_session(std::io::empty()).0);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760.0, 500.0))),
            ..Default::default()
        };
        let _ = ctx.run_ui(input(), |ui| app.show(ui));
        let output = format!("SCROLLBACK_MARKER\r\n{}", "later line\r\n".repeat(50));
        let session = &mut app.sessions[0];
        session
            .processor
            .advance(&mut session.term, output.as_bytes());
        session
            .term
            .scroll_display(alacritty_terminal::grid::Scroll::Top);
        let painted = ctx.run_ui(input(), |ui| app.show(ui));
        let text: String = painted
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            text.contains("SCROLLBACK_MARKER"),
            "history was not painted: {text}"
        );
    }

    #[test]
    fn command_line_endings_are_sent_as_single_carriage_returns() {
        let mut app = test_app();
        let (s, writes) = test_session(std::io::empty());
        app.sessions.push(s);
        app.command = "first\r\nsecond\nthird\r".into();
        app.send_command(true);
        assert_eq!(&*writes.lock().unwrap(), b"first\rsecond\rthird\r");
    }

    #[test]
    fn editing_device_preserves_advanced_settings() {
        let mut app = test_app();
        let mut input: SavedSessionInput = serde_json::from_value(serde_json::json!({
            "name": "Router", "host": "127.0.0.1", "protocol": "ssh", "port": 22,
            "identityFile": "/home/example/.ssh/id_ed25519", "proxyJump": "bastion",
            "tags": ["production"], "notes": "keep me", "isFavorite": true,
            "settingsOverride": {"engine": "native", "encoding": "gbk", "backspace": "del", "pager": "custom", "pagerCommand": "screen 0"}
        })).unwrap();
        input.sort_order = Some(7);
        let saved = app.db.upsert_session(&input).unwrap();
        let mut editor = EditorState::from_saved(&saved);
        editor.name = "Renamed".into();
        app.save_editor(&editor);
        let actual = app.db.get_session(&saved.id).unwrap();
        assert_eq!(actual.name, "Renamed");
        assert_eq!(actual.identity_file, saved.identity_file);
        assert_eq!(actual.proxy_jump, saved.proxy_jump);
        assert_eq!(actual.tags, saved.tags);
        assert_eq!(actual.notes, saved.notes);
        assert!(actual.is_favorite);
        assert_eq!(actual.sort_order, 7);
        assert_eq!(actual.settings_override, saved.settings_override);
    }

    #[test]
    fn runner_keeps_original_target_after_tab_switch() {
        let mut app = test_app();
        let (a, wa) = test_session(std::io::empty());
        let (b, wb) = test_session(std::io::empty());
        app.sessions.extend([a, b]);
        app.start_runner("sleep 0\nsendline show version");
        app.active = 1;
        app.tick_runner();
        assert_eq!(&*wa.lock().unwrap(), b"show version\r");
        assert!(wb.lock().unwrap().is_empty());
    }

    #[test]
    fn runner_timeout_does_not_send_next_command() {
        let mut app = test_app();
        let (s, writes) = test_session(std::io::empty());
        app.sessions.push(s);
        app.start_runner("waitfor NEVER 0\nsendline configure terminal");
        app.tick_runner();
        assert!(writes.lock().unwrap().is_empty());
        assert!(app.runners.is_empty());
        assert!(app.flash.contains("timeout"));
    }

    #[test]
    fn invalid_wait_pattern_rejects_entire_script_before_sending() {
        let mut app = test_app();
        let (s, writes) = test_session(std::io::empty());
        app.sessions.push(s);
        app.start_runner("sendline dangerous\nwaitfor [ 0");
        app.tick_runner();
        assert!(writes.lock().unwrap().is_empty());
        assert!(app.runners.is_empty());
        assert!(app.flash.contains("regex"));
    }

    #[test]
    fn runner_keeps_target_when_tabs_are_reordered() {
        let mut app = test_app();
        let (a, wa) = test_session(std::io::empty());
        let (b, wb) = test_session(std::io::empty());
        app.sessions.extend([a, b]);
        app.start_runner("sendline show version");
        app.move_tab(0, 1);
        app.active = 0;
        app.tick_runner();
        assert_eq!(&*wa.lock().unwrap(), b"show version\r");
        assert!(wb.lock().unwrap().is_empty());
    }

    #[test]
    fn runner_stops_when_its_target_is_closed() {
        let mut app = test_app();
        let (a, _) = test_session(std::io::empty());
        let (b, wb) = test_session(std::io::empty());
        app.sessions.extend([a, b]);
        app.start_runner("sendline configure terminal");
        app.sessions.remove(0);
        app.tick_runner();
        assert!(wb.lock().unwrap().is_empty());
        assert!(app.runners.is_empty());
        assert!(app.flash.contains("closed"));
    }

    #[test]
    fn hidden_sessions_receive_output_and_detect_exit_without_rendering() {
        let mut app = test_app();
        let (hidden, _) = test_session(std::io::Cursor::new(b"hidden output\r\n"));
        let (visible, _) = test_session(std::io::empty());
        app.sessions.extend([hidden, visible]);
        app.active = 1;
        let deadline = Instant::now() + Duration::from_secs(2);
        while !app.sessions[0].term_text().contains("hidden output") {
            app.pump_sessions();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        app.sessions[0].provider.kill().unwrap();
        app.pump_sessions();
        assert!(app.sessions[0].exited);
    }

    fn saved_ssh(db: &Database, name: &str, settings: Option<serde_json::Value>) -> SavedSession {
        db.upsert_session(&SavedSessionInput {
            id: None,
            folder_id: None,
            name: name.into(),
            host: "127.0.0.1".into(),
            port: 9,
            protocol: "ssh".into(),
            username: None,
            ssh_alias: None,
            identity_file: None,
            proxy_jump: None,
            tags: vec![],
            description: None,
            notes: None,
            settings_override: settings,
            credential_ref: None,
            is_favorite: false,
            sort_order: None,
        })
        .unwrap()
    }

    fn poll_until_connections_finish(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !app.pending_connections.is_empty() || !app.queued_connections.is_empty() {
            app.poll_connections();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    #[test]
    fn failed_connection_cannot_run_login_actions_on_existing_session() {
        let mut app = test_app();
        let saved = saved_ssh(
            &app.db,
            "failed",
            Some(serde_json::json!({
                "loginScript": "sendline dangerous"
            })),
        );
        let (existing, writes) = test_session(std::io::empty());
        app.sessions.push(existing);
        app.pending_connections.push(connections::Pending {
            request: connections::Request {
                title: "failed".into(),
                origin: Origin::Local,
                saved: Some(saved),
                secret: None,
                target: None,
                dims: TerminalDimensions { cols: 80, rows: 24 },
            },
            task: background::Task::spawn(|| Err("network denied".into())).unwrap(),
        });
        poll_until_connections_finish(&mut app);
        app.tick_runner();
        assert!(writes.lock().unwrap().is_empty());
        assert_eq!(app.sessions.len(), 1);
        assert!(app.flash.contains("network denied"));
    }

    #[test]
    fn abandoned_reconnect_cleans_up_new_provider() {
        let mut app = test_app();
        let (old, _) = test_session(std::io::empty());
        let target = old.id;
        app.sessions.push(old);
        let alive = Arc::new(AtomicBool::new(true));
        let provider = TestProvider {
            reader: Mutex::new(Some(Box::new(std::io::empty()))),
            writes: Arc::new(Mutex::new(Vec::new())),
            alive: alive.clone(),
        };
        app.pending_connections.push(connections::Pending {
            request: connections::Request {
                title: "reconnect".into(),
                origin: Origin::Local,
                saved: None,
                secret: None,
                target: Some(target),
                dims: TerminalDimensions { cols: 80, rows: 24 },
            },
            task: background::Task::spawn(move || {
                Ok(connections::OpenedProvider(Some(Box::new(provider)), None))
            })
            .unwrap(),
        });
        app.sessions.clear();
        poll_until_connections_finish(&mut app);
        assert!(app.sessions.is_empty());
        assert!(!alive.load(Ordering::SeqCst));
    }

    #[test]
    fn stale_sftp_result_is_not_displayed_in_another_directory() {
        let mut app = test_app();
        let saved = saved_ssh(&app.db, "files", None);
        app.refresh_devices();
        app.sftp_pending = Some(remote_files::Pending {
            device_id: saved.id,
            path: "/old".into(),
            task: background::Task::spawn(|| {
                Ok(vec![SftpEntry {
                    name: "old file".into(),
                    path: "/old/old file".into(),
                    is_dir: false,
                    is_link: false,
                    size: 1,
                    permissions: "-rw-------".into(),
                    modified: "today".into(),
                }])
            })
            .unwrap(),
        });
        app.sftp_path = "/new".into();
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.sftp_pending.is_some() {
            app.poll_sftp();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(app.sftp_entries.is_empty());
        assert!(app.sftp_location.is_none());
    }

    #[test]
    fn sftp_failure_clears_pending_state_and_reports_error() {
        let mut app = test_app();
        let saved = saved_ssh(&app.db, "files", None);
        app.refresh_devices();
        app.sftp_pending = Some(remote_files::Pending {
            device_id: saved.id,
            path: app.sftp_path.clone(),
            task: background::Task::spawn(|| Err("permission denied".into())).unwrap(),
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.sftp_pending.is_some() {
            app.poll_sftp();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(app.sftp_msg.contains("permission denied"));
    }

    struct RejectResizeOnce(Arc<AtomicUsize>);
    impl ConnectionProvider for RejectResizeOnce {
        fn write(&self, _: &[u8]) -> opsterm_core::error::AppResult<()> {
            Ok(())
        }
        fn resize(&self, _: TerminalDimensions) -> opsterm_core::error::AppResult<()> {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(opsterm_core::error::AppError::Validation(
                    "queue full".into(),
                ))
            } else {
                Ok(())
            }
        }
        fn kill(&self) -> opsterm_core::error::AppResult<()> {
            Ok(())
        }
        fn take_reader(&self) -> opsterm_core::error::AppResult<Box<dyn Read + Send>> {
            Ok(Box::new(std::io::empty()))
        }
        fn is_alive(&self) -> bool {
            true
        }
    }

    #[test]
    fn rejected_resize_is_retried_on_the_next_frame() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut session = Session::new(
            "resize".into(),
            Box::new(RejectResizeOnce(calls.clone())),
            80,
            24,
            None,
            Origin::Local,
        );
        session.resize(120, 40);
        session.resize(120, 40);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "final size was not retried after backpressure"
        );
        assert_eq!((session.cols, session.rows), (120, 40));
    }

    #[test]
    fn regression_agent_image_paste_key_reaches_the_terminal() {
        assert_eq!(
            key_to_bytes(egui::Key::V, egui::Modifiers::CTRL, TermMode::default()),
            Some(vec![0x16])
        );
    }

    #[test]
    fn arrows_follow_application_cursor_mode() {
        let (mut session, _) = test_session(std::io::empty());
        assert_eq!(
            session.key_bytes(egui::Key::ArrowUp, egui::Modifiers::NONE),
            Some(b"\x1b[A".to_vec())
        );
        // ncurses/ncdu send DECSET ?1h, after which arrows must be SS3.
        session
            .processor
            .advance(&mut session.term, b"\x1b[?1h\x1b[?1049h");
        assert!(session.term.mode().contains(TermMode::APP_CURSOR));
        assert_eq!(
            session.key_bytes(egui::Key::ArrowUp, egui::Modifiers::NONE),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            session.key_bytes(egui::Key::ArrowDown, egui::Modifiers::NONE),
            Some(b"\x1bOB".to_vec())
        );
        session.processor.advance(&mut session.term, b"\x1b[?1l");
        assert_eq!(
            session.key_bytes(egui::Key::ArrowDown, egui::Modifiers::NONE),
            Some(b"\x1b[B".to_vec())
        );
    }

    #[test]
    fn modified_and_function_keys_use_xterm_sequences() {
        let mode = TermMode::default();
        assert_eq!(
            key_to_bytes(egui::Key::ArrowLeft, egui::Modifiers::CTRL, mode),
            Some(b"\x1b[1;5D".to_vec())
        );
        assert_eq!(
            key_to_bytes(egui::Key::ArrowUp, egui::Modifiers::ALT, mode),
            Some(b"\x1b[1;3A".to_vec())
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            key_to_bytes(egui::Key::ArrowRight, egui::Modifiers::ALT, mode),
            Some(b"\x1b[1;5C".to_vec())
        );
        assert_eq!(
            key_to_bytes(egui::Key::F1, egui::Modifiers::NONE, mode),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(
            key_to_bytes(egui::Key::F5, egui::Modifiers::NONE, mode),
            Some(b"\x1b[15~".to_vec())
        );
        assert_eq!(
            key_to_bytes(egui::Key::Delete, egui::Modifiers::SHIFT, mode),
            Some(b"\x1b[3;2~".to_vec())
        );
    }

    #[test]
    fn device_attribute_queries_are_routed_back_to_the_pty() {
        let (session, writes) = test_session(std::io::empty());
        let mut app = test_app();
        app.sessions.push(session);
        let ctx = egui::Context::default();
        let session = &mut app.sessions[0];
        session
            .processor
            .advance(&mut session.term, b"\x1b[c\x1b[5n");
        app.handle_session_events(&ctx);
        let written = writes.lock().unwrap().clone();
        assert!(written.starts_with(b"\x1b[?6c"));
        assert!(written.ends_with(b"\x1b[0n"));
    }

    #[test]
    fn regression_close_shortcut_keeps_live_session_until_confirmed() {
        let mut app = test_app();
        let (session, _) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1180.0, 760.0))),
            modifiers,
            events: vec![egui::Event::Key {
                key: egui::Key::W,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| app.show(ui));
        assert_eq!(
            app.sessions.len(),
            1,
            "live session closed before confirmation"
        );
    }

    fn close_test_frame(
        app: &mut App,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        native_close: bool,
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1180.0, 760.0))),
            events,
            ..Default::default()
        };
        if native_close {
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
        }
        ctx.run_ui(input, |ui| app.show(ui))
    }
    fn close_test_key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn regression_native_close_enter_does_not_submit_background_composer() {
        let mut app = test_app();
        let (session, writes) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        app.composer_open = true;
        app.command = "must stay in editor".into();
        let ctx = egui::Context::default();
        close_test_frame(&mut app, &ctx, vec![], false);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("command-editor")));
        close_test_frame(&mut app, &ctx, vec![], false);
        close_test_frame(
            &mut app,
            &ctx,
            vec![close_test_key(egui::Key::Enter, egui::Modifiers::NONE)],
            true,
        );
        assert!(
            writes.lock().unwrap().is_empty(),
            "native close leaked Enter to the command composer"
        );
        assert_eq!(app.command, "must stay in editor");
        assert_eq!(app.sessions.len(), 1);
    }

    #[test]
    fn close_confirmation_keeps_target_identity_after_tab_reorder() {
        let mut app = test_app();
        let (first, _) = test_session(std::io::empty());
        let first_id = first.id;
        let (second, _) = test_session(std::io::empty());
        let second_id = second.id;
        app.sessions.extend([first, second]);
        app.fonts_ready = true;
        app.request_close_tab(0);
        app.move_tab(0, 1);
        let ctx = egui::Context::default();
        close_test_frame(&mut app, &ctx, vec![], false);
        close_test_frame(
            &mut app,
            &ctx,
            vec![close_test_key(egui::Key::Enter, egui::Modifiers::CTRL)],
            false,
        );
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.sessions[0].id, second_id);
        assert_ne!(app.sessions[0].id, first_id);
    }

    #[test]
    fn native_close_is_cancelled_until_explicit_confirmation() {
        let mut app = test_app();
        let (session, _) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        let output = close_test_frame(&mut app, &ctx, vec![], true);
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        assert!(!app.allow_close);
        assert_eq!(app.sessions.len(), 1);
        let output = close_test_frame(
            &mut app,
            &ctx,
            vec![close_test_key(egui::Key::Enter, egui::Modifiers::CTRL)],
            false,
        );
        assert!(app.allow_close);
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close));
    }

    #[test]
    fn cancelling_native_close_restores_terminal_focus_without_a_click() {
        let mut app = test_app();
        let (session, writes) = test_session(std::io::empty());
        app.terminal_focus = Some(session.id);
        app.sessions.push(session);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        close_test_frame(&mut app, &ctx, vec![], false);
        let original = ctx.memory(|memory| memory.focused());
        assert!(original.is_some());
        close_test_frame(&mut app, &ctx, vec![], true);
        close_test_frame(
            &mut app,
            &ctx,
            vec![close_test_key(egui::Key::Escape, egui::Modifiers::NONE)],
            false,
        );
        // The next native input frame must already be ready for typing.
        close_test_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("typed after cancel".into())],
            false,
        );
        assert_eq!(&*writes.lock().unwrap(), b"typed after cancel");
        assert_eq!(ctx.memory(|memory| memory.focused()), original);
    }

    #[test]
    fn native_close_enter_cancel_is_not_reopened_by_layout_replay() {
        let mut app = test_app();
        let (session, _) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        close_test_frame(&mut app, &ctx, vec![], true);
        // Cancel already has focus; a repeated native close arrives with Enter.
        close_test_frame(
            &mut app,
            &ctx,
            vec![close_test_key(egui::Key::Enter, egui::Modifiers::NONE)],
            true,
        );
        assert!(
            !app.close_confirmation.active(),
            "the same native close event reopened a cancelled dialog"
        );
        assert!(!app.allow_close);
        assert_eq!(app.sessions.len(), 1);
    }

    #[test]
    fn disconnected_tab_closes_without_confirmation() {
        let mut app = test_app();
        let (mut session, _) = test_session(std::io::empty());
        session.exited = true;
        app.sessions.push(session);
        app.request_close_tab(0);
        assert!(app.sessions.is_empty());
        assert!(!app.close_confirmation.active());
        assert!(app.close_input_blocked);
    }

    struct IdleProvider(TestProvider);
    impl ConnectionProvider for IdleProvider {
        fn write(&self, bytes: &[u8]) -> opsterm_core::error::AppResult<()> {
            self.0.write(bytes)
        }
        fn resize(&self, dims: TerminalDimensions) -> opsterm_core::error::AppResult<()> {
            self.0.resize(dims)
        }
        fn kill(&self) -> opsterm_core::error::AppResult<()> {
            self.0.kill()
        }
        fn take_reader(&self) -> opsterm_core::error::AppResult<Box<dyn Read + Send>> {
            self.0.take_reader()
        }
        fn is_alive(&self) -> bool {
            self.0.is_alive()
        }
        fn has_running_command(&self) -> bool {
            false
        }
    }

    fn idle_test_session() -> Session {
        Session::new(
            "idle".into(),
            Box::new(IdleProvider(TestProvider {
                reader: Mutex::new(Some(Box::new(std::io::empty()))),
                writes: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(AtomicBool::new(true)),
            })),
            80,
            24,
            None,
            Origin::Local,
        )
    }

    #[test]
    fn idle_local_tab_closes_immediately_but_active_remote_still_prompts() {
        let mut app = test_app();
        app.sessions.push(idle_test_session());
        app.request_close_tab(0);
        assert!(app.sessions.is_empty());
        assert!(!app.close_confirmation.active());
        let mut remote = idle_test_session();
        remote.origin = Origin::Telnet(TelnetSpec {
            host: "127.0.0.1".into(),
            port: 23,
            username: None,
            term: None,
            password: None,
        });
        app.sessions.push(remote);
        app.request_close_tab(0);
        assert_eq!(app.sessions.len(), 1);
        assert!(app.close_confirmation.active());
    }

    #[test]
    fn native_close_ignores_idle_dead_and_opening_tabs() {
        let mut app = test_app();
        app.sessions.push(idle_test_session());
        let (mut dead, _) = test_session(std::io::empty());
        dead.exited = true;
        app.sessions.push(dead);
        let (mut opening, _) = test_session(std::io::empty());
        opening.opening = true;
        app.sessions.push(opening);
        app.fonts_ready = true;
        let output = close_test_frame(&mut app, &egui::Context::default(), vec![], true);
        assert!(!output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        assert!(!app.close_confirmation.active());
    }

    #[test]
    fn immediate_idle_native_close_blocks_background_enter() {
        let mut app = test_app();
        let _release = hold_connection_workers(&mut app);
        app.sessions.push(idle_test_session());
        app.fonts_ready = true;
        app.quick = "ops@127.0.0.1:1".into();
        let ctx = egui::Context::default();
        close_test_frame(&mut app, &ctx, vec![], false);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("quick-connect")));
        close_test_frame(&mut app, &ctx, vec![], false);
        let output = close_test_frame(
            &mut app,
            &ctx,
            vec![close_test_key(egui::Key::Enter, egui::Modifiers::NONE)],
            true,
        );
        assert!(!output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        assert!(!app.close_confirmation.active());
        assert!(app.queued_connections.is_empty());
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.quick, "ops@127.0.0.1:1");
    }

    #[test]
    fn window_close_counts_only_sessions_at_risk() {
        let mut app = test_app();
        app.sessions.push(idle_test_session());
        let (busy, _) = test_session(std::io::empty());
        app.sessions.push(busy);
        assert_eq!(app.risky_session_count(), 1);
    }

    #[test]
    fn opening_placeholder_closes_without_confirmation() {
        let mut app = test_app();
        let (mut session, _) = test_session(std::io::empty());
        session.opening = true;
        app.sessions.push(session);
        app.request_close_tab(0);
        assert!(app.sessions.is_empty());
        assert!(!app.close_confirmation.active());
    }

    #[test]
    fn regression_native_close_enter_does_not_launch_quick_connect() {
        let mut app = test_app();
        let _release = hold_connection_workers(&mut app);
        let (session, _) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        app.quick = "ops@127.0.0.1:1".into();
        let ctx = egui::Context::default();
        close_test_frame(&mut app, &ctx, vec![], false);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("quick-connect")));
        close_test_frame(&mut app, &ctx, vec![], false);
        close_test_frame(
            &mut app,
            &ctx,
            vec![close_test_key(egui::Key::Enter, egui::Modifiers::NONE)],
            true,
        );
        assert_eq!(
            app.sessions.len(),
            1,
            "native-close Enter opened a background connection"
        );
        assert_eq!(app.quick, "ops@127.0.0.1:1");
        assert!(app.queued_connections.is_empty());
    }

    #[test]
    fn regression_close_all_requires_showing_the_new_prompt_before_confirming() {
        let mut app = test_app();
        let (first, _) = test_session(std::io::empty());
        let (second, _) = test_session(std::io::empty());
        app.sessions.extend([first, second]);
        app.fonts_ready = true;
        let ctx = egui::Context::default();
        app.request_close_tab(0);
        close_test_frame(&mut app, &ctx, vec![], false);
        close_test_frame(
            &mut app,
            &ctx,
            vec![close_test_key(egui::Key::Enter, egui::Modifiers::CTRL)],
            true,
        );
        assert!(
            !app.allow_close,
            "a close-one confirmation was reused to approve closing all tabs"
        );
        assert!(app.close_confirmation.active());
        assert_eq!(app.sessions.len(), 2);
    }

    #[test]
    fn cancelling_tab_close_restores_previously_focused_editor() {
        let mut app = test_app();
        let (session, writes) = test_session(std::io::empty());
        app.sessions.push(session);
        app.fonts_ready = true;
        app.quick = "saved text".into();
        let ctx = egui::Context::default();
        close_test_frame(&mut app, &ctx, vec![], false);
        let editor = egui::Id::new("quick-connect");
        ctx.memory_mut(|memory| memory.request_focus(editor));
        close_test_frame(&mut app, &ctx, vec![], false);
        app.request_close_tab(0);
        close_test_frame(&mut app, &ctx, vec![], false);
        close_test_frame(
            &mut app,
            &ctx,
            vec![close_test_key(egui::Key::Escape, egui::Modifiers::NONE)],
            false,
        );
        close_test_frame(&mut app, &ctx, vec![], false);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(editor));
        close_test_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text(" suffix".into())],
            false,
        );
        assert_eq!(app.quick, "saved text suffix");
        assert!(writes.lock().unwrap().is_empty());
    }

    struct BurstReader(Arc<AtomicUsize>);
    impl Read for BurstReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0.fetch_add(1, Ordering::SeqCst) >= 512 {
                return Ok(0);
            }
            buf.fill(b'x');
            Ok(buf.len())
        }
    }

    #[test]
    fn undrained_output_backpressures_reader() {
        let reads = Arc::new(AtomicUsize::new(0));
        let (s, _) = test_session(BurstReader(reads.clone()));
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline && reads.load(Ordering::SeqCst) < 512 {
            std::thread::sleep(Duration::from_millis(2));
        }
        let read_count = reads.load(Ordering::SeqCst);
        drop(s);
        assert!(
            read_count <= 33,
            "unbounded read-ahead: {read_count} chunks"
        );
    }

    #[test]
    fn workspace_keeps_saved_ssh_device_reference() {
        let mut app = test_app();
        let input = SavedSessionInput {
            id: None,
            folder_id: None,
            name: "loopback".into(),
            host: "127.0.0.1".into(),
            port: 9,
            protocol: "ssh".into(),
            username: None,
            ssh_alias: None,
            identity_file: None,
            proxy_jump: None,
            tags: vec![],
            description: None,
            notes: None,
            settings_override: None,
            credential_ref: None,
            is_favorite: false,
            sort_order: None,
        };
        let saved = app.db.upsert_session(&input).unwrap();
        app.open_saved(&saved);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !app.pending_connections.is_empty() || !app.queued_connections.is_empty() {
            app.poll_connections();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        app.ws_name = "test workspace".into();
        app.save_workspace();
        let all = app.db.list_workspaces().unwrap();
        assert_eq!(all[0].layout["devices"], serde_json::json!([saved.id]));
        for s in &app.sessions {
            let _ = s.provider.kill();
        }
    }

    #[test]
    fn workspace_restore_retains_requests_beyond_active_worker_limit() {
        let mut app = test_app();
        // Fill slots with controlled jobs; do not open sockets.
        for _ in 0..16 {
            app.pending_connections.push(connections::Pending {
                request: connections::Request {
                    title: "pending".into(),
                    origin: Origin::Local,
                    saved: None,
                    secret: None,
                    target: None,
                    dims: TerminalDimensions { cols: 80, rows: 24 },
                },
                task: background::Task::spawn(|| Err("test failure".into())).unwrap(),
            });
        }
        let saved = saved_ssh(&app.db, "workspace device", None);
        app.restore_workspace(&serde_json::json!({ "devices": vec![saved.id; 20] }));
        assert_eq!(
            app.pending_connections.len() + app.queued_connections.len(),
            36,
            "no saved device request may be dropped"
        );
    }

    #[test]
    fn normalize_path_collapses_slashes() {
        assert_eq!(normalize_path("//home//ubuntu/"), "/home/ubuntu");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path(""), "/");
        assert_eq!(normalize_path("/a/b/c"), "/a/b/c");
    }

    #[test]
    fn subnet_calc_basic() {
        let s = subnet_calc("192.168.1.10", 24).unwrap();
        assert!(s.contains("192.168.1.0/24"));
        assert!(s.contains("255.255.255.0"));
        assert!(s.contains("192.168.1.255"));
        assert!(s.contains("192.168.1.1 - 192.168.1.254"));
    }

    #[test]
    fn subnet_calc_edge_cases() {
        assert!(subnet_calc("not-an-ip", 24).is_none());
        assert!(subnet_calc("10.0.0.1", 33).is_none());
        assert!(subnet_calc("10.0.0.1", 32).is_some());
        assert!(subnet_calc("0.0.0.0", 0).is_some());
    }

    #[test]
    fn expect_parses_commands() {
        let steps =
            parse_expect("sendline hello\nwaitfor [#>]\\s*$ 5000\nsleep 100\n# comment\nsend x");
        assert_eq!(steps.len(), 4);
        match &steps[0] {
            Step::Send(d, enter) => {
                assert_eq!(d, "hello");
                assert!(*enter);
            }
            _ => panic!("expected send"),
        }
        match &steps[1] {
            Step::WaitFor(pat, ms) => {
                assert_eq!(pat, "[#>]\\s*$");
                assert_eq!(*ms, 5000);
            }
            _ => panic!("expected waitfor"),
        }
        match &steps[2] {
            Step::Sleep(ms) => assert_eq!(*ms, 100),
            _ => panic!("expected sleep"),
        }
        match &steps[3] {
            Step::Send(d, enter) => {
                assert_eq!(d, "x");
                assert!(!*enter);
            }
            _ => panic!("expected send"),
        }
    }

    #[test]
    fn highlight_finds_keywords() {
        let rules = vec![("down".to_string(), [224, 108, 117], true)];
        let marks = line_highlights("Gi0/1 is down", &rules);
        assert_eq!(marks.len(), 1);
        let (start, end, _) = marks[0];
        assert_eq!(&"Gi0/1 is down"[start..=end], "down");
    }

    #[test]
    fn highlight_respects_disabled_rule() {
        let rules = vec![("down".to_string(), [1, 2, 3], false)];
        assert!(line_highlights("down down", &rules).is_empty());
    }

    #[test]
    fn serial_spec_defaults() {
        let saved = SavedSessionInput {
            id: Some("x".into()),
            folder_id: None,
            name: "s".into(),
            host: "/dev/ttyUSB0".into(),
            port: 0,
            protocol: "serial".into(),
            username: None,
            ssh_alias: None,
            identity_file: None,
            proxy_jump: None,
            tags: vec![],
            description: None,
            notes: None,
            settings_override: None,
            credential_ref: None,
            is_favorite: false,
            sort_order: None,
        };
        let db = Database::open_in_memory().unwrap();
        let saved = db.upsert_session(&saved).unwrap();
        let spec = serial_spec_from_saved(&saved);
        assert_eq!(spec.device, "/dev/ttyUSB0");
        assert_eq!(spec.baud, 9600);
        assert_eq!(spec.data_bits, 8);
        assert_eq!(spec.parity, "none");
    }

    #[test]
    fn pager_presets_map_correctly() {
        let ov = Some(serde_json::json!({ "pager": "huawei" }));
        assert_eq!(
            pager_command(&ov).as_deref(),
            Some("screen-length 0 temporary")
        );
        let ov = Some(serde_json::json!({ "pager": "custom", "pagerCommand": "foo bar" }));
        assert_eq!(pager_command(&ov).as_deref(), Some("foo bar"));
        assert!(pager_command(&None).is_none());
    }
}
