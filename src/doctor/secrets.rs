//! Secrets: each one the Profile needs can be read, and from where, without
//! ever reading its value out. The keychain is asked without prompting, so a
//! macOS item that a scheduled Run couldn't read (it would wait on a prompt)
//! shows up here; a locked or missing keyring, which reads quietly fall back
//! from, is said out loud, with the fallback file's permissions checked.

use super::{Check, Section};
use crate::fares::serpapi;
use crate::init::SERPAPI_SIGN_UP;
use crate::profile::{AlertChannelSetting, Profile};
use crate::secrets::{FileState, Probe, SecretName, Secrets, keychain_name};

const S: Section = Section::Secrets;

/// How much a secret matters to this Profile.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Need {
    /// Runs fail without it.
    Required,
    /// Runs work without it, but less well.
    Optional,
    /// Only checked when it is set somewhere.
    IfSet,
}

fn needed(profile: &Profile) -> Vec<(SecretName, Need)> {
    let mut needed = Vec::new();
    match profile.alert_channel {
        Some(AlertChannelSetting::Photon(_)) => {
            needed.push((SecretName::PhotonProjectId, Need::Required));
            needed.push((SecretName::PhotonProjectSecret, Need::Required));
        }
        Some(AlertChannelSetting::Ntfy(_)) => needed.push((SecretName::NtfyToken, Need::IfSet)),
        None => {}
    }
    if profile.travel.flight_budget.is_some() {
        needed.push((SecretName::SerpApiKey, Need::Optional));
    }
    needed
}

fn label(name: SecretName) -> &'static str {
    match name {
        SecretName::NtfyToken => "ntfy access token",
        SecretName::PhotonProjectId => "Photon project ID",
        SecretName::PhotonProjectSecret => "Photon project secret",
        SecretName::SerpApiKey => "SerpApi key",
    }
}

pub(super) fn check(
    profile: Option<&Profile>,
    secrets: &Secrets,
    env: &dyn Fn(&str) -> Option<String>,
    checks: &mut Vec<Check>,
) {
    let file = secrets.file().display().to_string();
    let needed = profile.map(needed).unwrap_or_default();
    let diagnoses: Vec<_> = needed
        .iter()
        .map(|(name, need)| (*name, *need, secrets.diagnose(*name)))
        .collect();

    // A keychain that can't be reached, which reads quietly fall back from.
    let unreachable = diagnoses.iter().find_map(|(_, _, d)| match &d.keychain {
        Probe::Unavailable(why) => Some(why.clone()),
        _ => None,
    });
    if let Some(why) = &unreachable {
        let fix = if cfg!(target_os = "macos") {
            "Unlock your login keychain in Keychain Access, then run doctor again.".to_owned()
        } else {
            format!(
                "Unlock your keyring by logging in to a desktop session, or keep secrets in \
                 {file}, which must stay readable only by you; on a headless machine the file \
                 is the place for them."
            )
        };
        checks.push(Check::warn(
            S,
            "secrets.keychain",
            "Keychain",
            format!(
                "{} couldn't be reached ({why}), so secrets are read from {file} instead.",
                capitalise(keychain_name())
            ),
            fix,
        ));
    }

    match secrets.file_state() {
        FileState::Missing if unreachable.is_some() => checks.push(Check::pass(
            S,
            "secrets.file",
            "Secrets file",
            format!("There is no {file} yet; the checks below say which secrets are missing."),
        )),
        FileState::Missing => {}
        FileState::Present { mode, secrets } => checks.push(Check::pass(
            S,
            "secrets.file",
            "Secrets file",
            format!(
                "{file} holds {secrets} {} and is readable only by you (mode {mode:04o}).",
                if secrets == 1 { "secret" } else { "secrets" }
            ),
        )),
        FileState::TooOpen { mode } => checks.push(Check::fail(
            S,
            "secrets.file",
            "Secrets file",
            format!(
                "{file} has mode {mode:04o}, so others can read or change it, and \
                 hackathon-radar refuses to read it."
            ),
            format!("Run `chmod 600 {file}`."),
        )),
        FileState::Invalid => checks.push(Check::fail(
            S,
            "secrets.file",
            "Secrets file",
            format!("{file} is not a valid secrets file."),
            format!(
                "Fix its lines (such as serpapi-api-key = \"...\"), or delete {file} and run \
                 `hackathon-radar init` to store the secrets again."
            ),
        )),
        FileState::Unreadable(why) => checks.push(Check::fail(
            S,
            "secrets.file",
            "Secrets file",
            format!("Could not read {file}: {why}."),
            format!("Make {file} readable by you, and only by you (`chmod 600 {file}`)."),
        )),
    }

    for (name, need, diagnosis) in diagnoses {
        let id = format!("secrets.{}", name.account());
        let label = label(name);
        let var = name.env_var();
        let in_file = matches!(diagnosis.in_file, Ok(true));
        let env_note = if diagnosis.in_env {
            format!(" In this shell, the environment variable {var} overrides it.")
        } else {
            String::new()
        };
        let check = match &diagnosis.keychain {
            Probe::Found => Some(Check::pass(
                S,
                id,
                label,
                format!("In {}.{env_note}", keychain_name()),
            )),
            Probe::NeedsApproval(_) if in_file => Some(Check::warn(
                S,
                id,
                label,
                format!(
                    "It is in {} but reading it needs your approval in a prompt (a rebuild \
                     changed this binary), so Runs use the copy in {file}.",
                    keychain_name()
                ),
                "Run `hackathon-radar init` again and choose Always Allow if macOS asks, so \
                 the keychain copy is readable again.",
            )),
            Probe::NeedsApproval(_) => Some(Check::fail(
                S,
                id,
                label,
                format!(
                    "It is in {} but reading it needs your approval in a prompt, as after a \
                     rebuild changed this binary. A scheduled Run can't answer the prompt, so it \
                     would see {} as missing.",
                    keychain_name(),
                    name.describe()
                ),
                "Run `hackathon-radar init` again (or store the secret again) and choose Always \
                 Allow if macOS asks, then run doctor again.",
            )),
            Probe::Missing | Probe::Unavailable(_) if in_file => {
                Some(Check::pass(S, id, label, format!("In {file}.{env_note}")))
            }
            Probe::Missing | Probe::Unavailable(_) if diagnosis.in_env => Some(Check::warn(
                S,
                id,
                label,
                format!(
                    "Only in the environment variable {var}. Scheduled Runs don't see your \
                     shell's environment, so they would see {} as missing.",
                    name.describe()
                ),
                "Store it with `hackathon-radar init` so scheduled Runs have it too.",
            )),
            Probe::Missing | Probe::Unavailable(_) => match (&diagnosis.in_file, need) {
                (Err(_), _) => Some(Check::fail(
                    S,
                    id,
                    label,
                    format!(
                        "Not in {} or the environment, and {file} can't be read, so Runs fail.",
                        keychain_name()
                    ),
                    "Fix the secrets file as the check above says.",
                )),
                (Ok(_), Need::Required) => Some(Check::fail(
                    S,
                    id,
                    label,
                    format!(
                        "Not set: not in {}, {file} or the environment variable {var}.",
                        keychain_name()
                    ),
                    format!(
                        "Run `hackathon-radar init` and give your Photon project's ID and secret \
                         (they're in the Photon dashboard, {}).",
                        crate::alert::photon::DASHBOARD
                    ),
                )),
                (Ok(_), Need::Optional) => Some(Check::warn(
                    S,
                    id,
                    label,
                    format!(
                        "No SerpApi key, so your Agent estimates Fares and they are marked \
                         unverified (it would be read from {}, {file} or {}).",
                        keychain_name(),
                        serpapi::KEY_VAR
                    ),
                    format!(
                        "Get a free key at {SERPAPI_SIGN_UP} and store it with `hackathon-radar \
                         init`."
                    ),
                )),
                (Ok(_), Need::IfSet) => None,
            },
        };
        checks.extend(check);
    }

    if let Some(dir) = env("XDG_CONFIG_HOME").filter(|d| !d.trim().is_empty()) {
        checks.push(Check::warn(
            S,
            "secrets.xdg-config-home",
            "XDG_CONFIG_HOME",
            format!(
                "XDG_CONFIG_HOME is set to {dir} in this shell, but scheduled Runs don't see \
                 it: they look for the secrets file under ~/.config instead."
            ),
            "Keep secrets in the keychain, which scheduled Runs read either way, or unset \
             XDG_CONFIG_HOME before running `hackathon-radar init`.",
        ));
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
