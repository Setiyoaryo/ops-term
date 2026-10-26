//! Storage layer integration tests: migrations, CRUD and settings round-trips.

use opsterm_lib::domain::{
    CommandGroupInput, CommandInput, FolderInput, SavedSessionInput, WorkspaceInput,
};
use opsterm_lib::storage::Database;

#[test]
fn folder_and_session_lifecycle() {
    let db = Database::open_in_memory().expect("open db");

    let folder = db
        .upsert_folder(&FolderInput {
            id: None,
            parent_id: None,
            name: "Production".into(),
            sort_order: Some(1),
        })
        .expect("folder");

    let session = db
        .upsert_session(&SavedSessionInput {
            id: None,
            folder_id: Some(folder.id.clone()),
            name: "CORE-JKT-01".into(),
            host: "192.0.2.10".into(),
            port: 22,
            protocol: "ssh".into(),
            username: Some("operator".into()),
            ssh_alias: None,
            identity_file: None,
            proxy_jump: None,
            tags: vec!["cisco".into(), "core".into()],
            description: None,
            notes: None,
            settings_override: None,
            credential_ref: None,
            is_favorite: true,
            sort_order: None,
        })
        .expect("session");

    assert_eq!(session.name, "CORE-JKT-01");
    assert_eq!(session.tags.len(), 2);

    // Update keeps created_at, changes fields.
    let updated = db
        .upsert_session(&SavedSessionInput {
            id: Some(session.id.clone()),
            folder_id: Some(folder.id.clone()),
            name: "CORE-JKT-02".into(),
            host: "192.0.2.11".into(),
            port: 2222,
            protocol: "ssh".into(),
            username: Some("operator".into()),
            ssh_alias: None,
            identity_file: None,
            proxy_jump: None,
            tags: vec![],
            description: Some("moved".into()),
            notes: None,
            settings_override: None,
            credential_ref: None,
            is_favorite: false,
            sort_order: None,
        })
        .expect("update");
    assert_eq!(updated.created_at, session.created_at);
    assert_eq!(updated.host, "192.0.2.11");
    assert_eq!(updated.port, 2222);

    assert_eq!(db.list_sessions().unwrap().len(), 1);
    assert_eq!(db.list_folders().unwrap().len(), 1);

    db.delete_session(&session.id).unwrap();
    assert!(db.list_sessions().unwrap().is_empty());
}

#[test]
fn rejects_host_that_looks_like_an_option() {
    let db = Database::open_in_memory().expect("open db");
    let result = db.upsert_session(&SavedSessionInput {
        id: None,
        folder_id: None,
        name: "evil".into(),
        host: "-oProxyCommand=touch /tmp/pwned".into(),
        port: 22,
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
    });
    assert!(result.is_err(), "option-like host must be rejected");
}

#[test]
fn settings_roundtrip_and_validation() {
    let db = Database::open_in_memory().expect("open db");
    let mut settings = db.load_settings().expect("defaults");
    assert_eq!(settings.terminal.font_size, 13);

    settings.terminal.font_size = 18;
    settings.appearance.ui_theme = "light".into();
    db.save_settings(&settings).expect("save");
    let loaded = db.load_settings().expect("reload");
    assert_eq!(loaded.terminal.font_size, 18);
    assert_eq!(loaded.appearance.ui_theme, "light");

    settings.terminal.font_size = 99;
    assert!(db.save_settings(&settings).is_err());
}

#[test]
fn commands_and_workspaces() {
    let db = Database::open_in_memory().expect("open db");
    let group = db
        .upsert_command_group(&CommandGroupInput {
            id: None,
            parent_id: None,
            name: "Routing".into(),
            sort_order: None,
        })
        .expect("group");

    let command = db
        .upsert_command(&CommandInput {
            id: None,
            group_id: Some(group.id.clone()),
            name: "BGP summary".into(),
            content: "show ip bgp summary".into(),
            vendor_profile: Some("cisco".into()),
            execute_mode: "insert".into(),
            risk_level: "low".into(),
            variables: serde_json::json!({}),
            tags: vec!["bgp".into()],
        })
        .expect("command");
    assert_eq!(command.content, "show ip bgp summary");
    assert_eq!(db.list_commands().unwrap().len(), 1);

    let ws = db
        .upsert_workspace(&WorkspaceInput {
            id: None,
            name: "NOC".into(),
            description: None,
            layout: serde_json::json!({ "tabs": [] }),
        })
        .expect("workspace");
    assert_eq!(ws.name, "NOC");
    assert_eq!(db.list_workspaces().unwrap().len(), 1);
}
