//! OpsTerm Native — a lightweight GTK4 + VTE desktop client.
//!
//! Its own standalone application window (not GNOME Terminal). Reuses the
//! UI-agnostic `opsterm-core` crate (storage, validation, argv building,
//! credentials) and renders terminals with VTE natively, so idle tabs are
//! cheap and the binary stays small.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4 as gtk;
use libadwaita::prelude::*;
use vte4::prelude::*;

use opsterm_core::domain::{
    AppSettings, SavedSession, SavedSessionInput, SerialSpec, SshSpec, TelnetSpec,
    TerminalDimensions,
};
use opsterm_core::providers::{self, ConnectionProvider};
use opsterm_core::{credential, security, storage::Database};

const APP_ID: &str = "org.opsterm.native";

/// Minimal tweaks on top of libadwaita's own theme.
const CSS: &str = r#"
.ot-status { color: alpha(currentColor, .6); font-size: 11px; }
"#;

fn install_theme() {
    if let Some(display) = gtk::gdk::Display::default() {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
}

fn rgba(hex: &str) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::parse(hex).unwrap_or(gtk::gdk::RGBA::BLACK)
}

struct AppCtx {
    db: Database,
    sessions: RefCell<Vec<SavedSession>>,
    settings: RefCell<AppSettings>,
    askpass_dirs: RefCell<Vec<PathBuf>>,
}

impl AppCtx {
    fn reload_sessions(&self) {
        if let Ok(list) = self.db.list_sessions() {
            *self.sessions.borrow_mut() = list;
        }
    }
    fn font(&self) -> gtk::pango::FontDescription {
        let size = self.settings.borrow().terminal.font_size;
        gtk::pango::FontDescription::from_string(&format!("monospace {size}"))
    }
    fn scrollback(&self) -> i64 {
        self.settings.borrow().terminal.scrollback as i64
    }
}

fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    base.join("org.opsterm.app")
}

fn spec_from_saved(saved: &SavedSession) -> SshSpec {
    SshSpec {
        host: saved.host.clone(),
        port: saved.port,
        username: saved.username.clone(),
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

/// Add a VTE tab that runs `argv` under VTE's own PTY.
fn add_terminal(
    notebook: &gtk::Notebook,
    ctx: &Rc<AppCtx>,
    title: &str,
    argv: Vec<String>,
    env: Vec<(String, String)>,
) {
    let term = vte4::Terminal::new();
    term.set_scrollback_lines(ctx.scrollback());
    term.set_font(Some(&ctx.font()));
    term.set_cursor_blink_mode(vte4::CursorBlinkMode::On);
    term.set_color_background(&rgba("#111418"));
    term.set_color_foreground(&rgba("#E5E9EF"));
    term.set_color_cursor(Some(&rgba("#4F8EE8")));
    term.set_color_highlight(Some(&rgba("#2B323A")));
    term.set_hexpand(true);
    term.set_vexpand(true);

    let argv_refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    let child_setup = move || {
        for (k, v) in &env {
            if let (Ok(k), Ok(v)) = (
                std::ffi::CString::new(k.as_str()),
                std::ffi::CString::new(v.as_str()),
            ) {
                unsafe {
                    libc::setenv(k.as_ptr(), v.as_ptr(), 1);
                }
            }
        }
    };

    term.spawn_async(
        vte4::PtyFlags::DEFAULT,
        None,
        &argv_refs,
        &[],
        gtk::glib::SpawnFlags::DEFAULT,
        child_setup,
        -1,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            if let Err(e) = result {
                eprintln!("spawn failed: {e}");
            }
        },
    );

    // Tab label with a close button.
    let tab = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    tab.append(&gtk::Label::new(Some(title)));
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("flat");
    close.set_tooltip_text(Some("Close tab"));
    tab.append(&close);

    let idx = notebook.append_page(&term, Some(&tab));
    notebook.set_current_page(Some(idx));
    term.grab_focus();

    let nb = notebook.clone();
    let term_for_close = term.clone();
    close.connect_clicked(move |_| {
        if let Some(page) = nb.page_num(&term_for_close) {
            nb.remove_page(Some(page));
        }
    });
}

fn local_shell_argv() -> Vec<String> {
    vec![security::resolve_local_shell().unwrap_or_else(|_| "/bin/bash".into())]
}

fn active_terminal(notebook: &gtk::Notebook) -> Option<vte4::Terminal> {
    notebook
        .nth_page(notebook.current_page())
        .and_downcast::<vte4::Terminal>()
}

fn rebuild_session_list(list: &gtk::ListBox, ctx: &Rc<AppCtx>) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    for s in ctx.sessions.borrow().iter() {
        let row = gtk::ListBoxRow::new();
        let label = gtk::Label::new(Some(&format!("{}  ({})", s.name, s.protocol)));
        label.set_xalign(0.0);
        row.set_child(Some(&label));
        list.append(&row);
    }
}

fn build_ui(app: &libadwaita::Application) {
    let db = Database::open(&data_dir().join("opsterm.db")).expect("open database");
    let settings = db.load_settings().unwrap_or_default();
    let sessions = db.list_sessions().unwrap_or_default();
    let ctx = Rc::new(AppCtx {
        db,
        sessions: RefCell::new(sessions),
        settings: RefCell::new(settings),
        askpass_dirs: RefCell::new(Vec::new()),
    });

    let window = libadwaita::ApplicationWindow::builder()
        .application(app)
        .title("OpsTerm")
        .default_width(1120)
        .default_height(740)
        .build();

    // Header (libadwaita gives the modern GNOME headerbar look).
    let header = libadwaita::HeaderBar::new();
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    title_box.set_valign(gtk::Align::Center);
    let title_lbl = gtk::Label::new(Some("OpsTerm"));
    title_lbl.add_css_class("title");
    let sub_lbl = gtk::Label::new(Some("secure terminal workspace"));
    sub_lbl.add_css_class("subtitle");
    title_box.append(&title_lbl);
    title_box.append(&sub_lbl);
    header.set_title_widget(Some(&title_box));
    let local_btn = gtk::Button::with_label("Local Shell");
    let new_btn = gtk::Button::with_label("New Device");
    new_btn.add_css_class("suggested-action");
    let tools_btn = gtk::Button::with_label("Tools");
    let settings_btn = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_btn.set_tooltip_text(Some("Settings"));
    header.pack_start(&local_btn);
    header.pack_start(&new_btn);
    header.pack_end(&settings_btn);
    header.pack_end(&tools_btn);

    // Sidebar.
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 6);
    sidebar.set_width_request(260);
    sidebar.set_margin_top(6);
    sidebar.set_margin_bottom(6);
    sidebar.set_margin_start(6);
    sidebar.set_margin_end(6);
    let filter = gtk::SearchEntry::new();
    filter.set_placeholder_text(Some("Filter sessions"));
    sidebar.append(&filter);
    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.set_selection_mode(gtk::SelectionMode::Single);
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();
    sidebar.append(&scroller);
    rebuild_session_list(&list, &ctx);
    let sidebar_page = libadwaita::NavigationPage::new(&sidebar, "Saved Sessions");

    // Content: tabs.
    let notebook = gtk::Notebook::new();
    notebook.set_scrollable(true);
    notebook.set_vexpand(true);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    content.set_margin_start(6);
    content.set_margin_end(6);
    content.append(&notebook);
    let content_page = libadwaita::NavigationPage::new(&content, "Terminal");

    let split = libadwaita::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .build();
    split.set_min_sidebar_width(220.0);
    split.set_max_sidebar_width(360.0);

    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&split));

    // Bottom bar: quick connect + button bar + status.
    let quick = gtk::Entry::new();
    quick.set_placeholder_text(Some("user@hostname or host"));
    quick.set_hexpand(true);
    let connect_btn = gtk::Button::with_label("Connect");
    connect_btn.add_css_class("suggested-action");
    let quick_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    quick_bar.set_margin_top(6);
    quick_bar.set_margin_bottom(2);
    quick_bar.set_margin_start(6);
    quick_bar.set_margin_end(6);
    quick_bar.append(&quick);
    quick_bar.append(&connect_btn);

    let button_bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    button_bar.set_margin_start(6);
    button_bar.set_margin_end(6);
    button_bar.set_margin_bottom(2);
    {
        let commands = ctx.db.list_commands().unwrap_or_default();
        let nb = notebook.clone();
        for command in commands.iter().take(24) {
            let btn = gtk::Button::with_label(&command.name);
            btn.add_css_class("pill");
            let content_str = command.content.clone();
            let nb = nb.clone();
            btn.connect_clicked(move |_| {
                if let Some(term) = active_terminal(&nb) {
                    term.feed_child(content_str.as_bytes());
                    term.feed_child(b"\r");
                }
            });
            button_bar.append(&btn);
        }
    }

    let status = gtk::Label::new(Some("Ready"));
    status.set_xalign(0.0);
    status.add_css_class("ot-status");
    status.set_margin_start(8);
    status.set_margin_end(8);
    status.set_margin_bottom(6);

    let bottom = gtk::Box::new(gtk::Orientation::Vertical, 2);
    bottom.append(&quick_bar);
    bottom.append(&button_bar);
    bottom.append(&status);
    toolbar.add_bottom_bar(&bottom);

    window.set_content(Some(&toolbar));

    // Open a saved session.
    {
        let ctx2 = ctx.clone();
        let notebook2 = notebook.clone();
        let status2 = status.clone();
        list.connect_row_activated(move |_, row| {
            let idx = row.index();
            if idx < 0 {
                return;
            }
            let saved = ctx2.sessions.borrow().get(idx as usize).cloned();
            let Some(saved) = saved else { return };
            let password = saved
                .credential_ref
                .as_deref()
                .filter(|r| !r.is_empty())
                .and_then(|r| credential::resolve(r).ok());
            // Username may live on the credential (tag) rather than the device.
            let cred_user = saved
                .credential_ref
                .as_deref()
                .filter(|r| !r.is_empty())
                .and_then(|r| ctx2.db.get_credential(r).ok().flatten())
                .and_then(|c| c.username);
            let engine = saved_engine(&saved);
            match saved.protocol.as_str() {
                "ssh" | "" if engine == "native" => {
                    let mut spec = spec_from_saved(&saved);
                    if spec
                        .username
                        .as_deref()
                        .map(|u| u.trim().is_empty())
                        .unwrap_or(true)
                    {
                        spec.username = cred_user.clone();
                    }
                    spec.password = None;
                    if let Ok(json) = serde_json::to_string(&spec) {
                        let env = relay_pw_env(password.as_deref());
                        add_terminal(
                            &notebook2,
                            &ctx2,
                            &saved.name,
                            relay_argv("russh", json),
                            env,
                        );
                        status2.set_text("Connecting (native engine)…");
                    }
                }
                "ssh" | "" => {
                    let mut spec = spec_from_saved(&saved);
                    if spec
                        .username
                        .as_deref()
                        .map(|u| u.trim().is_empty())
                        .unwrap_or(true)
                    {
                        spec.username = cred_user.clone();
                    }
                    spec.password = password.clone();
                    match security::build_ssh_argv(&spec) {
                        Ok(argv) => {
                            let env = askpass_env(&ctx2, password.as_deref());
                            add_terminal(&notebook2, &ctx2, &saved.name, argv, env);
                            status2.set_text(&format!("Connecting to {}", saved.host));
                        }
                        Err(e) => status2.set_text(&format!("Invalid: {e}")),
                    }
                }
                "telnet" => {
                    let mut spec = TelnetSpec {
                        host: saved.host.clone(),
                        port: if saved.port == 0 { 23 } else { saved.port },
                        username: saved.username.clone().or_else(|| cred_user.clone()),
                        term: Some("xterm-256color".into()),
                        password: None,
                    };
                    spec.password = None;
                    match serde_json::to_string(&spec) {
                        Ok(json) => {
                            let env = relay_pw_env(password.as_deref());
                            add_terminal(
                                &notebook2,
                                &ctx2,
                                &saved.name,
                                relay_argv("telnet", json),
                                env,
                            );
                            status2.set_text("Telnet (insecure)");
                        }
                        Err(e) => status2.set_text(&format!("{e}")),
                    }
                }
                "serial" => {
                    let spec = serial_spec_from_saved(&saved);
                    match serde_json::to_string(&spec) {
                        Ok(json) => {
                            add_terminal(
                                &notebook2,
                                &ctx2,
                                &saved.name,
                                relay_argv("serial", json),
                                vec![],
                            );
                            status2.set_text("Serial");
                        }
                        Err(e) => status2.set_text(&format!("{e}")),
                    }
                }
                other => status2.set_text(&format!("{other} not supported in native yet")),
            }
        });
    }

    // Local shell.
    {
        let ctx2 = ctx.clone();
        let notebook2 = notebook.clone();
        local_btn.connect_clicked(move |_| {
            add_terminal(&notebook2, &ctx2, "Local Shell", local_shell_argv(), vec![]);
        });
    }

    // Quick connect.
    {
        let ctx2 = ctx.clone();
        let notebook2 = notebook.clone();
        let status2 = status.clone();
        let quick2 = quick.clone();
        let do_connect = move || {
            let text = quick2.text().to_string().trim().to_string();
            let text = text.trim_start_matches("ssh ").to_string();
            if text.is_empty() {
                return;
            }
            let (user, host) = match text.split_once('@') {
                Some((u, h)) => (Some(u.to_string()), h.to_string()),
                None => (None, text),
            };
            let mut spec = SshSpec {
                host,
                port: 22,
                username: user,
                identity_file: None,
                proxy_jump: None,
                name: None,
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
            };
            match security::build_ssh_argv(&spec) {
                Ok(argv) => {
                    let title = spec
                        .username
                        .as_ref()
                        .map(|u| format!("{u}@{}", spec.host))
                        .unwrap_or_else(|| spec.host.clone());
                    spec.password = None;
                    add_terminal(&notebook2, &ctx2, &title, argv, vec![]);
                    status2.set_text("Connecting…");
                }
                Err(e) => status2.set_text(&format!("Invalid: {e}")),
            }
        };
        let dc = do_connect.clone();
        connect_btn.connect_clicked(move |_| dc());
        quick.connect_activate(move |_| do_connect());
    }

    // Filter.
    {
        let list2 = list.clone();
        filter.connect_search_changed(move |f| {
            let needle = f.text().to_string().to_lowercase();
            let mut i = 0;
            while let Some(row) = list2.row_at_index(i) {
                let visible = needle.is_empty()
                    || row
                        .child()
                        .and_downcast::<gtk::Label>()
                        .map(|l| l.text().to_lowercase().contains(&needle))
                        .unwrap_or(true);
                row.set_visible(visible);
                i += 1;
            }
        });
    }

    // New device dialog.
    {
        let ctx2 = ctx.clone();
        let list2 = list.clone();
        let window2 = window.clone();
        new_btn.connect_clicked(move |_| {
            let dialog = gtk::Window::builder()
                .title("New Device")
                .transient_for(&window2)
                .modal(true)
                .default_width(360)
                .build();
            let grid = gtk::Grid::builder()
                .row_spacing(6)
                .column_spacing(6)
                .margin_top(10)
                .margin_bottom(10)
                .margin_start(10)
                .margin_end(10)
                .build();
            let name = gtk::Entry::new();
            let host = gtk::Entry::new();
            let port = gtk::SpinButton::with_range(1.0, 65535.0, 1.0);
            port.set_value(22.0);
            let user = gtk::Entry::new();
            let cred = gtk::Entry::new();
            cred.set_placeholder_text(Some("credential name (optional)"));
            for (i, (lbl, widget)) in [
                ("Name", name.clone().upcast::<gtk::Widget>()),
                ("Host", host.clone().upcast()),
                ("Port", port.clone().upcast()),
                ("Username", user.clone().upcast()),
                ("Credential", cred.clone().upcast()),
            ]
            .into_iter()
            .enumerate()
            {
                grid.attach(&gtk::Label::new(Some(lbl)), 0, i as i32, 1, 1);
                grid.attach(&widget, 1, i as i32, 1, 1);
            }
            let save = gtk::Button::with_label("Save");
            let cancel = gtk::Button::with_label("Cancel");
            let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            buttons.set_halign(gtk::Align::End);
            buttons.append(&cancel);
            buttons.append(&save);
            grid.attach(&buttons, 0, 5, 2, 1);

            let dialog2 = dialog.clone();
            cancel.connect_clicked(move |_| dialog2.close());

            let ctx3 = ctx2.clone();
            let list3 = list2.clone();
            let dialog3 = dialog.clone();
            save.connect_clicked(move |_| {
                let input = SavedSessionInput {
                    id: None,
                    folder_id: None,
                    name: name.text().to_string(),
                    host: host.text().to_string(),
                    port: port.value() as u16,
                    protocol: "ssh".into(),
                    username: {
                        let u = user.text().to_string();
                        if u.is_empty() {
                            None
                        } else {
                            Some(u)
                        }
                    },
                    ssh_alias: None,
                    identity_file: None,
                    proxy_jump: None,
                    tags: vec![],
                    description: None,
                    notes: None,
                    settings_override: None,
                    credential_ref: {
                        let c = cred.text().to_string();
                        if c.is_empty() {
                            None
                        } else {
                            Some(c)
                        }
                    },
                    is_favorite: false,
                    sort_order: None,
                };
                if ctx3.db.upsert_session(&input).is_ok() {
                    ctx3.reload_sessions();
                    rebuild_session_list(&list3, &ctx3);
                    dialog3.close();
                } else {
                    eprintln!("failed to save session");
                }
            });
            dialog.set_child(Some(&grid));
            dialog.present();
        });
    }

    // Tools window.
    {
        let ctx2 = ctx.clone();
        let window2 = window.clone();
        tools_btn.connect_clicked(move |_| {
            build_tools_window(&window2, &ctx2);
        });
    }

    // Settings dialog.
    {
        let ctx2 = ctx.clone();
        let window2 = window.clone();
        let notebook2 = notebook.clone();
        settings_btn.connect_clicked(move |_| {
            let dialog = gtk::Window::builder()
                .title("Settings")
                .transient_for(&window2)
                .modal(true)
                .default_width(320)
                .build();
            let grid = gtk::Grid::builder()
                .row_spacing(6)
                .column_spacing(6)
                .margin_top(10)
                .margin_bottom(10)
                .margin_start(10)
                .margin_end(10)
                .build();
            let font = gtk::SpinButton::with_range(8.0, 24.0, 1.0);
            font.set_value(ctx2.settings.borrow().terminal.font_size as f64);
            let scroll = gtk::SpinButton::with_range(1000.0, 200000.0, 1000.0);
            scroll.set_value(ctx2.settings.borrow().terminal.scrollback as f64);
            grid.attach(&gtk::Label::new(Some("Font size")), 0, 0, 1, 1);
            grid.attach(&font, 1, 0, 1, 1);
            grid.attach(&gtk::Label::new(Some("Scrollback")), 0, 1, 1, 1);
            grid.attach(&scroll, 1, 1, 1, 1);
            let save = gtk::Button::with_label("Apply");
            save.add_css_class("suggested-action");
            grid.attach(&save, 0, 2, 2, 1);

            let ctx3 = ctx2.clone();
            let dialog3 = dialog.clone();
            let nb = notebook2.clone();
            save.connect_clicked(move |_| {
                {
                    let mut s = ctx3.settings.borrow_mut();
                    s.terminal.font_size = font.value() as u16;
                    s.terminal.scrollback = scroll.value() as u32;
                }
                let s = ctx3.settings.borrow().clone();
                let _ = ctx3.db.save_settings(&s);
                for i in 0..nb.n_pages() {
                    if let Some(term) = nb.nth_page(Some(i)).and_downcast::<vte4::Terminal>() {
                        term.set_font(Some(&ctx3.font()));
                        term.set_scrollback_lines(ctx3.scrollback());
                    }
                }
                dialog3.close();
            });
            dialog.set_child(Some(&grid));
            dialog.present();
        });
    }

    // Status follows the active tab.
    {
        let status2 = status.clone();
        notebook.connect_switch_page(move |nb, _, page| {
            let title = nb
                .nth_page(Some(page))
                .and_then(|w| nb.tab_label(&w))
                .and_downcast::<gtk::Box>()
                .and_then(|b| b.first_child())
                .and_downcast::<gtk::Label>()
                .map(|l| l.text().to_string())
                .unwrap_or_default();
            status2.set_text(&title);
        });
    }

    window.present();
}

/// Current terminal size from the controlling tty (stdin).
fn tty_size() -> Option<TerminalDimensions> {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDIN_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
            Some(TerminalDimensions {
                cols: ws.ws_col,
                rows: ws.ws_row,
            })
        } else {
            None
        }
    }
}

static WINCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_winch(_: libc::c_int) {
    WINCH.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Relay mode: run a core provider and pipe it to stdio, so VTE (or any
/// terminal) can drive Telnet/Serial/native-SSH sessions. Invoked as
/// `opsterm-native --relay <kind> <json-spec>`.
fn run_relay(kind: &str, spec_json: &str) -> i32 {
    use std::io::{Read, Write};

    let dims = tty_size().unwrap_or(TerminalDimensions { cols: 80, rows: 24 });
    let password = std::env::var("OPSTERM_RELAY_PW").ok();

    let provider: Box<dyn ConnectionProvider> = match kind {
        "telnet" => match serde_json::from_str::<TelnetSpec>(spec_json) {
            Ok(mut s) => {
                s.password = password;
                match providers::telnet::spawn_telnet(&s, dims) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("telnet: {e}");
                        return 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("bad telnet spec: {e}");
                return 1;
            }
        },
        "serial" => match serde_json::from_str::<SerialSpec>(spec_json) {
            Ok(s) => match providers::serial::spawn_serial(&s, dims) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("serial: {e}");
                    return 1;
                }
            },
            Err(e) => {
                eprintln!("bad serial spec: {e}");
                return 1;
            }
        },
        "russh" => match serde_json::from_str::<SshSpec>(spec_json) {
            Ok(mut s) => {
                s.password = password;
                match providers::russh_ssh::spawn_russh(&s, dims) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("native ssh: {e}");
                        return 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("bad ssh spec: {e}");
                return 1;
            }
        },
        other => {
            eprintln!("unknown relay kind: {other}");
            return 1;
        }
    };

    let provider: std::sync::Arc<dyn ConnectionProvider> = std::sync::Arc::from(provider);
    let mut reader = match provider.take_reader() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("reader: {e}");
            return 1;
        }
    };

    // Resize on SIGWINCH.
    unsafe {
        libc::signal(
            libc::SIGWINCH,
            on_winch as extern "C" fn(libc::c_int) as libc::sighandler_t,
        );
    }
    {
        let provider = provider.clone();
        std::thread::spawn(move || loop {
            if WINCH.swap(false, std::sync::atomic::Ordering::SeqCst) {
                if let Some(d) = tty_size() {
                    let _ = provider.resize(d);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        });
    }

    // Provider output → stdout.
    {
        std::thread::spawn(move || {
            let mut out = std::io::stdout();
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if out.write_all(&buf[..n]).is_err() {
                            break;
                        }
                        let _ = out.flush();
                    }
                }
            }
            std::process::exit(0);
        });
    }

    // stdin → provider.
    let mut input = std::io::stdin();
    let mut buf = [0u8; 4096];
    loop {
        match input.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if provider.write(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    }
    let _ = provider.kill();
    0
}

/// Build the argv for a non-OpenSSH session by relaying through this binary.
fn relay_argv(kind: &str, spec_json: String) -> Vec<String> {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "opsterm-native".into());
    vec![exe, "--relay".to_string(), kind.to_string(), spec_json]
}

/// Environment for a relayed session carrying an inventory password.
fn relay_pw_env(password: Option<&str>) -> Vec<(String, String)> {
    match password.filter(|p| !p.is_empty()) {
        Some(pw) => vec![("OPSTERM_RELAY_PW".to_string(), pw.to_string())],
        None => vec![],
    }
}

/// Environment for a direct OpenSSH session using the askpass helper.
fn askpass_env(ctx: &Rc<AppCtx>, password: Option<&str>) -> Vec<(String, String)> {
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        if let Ok(dir) = opsterm_core::askpass::create_askpass(pw) {
            ctx.askpass_dirs.borrow_mut().push(dir.clone());
            return vec![
                (
                    "SSH_ASKPASS".to_string(),
                    dir.join("askpass.sh").to_string_lossy().into_owned(),
                ),
                ("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string()),
                ("DISPLAY".to_string(), ":0".to_string()),
            ];
        }
    }
    vec![]
}

/// Build the SSH spec for the currently selected SFTP session (with password).
fn sftp_spec(sessions: &Rc<Vec<SavedSession>>, pick: &gtk::DropDown) -> Option<SshSpec> {
    let saved = sessions.get(pick.selected() as usize)?;
    let mut spec = spec_from_saved(saved);
    spec.password = saved
        .credential_ref
        .as_deref()
        .and_then(|r| credential::resolve(r).ok());
    Some(spec)
}

fn saved_engine(saved: &SavedSession) -> String {
    saved
        .settings_override
        .as_ref()
        .and_then(|o| o.get("engine"))
        .and_then(|v| v.as_str())
        .unwrap_or("openssh")
        .to_string()
}

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
    if let Some(serial) = saved
        .settings_override
        .as_ref()
        .and_then(|o| o.get("serial"))
    {
        if let Some(v) = serial.get("baud").and_then(|v| v.as_u64()) {
            spec.baud = v as u32;
        }
        if let Some(v) = serial.get("dataBits").and_then(|v| v.as_u64()) {
            spec.data_bits = v as u8;
        }
        if let Some(v) = serial.get("parity").and_then(|v| v.as_str()) {
            spec.parity = v.to_string();
        }
        if let Some(v) = serial.get("stopBits").and_then(|v| v.as_u64()) {
            spec.stop_bits = v as u8;
        }
        if let Some(v) = serial.get("flowControl").and_then(|v| v.as_str()) {
            spec.flow_control = v.to_string();
        }
        if let Some(v) = serial.get("newline").and_then(|v| v.as_str()) {
            spec.newline = v.to_string();
        }
    }
    spec
}

#[allow(clippy::too_many_lines)]
fn build_tools_window(parent: &libadwaita::ApplicationWindow, ctx: &Rc<AppCtx>) {
    let win = gtk::Window::builder()
        .title("OpsTerm Tools")
        .transient_for(parent)
        .default_width(760)
        .default_height(560)
        .build();
    let nb = gtk::Notebook::new();

    // ---- Commands --------------------------------------------------------
    {
        let page = gtk::Box::new(gtk::Orientation::Vertical, 6);
        page.set_margin_top(8);
        page.set_margin_bottom(8);
        page.set_margin_start(8);
        page.set_margin_end(8);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&list)
            .build();
        let name = gtk::Entry::new();
        name.set_placeholder_text(Some("Command name"));
        let content = gtk::Entry::new();
        content.set_placeholder_text(Some("Command content"));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.append(&name);
        row.append(&content);
        let save = gtk::Button::with_label("Save");
        save.add_css_class("suggested-action");
        let del = gtk::Button::with_label("Delete");
        row.append(&save);
        row.append(&del);
        page.append(&scroll);
        page.append(&row);

        let reload = {
            let list = list.clone();
            let ctx = ctx.clone();
            move || {
                while let Some(r) = list.row_at_index(0) {
                    list.remove(&r);
                }
                let insert_mode = std::rc::Rc::new(std::cell::Cell::new(true));
                for c in ctx.db.list_commands().unwrap_or_default() {
                    let r = gtk::ListBoxRow::new();
                    let l = gtk::Label::new(Some(&format!("{}  —  {}", c.name, c.content)));
                    l.set_xalign(0.0);
                    r.set_child(Some(&l));
                    r.set_widget_name(&c.id);
                    list.append(&r);
                }
                let _ = insert_mode;
            }
        };
        reload();
        let reload = std::rc::Rc::new(reload);

        {
            list.connect_row_selected({
                let name = name.clone();
                let content = content.clone();
                move |_, row| {
                    if let Some(row) = row {
                        if let Some(l) = row.child().and_downcast::<gtk::Label>() {
                            let text = l.text().to_string();
                            if let Some((n, c)) = text.split_once("  —  ") {
                                name.set_text(n);
                                content.set_text(c);
                            }
                        }
                    }
                }
            });
        }
        {
            let ctx2 = ctx.clone();
            let reload2 = reload.clone();
            save.connect_clicked({
                let name = name.clone();
                let content = content.clone();
                let list = list.clone();
                move |_| {
                    let _ = &ctx2;
                    let input = opsterm_core::domain::CommandInput {
                        id: list
                            .selected_row()
                            .map(|row| row.widget_name().to_string())
                            .filter(|s| !s.is_empty()),
                        group_id: None,
                        name: name.text().to_string(),
                        content: content.text().to_string(),
                        vendor_profile: None,
                        execute_mode: "insert".into(),
                        risk_level: "low".into(),
                        variables: serde_json::json!({}),
                        tags: vec![],
                    };
                    let _ = ctx2.db.upsert_command(&input);
                    reload2();
                }
            });
            let reload3 = reload.clone();
            let ctx3 = ctx.clone();
            del.connect_clicked(move |_| {
                if let Some(row) = list.selected_row() {
                    let id = row.widget_name().to_string();
                    if !id.is_empty() {
                        let _ = ctx3.db.delete_command(&id);
                        reload3();
                    }
                }
            });
        }
        nb.append_page(&page, Some(&gtk::Label::new(Some("Commands"))));
    }

    // ---- Credentials -----------------------------------------------------
    {
        let page = gtk::Box::new(gtk::Orientation::Vertical, 6);
        page.set_margin_top(8);
        page.set_margin_bottom(8);
        page.set_margin_start(8);
        page.set_margin_end(8);
        let list = gtk::ListBox::new();
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&list)
            .build();
        let name = gtk::Entry::new();
        name.set_placeholder_text(Some("name / tag"));
        let user = gtk::Entry::new();
        user.set_placeholder_text(Some("username (optional)"));
        let tags = gtk::Entry::new();
        tags.set_placeholder_text(Some("tags (space)"));
        let secret = gtk::Entry::new();
        secret.set_visibility(false);
        secret.set_placeholder_text(Some("secret"));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.append(&name);
        row.append(&user);
        row.append(&tags);
        row.append(&secret);
        let store = gtk::Button::with_label("Store");
        store.add_css_class("suggested-action");
        let del = gtk::Button::with_label("Delete");
        row.append(&store);
        row.append(&del);
        page.append(&scroll);
        page.append(&row);
        let credential_status = gtk::Label::new(None);
        credential_status.set_xalign(0.0);
        credential_status.set_wrap(true);
        page.append(&credential_status);
        let reload = {
            let list = list.clone();
            let ctx = ctx.clone();
            move || {
                while let Some(r) = list.row_at_index(0) {
                    list.remove(&r);
                }
                for c in ctx.db.list_credentials().unwrap_or_default() {
                    let r = gtk::ListBoxRow::new();
                    let l = gtk::Label::new(Some(&format!(
                        "{}  ({})  {}",
                        c.name,
                        c.username.unwrap_or_else(|| "-".into()),
                        c.tags.join(" ")
                    )));
                    l.set_xalign(0.0);
                    r.set_child(Some(&l));
                    r.set_widget_name(&c.name);
                    list.append(&r);
                }
            }
        };
        reload();
        let reload = std::rc::Rc::new(reload);
        {
            let reload2 = reload.clone();
            store.connect_clicked({
                let name = name.clone();
                let user = user.clone();
                let tags = tags.clone();
                let secret = secret.clone();
                let ctx = ctx.clone();
                move |_| {
                    let n = name.text().to_string();
                    if n.is_empty() {
                        return;
                    }
                    if credential::store(&n, &secret.text()).is_err() {
                        credential_status.set_text("Could not save the password in the system keyring. The credential was not saved.");
                        return;
                    }
                    let t: Vec<String> = tags
                        .text()
                        .to_string()
                        .split_whitespace()
                        .map(String::from)
                        .collect();
                    let u = user.text().to_string();
                    let saved = ctx.db.upsert_credential(
                        &n,
                        if u.is_empty() { None } else { Some(&u) },
                        "password",
                        &t,
                    );
                    if saved.is_err() {
                        credential_status.set_text("Password stored, but credential metadata could not be saved. Try Store again.");
                        return;
                    }
                    secret.set_text("");
                    credential_status.set_text("Credential saved.");
                    reload2();
                }
            });
            let reload3 = reload.clone();
            let ctx3 = ctx.clone();
            del.connect_clicked(move |_| {
                if let Some(row) = list.selected_row() {
                    let id = row.widget_name().to_string();
                    if !id.is_empty() {
                        let _ = credential::remove(&id);
                        let _ = ctx3.db.remove_credential_name(&id);
                        reload3();
                    }
                }
            });
        }
        nb.append_page(&page, Some(&gtk::Label::new(Some("Credentials"))));
    }

    // ---- SFTP ------------------------------------------------------------
    {
        let page = gtk::Box::new(gtk::Orientation::Vertical, 6);
        page.set_margin_top(8);
        page.set_margin_bottom(8);
        page.set_margin_start(8);
        page.set_margin_end(8);
        let (status_tx, status_rx) = std::sync::mpsc::channel::<String>();
        let pick = gtk::DropDown::from_strings(&[]);
        let path = gtk::Entry::new();
        path.set_text("/");
        path.set_hexpand(true);
        let newname = gtk::Entry::new();
        newname.set_placeholder_text(Some("new folder"));
        let up = gtk::Button::with_label("Up");
        let browse = gtk::Button::with_label("Browse");
        let mkdir = gtk::Button::with_label("+ Folder");
        let download = gtk::Button::with_label("Download");
        let delete = gtk::Button::with_label("Delete");
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        top.append(&pick);
        top.append(&path);
        top.append(&up);
        top.append(&browse);
        top.append(&newname);
        top.append(&mkdir);
        top.append(&download);
        top.append(&delete);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&list)
            .build();
        let status = gtk::Label::new(Some(""));
        status.set_xalign(0.0);
        page.append(&top);
        page.append(&scroll);
        page.append(&status);

        let ssh_sessions: Vec<SavedSession> = ctx
            .sessions
            .borrow()
            .iter()
            .filter(|s| s.protocol == "ssh")
            .cloned()
            .collect();
        let names: Vec<&str> = ssh_sessions.iter().map(|s| s.name.as_str()).collect();
        pick.set_model(Some(&gtk::StringList::new(&names)));

        let sftp_sessions = std::rc::Rc::new(ssh_sessions);

        let load = std::rc::Rc::new({
            let list = list.clone();
            let path = path.clone();
            let status = status.clone();
            let sessions = sftp_sessions.clone();
            let pick = pick.clone();
            move || {
                let Some(spec) = sftp_spec(&sessions, &pick) else {
                    return;
                };
                let target = path.text().to_string();
                match opsterm_core::sftp::list(&spec, &target) {
                    Ok(entries) => {
                        while let Some(r) = list.row_at_index(0) {
                            list.remove(&r);
                        }
                        let count = entries.len();
                        for e in entries {
                            let r = gtk::ListBoxRow::new();
                            let l = gtk::Label::new(Some(&format!(
                                "{} {}",
                                if e.is_dir { "📁" } else { "📄" },
                                e.name
                            )));
                            l.set_xalign(0.0);
                            r.set_child(Some(&l));
                            r.set_widget_name(&e.path);
                            r.set_tooltip_text(Some(if e.is_dir { "dir" } else { "file" }));
                            list.append(&r);
                        }
                        status.set_text(&format!("{count} entries"));
                    }
                    Err(e) => status.set_text(&format!("Error: {e}")),
                }
            }
        });

        {
            let load = load.clone();
            browse.connect_clicked(move |_| load());
        }
        {
            let load = load.clone();
            let path = path.clone();
            up.connect_clicked(move |_| {
                let p = path.text().to_string();
                let trimmed = p.trim_end_matches('/');
                path.set_text(&match trimmed.rfind('/') {
                    Some(0) | None => "/".to_string(),
                    Some(i) => trimmed[..i].to_string(),
                });
                load();
            });
        }
        {
            let load = load.clone();
            let path = path.clone();
            list.connect_row_activated(move |_, row| {
                if row.tooltip_text().as_deref() == Some("dir") {
                    path.set_text(row.widget_name().as_ref());
                    load();
                }
            });
        }
        {
            let load = load.clone();
            let status = status.clone();
            let path = path.clone();
            let newname = newname.clone();
            let sessions = sftp_sessions.clone();
            let pick = pick.clone();
            mkdir.connect_clicked(move |_| {
                let Some(spec) = sftp_spec(&sessions, &pick) else {
                    return;
                };
                let name = newname.text().to_string();
                if name.is_empty() {
                    status.set_text("Enter a folder name");
                    return;
                }
                let target = format!("{}/{}", path.text().to_string().trim_end_matches('/'), name);
                match opsterm_core::sftp::mkdir(&spec, &target) {
                    Ok(()) => {
                        status.set_text("Folder created");
                        load();
                    }
                    Err(e) => status.set_text(&format!("Error: {e}")),
                }
            });
        }
        {
            let load = load.clone();
            let status = status.clone();
            let list = list.clone();
            let sessions = sftp_sessions.clone();
            let pick = pick.clone();
            delete.connect_clicked(move |_| {
                let Some(spec) = sftp_spec(&sessions, &pick) else {
                    return;
                };
                let Some(row) = list.selected_row() else {
                    return;
                };
                let p = row.widget_name().to_string();
                let is_dir = row.tooltip_text().as_deref() == Some("dir");
                match opsterm_core::sftp::remove(&spec, &p, is_dir) {
                    Ok(()) => {
                        status.set_text("Deleted");
                        load();
                    }
                    Err(e) => status.set_text(&format!("Error: {e}")),
                }
            });
        }
        {
            let status = status.clone();
            let list = list.clone();
            let tx = status_tx.clone();
            let sessions = sftp_sessions.clone();
            let pick = pick.clone();
            download.connect_clicked(move |_| {
                let Some(spec) = sftp_spec(&sessions, &pick) else {
                    return;
                };
                let Some(row) = list.selected_row() else {
                    return;
                };
                let remote = row.widget_name().to_string();
                let name = remote.rsplit('/').next().unwrap_or("file").to_string();
                let local = format!("/tmp/{name}");
                match security::build_scp_argv(&spec, false, &[remote, local.clone()]) {
                    Ok(argv) => {
                        status.set_text(&format!("Downloading → {local}…"));
                        let tx = tx.clone();
                        std::thread::spawn(move || {
                            let ok = std::process::Command::new(&argv[0])
                                .args(&argv[1..])
                                .status()
                                .map(|s| s.success())
                                .unwrap_or(false);
                            let _ = tx.send(if ok {
                                format!("Saved {local}")
                            } else {
                                "Download failed".to_string()
                            });
                        });
                    }
                    Err(e) => status.set_text(&format!("Error: {e}")),
                }
            });
        }
        {
            let status = status.clone();
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
                while let Ok(msg) = status_rx.try_recv() {
                    status.set_text(&msg);
                }
                gtk::glib::ControlFlow::Continue
            });
        }
        nb.append_page(&page, Some(&gtk::Label::new(Some("Files (SFTP)"))));
    }

    win.set_child(Some(&nb));
    win.present();
}

fn main() -> gtk::glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4 && args[1] == "--relay" {
        std::process::exit(run_relay(&args[2], &args[3]));
    }
    let app = libadwaita::Application::builder()
        .application_id(APP_ID)
        .build();
    app.connect_startup(|_| install_theme());
    app.connect_activate(build_ui);
    app.run()
}
