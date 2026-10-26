//! Fixture-only helper for real cross-process secure-store checks.
//! No password is accepted in argv, environment or files.
use opsterm_core::credential;
const FIXTURE: &str = "fixture-password";
fn main() {
    let mode = std::env::args().nth(1).expect("fixture operation");
    let name = std::env::args().nth(2).expect("unique fixture reference");
    assert!(
        name.starts_with("opsterm-smoke-") && name.len() > 20,
        "this helper only operates on unique smoke-test entries"
    );
    let legacy = || {
        keyring::keyutils::KeyutilsCredential::new_with_target(None, "org.opsterm.app", &name)
            .map(|value| keyring::Entry::new_with_credential(Box::new(value)))
            .expect("kernel fixture entry")
    };
    match mode.as_str() {
        "set" => credential::set(&name, FIXTURE).expect("durable fixture save"),
        "get" => assert!(
            credential::get(&name)
                .map(|value| value == FIXTURE)
                .unwrap_or(false),
            "fixture secret missing or changed"
        ),
        "durable-get" => {
            let entry = keyring::Entry::new("org.opsterm.app", &name).unwrap();
            assert!(entry
                .get_credential()
                .is::<keyring::secret_service::SsCredential>());
            assert!(entry
                .get_password()
                .map(|value| value == FIXTURE)
                .unwrap_or(false));
        }
        "missing" => assert!(credential::get(&name).is_err()),
        "kernel-set" => legacy().set_password(FIXTURE).expect("legacy fixture save"),
        "kernel-delete" => match legacy().delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => panic!("kernel fixture cleanup failed"),
        },
        "delete" => credential::delete(&name).expect("fixture cleanup"),
        _ => panic!("unknown fixture operation"),
    }
    println!("fixture operation succeeded");
}
