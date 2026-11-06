//! Secrets: the keys and tokens hackathon-radar uses, kept out of the Profile.
//!
//! Each secret is read from, in order:
//!
//! 1. its environment variable (such as `SERPAPI_API_KEY`), when it is set and
//!    not empty, for CI and power users;
//! 2. the OS keychain, under the service [`SERVICE`]: the macOS Keychain, or
//!    the Secret Service on Linux;
//! 3. an owner-only file, `secrets.toml` in the config directory, which is
//!    where secrets go when there is no keychain (headless Linux). It is
//!    refused if anyone but its owner can read or write it.
//!
//! `hackathon-radar init` stores secrets in the keychain, or in the file when
//! the keychain can't be reached. Scheduled Runs don't see the person's shell
//! environment, which is why the keychain, not an environment variable, is
//! where secrets normally live.
//!
//! A [`Secret`] never shows its value in `Debug`, and has no `Display`; code
//! that needs the value asks for it with [`Secret::expose`].

use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::alert::{ntfy, photon};
use crate::fares::serpapi;

/// The keychain service every secret is stored under.
pub const SERVICE: &str = "hackathon-radar";

/// The secrets hackathon-radar knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretName {
    /// An access token for an ntfy server that needs one.
    NtfyToken,
    /// The Photon project's ID.
    PhotonProjectId,
    /// The Photon project's secret.
    PhotonProjectSecret,
    /// The person's SerpApi key, for Fares from Google Flights.
    SerpApiKey,
}

impl SecretName {
    pub const ALL: [SecretName; 4] = [
        SecretName::NtfyToken,
        SecretName::PhotonProjectId,
        SecretName::PhotonProjectSecret,
        SecretName::SerpApiKey,
    ];

    /// The environment variable that overrides the stored secret.
    pub fn env_var(self) -> &'static str {
        match self {
            SecretName::NtfyToken => ntfy::TOKEN_ENV_VAR,
            SecretName::PhotonProjectId => photon::PROJECT_ID_ENV_VAR,
            SecretName::PhotonProjectSecret => photon::PROJECT_SECRET_ENV_VAR,
            SecretName::SerpApiKey => serpapi::KEY_VAR,
        }
    }

    /// The account it is stored under, in the keychain and in the file.
    pub fn account(self) -> &'static str {
        match self {
            SecretName::NtfyToken => "ntfy-token",
            SecretName::PhotonProjectId => "photon-project-id",
            SecretName::PhotonProjectSecret => "photon-project-secret",
            SecretName::SerpApiKey => "serpapi-api-key",
        }
    }

    /// What it is, for messages: "the SerpApi key".
    pub fn describe(self) -> &'static str {
        match self {
            SecretName::NtfyToken => "the ntfy access token",
            SecretName::PhotonProjectId => "the Photon project ID",
            SecretName::PhotonProjectSecret => "the Photon project secret",
            SecretName::SerpApiKey => "the SerpApi key",
        }
    }
}

/// A secret's value. `Debug` never shows it, and there is no `Display`, so it
/// can't end up in output or logs by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// The value, trimmed; `None` when it is empty.
    pub fn new(value: &str) -> Option<Self> {
        let value = value.trim();
        (!value.is_empty()).then(|| Secret(value.to_owned()))
    }

    /// The value itself, for the one place that sends it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

/// Where a secret was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// The environment variable with this name.
    Env(&'static str),
    /// The OS keychain.
    Keychain,
    /// The owner-only file at this path.
    File(PathBuf),
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::Env(var) => write!(f, "the environment variable {var}"),
            Origin::Keychain => f.write_str(keychain_name()),
            Origin::File(path) => write!(f, "{}", path.display()),
        }
    }
}

/// A secret and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub secret: Secret,
    pub origin: Origin,
}

/// Where [`Secrets::store`] put a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    Keychain,
    /// The keychain couldn't be reached (for this reason), so the owner-only
    /// file at this path holds it.
    File {
        path: PathBuf,
        why: String,
    },
}

impl fmt::Display for Stored {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stored::Keychain => f.write_str(keychain_name()),
            Stored::File { path, .. } => write!(f, "{} (readable only by you)", path.display()),
        }
    }
}

/// The OS keychain's name, for messages.
pub fn keychain_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "the macOS Keychain"
    } else {
        "your keychain (the Secret Service)"
    }
}

/// Why the keychain couldn't be used: there is none (headless Linux), it is
/// locked, or access was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable(pub String);

/// The OS keychain, behind a trait so tests use a fake and never touch the real one.
pub trait Keychain {
    /// The secret stored under `account`, or `None` when there is none.
    fn get(&self, account: &str) -> Result<Option<String>, Unavailable>;
    fn set(&self, account: &str, value: &str) -> Result<(), Unavailable>;
}

/// The real keychain, through the `keyring` crate: the macOS Keychain, or the
/// Secret Service on Linux.
pub struct OsKeychain;

impl Keychain for OsKeychain {
    fn get(&self, account: &str) -> Result<Option<String>, Unavailable> {
        let entry = keyring::Entry::new(SERVICE, account).map_err(unavailable)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(unavailable(error)),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), Unavailable> {
        keyring::Entry::new(SERVICE, account)
            .and_then(|entry| entry.set_password(value))
            .map_err(unavailable)
    }
}

fn unavailable(error: keyring::Error) -> Unavailable {
    Unavailable(error.to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    #[error(
        "refusing to read {path}: its permissions are {mode:04o}, so others can read or change \
         it. Run `chmod 600 {path}` to make it readable only by you."
    )]
    FileTooOpen { path: PathBuf, mode: u32 },
    #[error("could not read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{path} is not a valid secrets file: {message}")]
    Syntax { path: PathBuf, message: String },
}

/// Reads an environment variable; tests pass a fake one.
pub type Env = Box<dyn Fn(&str) -> Option<String>>;

/// The secret store: environment variables over the keychain over the owner-only file.
pub struct Secrets {
    keychain: Box<dyn Keychain>,
    file: PathBuf,
    env: Env,
}

impl Secrets {
    pub fn new(keychain: Box<dyn Keychain>, file: PathBuf, env: Env) -> Self {
        Self {
            keychain,
            file,
            env,
        }
    }

    /// The real store: this process's environment, the OS keychain, and the
    /// file in the default config directory.
    pub fn system() -> anyhow::Result<Self> {
        Ok(Self::new(
            Box::new(OsKeychain),
            crate::paths::default_secrets_path()?,
            Box::new(|name| std::env::var(name).ok()),
        ))
    }

    /// The owner-only file secrets fall back to.
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// The secret and where it came from, or `None` when it isn't set anywhere.
    pub fn find(&self, name: SecretName) -> Result<Option<Found>, SecretsError> {
        if let Some(secret) = (self.env)(name.env_var()).as_deref().and_then(Secret::new) {
            return Ok(Some(Found {
                secret,
                origin: Origin::Env(name.env_var()),
            }));
        }
        // An unreachable keychain is the normal case on headless Linux, where
        // the file holds the secrets instead.
        if let Ok(Some(value)) = self.keychain.get(name.account())
            && let Some(secret) = Secret::new(&value)
        {
            return Ok(Some(Found {
                secret,
                origin: Origin::Keychain,
            }));
        }
        Ok(self
            .read_file()?
            .remove(name.account())
            .as_deref()
            .and_then(Secret::new)
            .map(|secret| Found {
                secret,
                origin: Origin::File(self.file.clone()),
            }))
    }

    /// The secret, or `None` when it isn't set anywhere.
    pub fn get(&self, name: SecretName) -> Result<Option<Secret>, SecretsError> {
        Ok(self.find(name)?.map(|found| found.secret))
    }

    /// Stores the secret in the keychain or, when the keychain can't be
    /// reached, in the owner-only file. A copy left in the file from before
    /// is removed once the keychain holds it, so there is only ever one.
    pub fn store(&self, name: SecretName, secret: &Secret) -> anyhow::Result<Stored> {
        match self.keychain.set(name.account(), secret.expose()) {
            Ok(()) => {
                let mut stored = self.read_file()?;
                if stored.remove(name.account()).is_some() {
                    self.write_file(&stored)?;
                }
                Ok(Stored::Keychain)
            }
            Err(Unavailable(why)) => {
                let mut stored = self.read_file()?;
                stored.insert(name.account().to_owned(), secret.expose().to_owned());
                self.write_file(&stored)?;
                Ok(Stored::File {
                    path: self.file.clone(),
                    why,
                })
            }
        }
    }

    /// The file's secrets by account; empty when there is no file.
    fn read_file(&self) -> Result<BTreeMap<String, String>, SecretsError> {
        let path = &self.file;
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(source) => {
                return Err(SecretsError::Read {
                    path: path.clone(),
                    source,
                });
            }
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode() & 0o7777;
            if mode & 0o077 != 0 {
                return Err(SecretsError::FileTooOpen {
                    path: path.clone(),
                    mode,
                });
            }
        }
        #[cfg(not(unix))]
        let _ = metadata;
        let text = std::fs::read_to_string(path).map_err(|source| SecretsError::Read {
            path: path.clone(),
            source,
        })?;
        // The parser's message can quote the file, so it is left out.
        toml::from_str(&text).map_err(|_| SecretsError::Syntax {
            path: path.clone(),
            message: "expected lines such as serpapi-api-key = \"...\"".to_owned(),
        })
    }

    /// Replaces the file with these secrets, readable and writable only by
    /// its owner from the moment it exists.
    fn write_file(&self, secrets: &BTreeMap<String, String>) -> anyhow::Result<()> {
        let path = &self.file;
        let dir = path
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)
            .with_context(|| format!("could not create {}", dir.display()))?;
        let mut text = String::from(
            "# hackathon-radar's secrets, kept here because no keychain could be reached.\n\
             # Readable only by you; never share or commit this file.\n",
        );
        text.push_str(&toml::to_string(secrets).context("could not write the secrets")?);
        // tempfile creates the file with mode 0600 on Unix.
        let mut file = tempfile::NamedTempFile::new_in(dir)
            .with_context(|| format!("could not write a file in {}", dir.display()))?;
        file.write_all(text.as_bytes())
            .with_context(|| format!("could not write {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("could not restrict {}", path.display()))?;
        }
        file.persist(path)
            .with_context(|| format!("could not write {}", path.display()))?;
        Ok(())
    }
}
