//! Builds what a Run (and a test Alert) talks to from the Profile and the
//! secret store: the Alert channel and the Fare lookup. Their secrets come
//! from [`Secrets`], never from the Profile, so scheduled Runs, which don't
//! see the person's shell environment, still have them.

use crate::alert::ntfy::NtfyChannel;
use crate::alert::photon::{Credentials, PhotonChannel, Sidecar};
use crate::alert::{Alert, AlertChannel, PrintChannel};
use crate::fares::serpapi::SerpApi;
use crate::http::{HttpGet, HttpPost};
use crate::process::ProcessRunner;
use crate::profile::{AlertChannelSetting, Profile};
use crate::secrets::{SecretName, Secrets};

/// What a test Alert says.
pub const TEST_ALERT: &str =
    "hackathon-radar: this is a test Alert. If you can read it, your Alerts reach you.";

/// The Alert channel the Profile names, or printing when it names none.
///
/// Photon credentials are checked here, before a live Run, so missing ones
/// don't waste the Agent's work; a dry run never sends, so it doesn't need them.
pub fn alert_channel<'a, H, R>(
    profile: &Profile,
    secrets: &Secrets,
    dry_run: bool,
    http: H,
    runner: R,
    sidecar: Sidecar,
) -> anyhow::Result<Box<dyn AlertChannel + 'a>>
where
    H: HttpPost + 'a,
    R: ProcessRunner + 'a,
{
    Ok(match &profile.alert_channel {
        Some(AlertChannelSetting::Ntfy(setting)) => {
            let token = secrets
                .get(SecretName::NtfyToken)?
                .map(|t| t.expose().to_owned());
            Box::new(NtfyChannel::new(setting.clone(), token, http))
        }
        Some(AlertChannelSetting::Photon(setting)) => {
            let credentials = photon_credentials(secrets)?;
            if let (false, Err(why)) = (dry_run, &credentials) {
                anyhow::bail!("{why}");
            }
            Box::new(PhotonChannel::new(
                setting.clone(),
                credentials,
                sidecar,
                runner,
            ))
        }
        None => Box::new(PrintChannel),
    })
}

/// The Photon project credentials from the secret store, or why they are
/// missing. Only a store that can't be read is an error.
pub fn photon_credentials(secrets: &Secrets) -> anyhow::Result<Result<Credentials, String>> {
    let id = secrets.get(SecretName::PhotonProjectId)?;
    let secret = secrets.get(SecretName::PhotonProjectSecret)?;
    Ok(Credentials::from_vars(
        id.map(|s| s.expose().to_owned()),
        secret.map(|s| s.expose().to_owned()),
    )
    .map_err(|e| e.to_string()))
}

/// Fares through SerpApi on the person's key, or `None` without one, when the
/// Agent estimates Unverified fares instead.
pub fn fare_lookup<H: HttpGet>(secrets: &Secrets, http: H) -> anyhow::Result<Option<SerpApi<H>>> {
    Ok(secrets
        .get(SecretName::SerpApiKey)?
        .map(|key| SerpApi::new(http, key.expose())))
}

/// Sends one short test Alert through `channel`. Printing doesn't count as
/// reaching the person, so a Profile with no Alert channel is an error.
pub fn send_test_alert(channel: &mut dyn AlertChannel) -> anyhow::Result<()> {
    if !channel.reaches_the_person() {
        anyhow::bail!(
            "the Profile has no Alert channel, so a test Alert would only be printed; \
             run `hackathon-radar init` to set one up"
        );
    }
    channel.send(&Alert {
        text: TEST_ALERT.to_owned(),
    })
}

/// The Alert channel's name, for messages: "ntfy", "iMessage through Photon".
pub fn channel_name(profile: &Profile) -> &'static str {
    match profile.alert_channel {
        Some(AlertChannelSetting::Ntfy(_)) => "ntfy",
        Some(AlertChannelSetting::Photon(_)) => "iMessage through Photon",
        None => "no Alert channel",
    }
}
