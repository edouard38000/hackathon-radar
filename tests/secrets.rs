//! The secret store: environment variables over the keychain over the
//! owner-only file. A fake keychain stands in for the real one; the file
//! fallback is a real file in a temporary directory, with real permissions.

mod support;

use std::path::Path;

use hackathon_radar::secrets::{
    FileState, Found, Keychain, Origin, Probe, Secret, SecretName, SecretsError, Stored,
};
use support::*;

const KEY: &str = "serp_test_key_0123456789";

fn secret(value: &str) -> Secret {
    Secret::new(value).unwrap()
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[cfg(unix)]
fn chmod(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn the_secrets_keep_their_names_and_environment_variables() {
    let names: Vec<(&str, &str)> = SecretName::ALL
        .iter()
        .map(|n| (n.account(), n.env_var()))
        .collect();
    assert_eq!(
        names,
        [
            ("ntfy-token", "HACKATHON_RADAR_NTFY_TOKEN"),
            ("photon-project-id", "PHOTON_PROJECT_ID"),
            ("photon-project-secret", "PHOTON_PROJECT_SECRET"),
            ("serpapi-api-key", "SERPAPI_API_KEY"),
        ]
    );
    assert_eq!(hackathon_radar::secrets::SERVICE, "hackathon-radar");
}

#[test]
fn a_stored_secret_is_read_back_from_the_keychain() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = FakeKeychain::default();
    let secrets = secrets_in(&keychain, dir.path(), &[]);

    assert_eq!(secrets.get(SecretName::SerpApiKey).unwrap(), None);
    let stored = secrets.store(SecretName::SerpApiKey, &secret(KEY)).unwrap();

    assert_eq!(stored, Stored::Keychain);
    assert_eq!(keychain.get("serpapi-api-key").as_deref(), Some(KEY));
    assert!(!secrets.file().exists(), "nothing goes to the file");
    assert_eq!(
        secrets.find(SecretName::SerpApiKey).unwrap(),
        Some(Found {
            secret: secret(KEY),
            origin: Origin::Keychain,
        })
    );
}

#[test]
fn an_environment_variable_overrides_the_keychain_and_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = FakeKeychain::default().with("serpapi-api-key", "from_keychain");
    let secrets = secrets_in(&keychain, dir.path(), &[("SERPAPI_API_KEY", " from_env ")]);
    let found = secrets.find(SecretName::SerpApiKey).unwrap().unwrap();
    assert_eq!(found.secret.expose(), "from_env");
    assert_eq!(found.origin, Origin::Env("SERPAPI_API_KEY"));

    // An empty variable counts as unset.
    let secrets = secrets_in(&keychain, dir.path(), &[("SERPAPI_API_KEY", "  ")]);
    let found = secrets.find(SecretName::SerpApiKey).unwrap().unwrap();
    assert_eq!(found.secret.expose(), "from_keychain");
    assert_eq!(found.origin, Origin::Keychain);
}

#[cfg(unix)]
#[test]
fn without_a_keychain_secrets_go_to_an_owner_only_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    let keychain = FakeKeychain::unavailable();
    let secrets = secrets_in(&keychain, &config, &[]);

    let stored = secrets
        .store(SecretName::PhotonProjectSecret, &secret("photon_secret_1"))
        .unwrap();
    secrets
        .store(SecretName::PhotonProjectId, &secret("proj_1"))
        .unwrap();

    let path = config.join("secrets.toml");
    assert_eq!(
        stored,
        Stored::File {
            path: path.clone(),
            why: "no keychain on this machine".into(),
        }
    );
    assert!(
        stored.to_string().contains("readable only by you"),
        "{stored}"
    );
    assert_eq!(mode(&path), 0o600);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("photon-project-secret = \"photon_secret_1\""),
        "{text}"
    );
    assert!(text.contains("photon-project-id = \"proj_1\""), "{text}");
    let found = secrets
        .find(SecretName::PhotonProjectSecret)
        .unwrap()
        .unwrap();
    assert_eq!(found.secret.expose(), "photon_secret_1");
    assert_eq!(found.origin, Origin::File(path));
}

#[cfg(unix)]
#[test]
fn a_file_others_can_read_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = FakeKeychain::unavailable();
    let secrets = secrets_in(&keychain, dir.path(), &[]);
    secrets
        .store(SecretName::NtfyToken, &secret("tk_fixture"))
        .unwrap();
    chmod(secrets.file(), 0o644);

    let error = secrets.get(SecretName::NtfyToken).unwrap_err();

    assert!(
        matches!(error, SecretsError::FileTooOpen { mode: 0o644, .. }),
        "{error}"
    );
    let message = error.to_string();
    assert!(message.contains("permissions are 0644"), "{message}");
    assert!(message.contains("chmod 600"), "{message}");
    assert!(!message.contains("tk_fixture"), "{message}");
    // Writing refuses too, rather than widening what others can read.
    assert!(secrets.store(SecretName::SerpApiKey, &secret(KEY)).is_err());

    chmod(secrets.file(), 0o640);
    assert!(
        secrets.get(SecretName::NtfyToken).is_err(),
        "group access is too wide"
    );
    chmod(secrets.file(), 0o600);
    assert_eq!(
        secrets.get(SecretName::NtfyToken).unwrap(),
        Some(secret("tk_fixture"))
    );
}

#[test]
fn the_keychain_wins_over_the_file_and_storing_there_removes_the_file_copy() {
    let dir = tempfile::tempdir().unwrap();
    // First stored with no keychain, as on a headless machine…
    let secrets = secrets_in(&FakeKeychain::unavailable(), dir.path(), &[]);
    secrets
        .store(SecretName::SerpApiKey, &secret("old_key"))
        .unwrap();
    secrets
        .store(SecretName::NtfyToken, &secret("tk_file"))
        .unwrap();

    // …then once a keychain is there, it holds the new value and the file
    // keeps only the other secret.
    let keychain = FakeKeychain::default();
    let secrets = secrets_in(&keychain, dir.path(), &[]);
    assert_eq!(
        secrets
            .find(SecretName::SerpApiKey)
            .unwrap()
            .unwrap()
            .origin,
        Origin::File(dir.path().join("secrets.toml"))
    );
    secrets
        .store(SecretName::SerpApiKey, &secret("new_key"))
        .unwrap();
    let text = std::fs::read_to_string(secrets.file()).unwrap();
    assert!(!text.contains("old_key"), "{text}");
    assert!(text.contains("tk_file"), "{text}");
    assert_eq!(
        secrets.find(SecretName::SerpApiKey).unwrap().unwrap(),
        Found {
            secret: secret("new_key"),
            origin: Origin::Keychain,
        }
    );
    assert_eq!(
        secrets.get(SecretName::NtfyToken).unwrap(),
        Some(secret("tk_file"))
    );
}

#[test]
fn a_broken_file_is_reported_without_its_contents() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_in(&FakeKeychain::unavailable(), dir.path(), &[]);
    std::fs::write(secrets.file(), "serpapi-api-key = \"leaky_key_42\n").unwrap();
    #[cfg(unix)]
    chmod(secrets.file(), 0o600);

    let error = secrets.get(SecretName::SerpApiKey).unwrap_err().to_string();

    assert!(error.contains("is not a valid secrets file"), "{error}");
    assert!(!error.contains("leaky_key_42"), "{error}");
}

#[test]
fn debug_output_never_shows_a_secret() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = FakeKeychain::default().with("serpapi-api-key", KEY);
    let secrets = secrets_in(&keychain, dir.path(), &[]);
    let found = secrets.find(SecretName::SerpApiKey).unwrap().unwrap();

    for debug in [
        format!("{:?}", secret(KEY)),
        format!("{found:?}"),
        format!("{:?}", Some(secret(KEY))),
    ] {
        assert!(!debug.contains(KEY), "{debug}");
        assert!(debug.contains("[redacted]"), "{debug}");
    }
}

#[test]
fn a_keychain_probe_says_whether_an_item_is_there_without_its_value() {
    let keychain = FakeKeychain::default().with("serpapi-api-key", KEY);
    assert_eq!(keychain.probe("serpapi-api-key"), Probe::Found);
    assert_eq!(keychain.probe("ntfy-token"), Probe::Missing);
    assert_eq!(
        FakeKeychain::unavailable().probe("serpapi-api-key"),
        Probe::Unavailable("no keychain on this machine".into())
    );
}

#[test]
fn a_diagnosis_checks_every_place_on_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = FakeKeychain::default().with("serpapi-api-key", KEY);
    let secrets = secrets_in(&keychain, dir.path(), &[("SERPAPI_API_KEY", "from_env")]);
    assert_eq!(secrets.file_state(), FileState::Missing);
    secrets
        .store(SecretName::NtfyToken, &secret("tk_x"))
        .unwrap();
    let diagnosis = secrets.diagnose(SecretName::SerpApiKey);

    assert!(diagnosis.in_env);
    assert_eq!(diagnosis.keychain, Probe::Found);
    assert!(!diagnosis.in_file.unwrap());

    let secrets = secrets_in(&FakeKeychain::unavailable(), dir.path(), &[]);
    secrets.store(SecretName::SerpApiKey, &secret(KEY)).unwrap();
    let diagnosis = secrets.diagnose(SecretName::SerpApiKey);
    assert!(!diagnosis.in_env);
    assert!(matches!(diagnosis.keychain, Probe::Unavailable(_)));
    assert!(diagnosis.in_file.unwrap());
    assert!(matches!(
        secrets.file_state(),
        FileState::Present { secrets: 1, .. }
    ));
}

#[cfg(unix)]
#[test]
fn a_file_others_can_read_is_reported_as_too_open() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = secrets_in(&FakeKeychain::unavailable(), dir.path(), &[]);
    secrets.store(SecretName::SerpApiKey, &secret(KEY)).unwrap();
    assert_eq!(
        secrets.file_state(),
        FileState::Present {
            mode: 0o600,
            secrets: 1
        }
    );
    chmod(secrets.file(), 0o640);
    assert_eq!(secrets.file_state(), FileState::TooOpen { mode: 0o640 });
    assert!(secrets.diagnose(SecretName::SerpApiKey).in_file.is_err());
}

#[test]
fn empty_secrets_are_not_secrets() {
    assert_eq!(Secret::new("   "), None);
    assert_eq!(Secret::new(" k ").unwrap().expose(), "k");
}
