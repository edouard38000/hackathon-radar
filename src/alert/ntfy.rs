//! The ntfy Alert channel: one POST of the Alert text to the Profile's topic.

use anyhow::Context;

use crate::alert::{Alert, AlertChannel};
use crate::http::{HttpPost, PostRequest};
use crate::profile::NtfySetting;

/// The environment variable that overrides the stored access token, for ntfy
/// servers that need one. The token is a secret, kept in the keychain
/// ([`crate::secrets`]), never in the Profile.
pub const TOKEN_ENV_VAR: &str = "HACKATHON_RADAR_NTFY_TOKEN";

pub struct NtfyChannel<H> {
    setting: NtfySetting,
    token: Option<String>,
    http: H,
}

impl<H: HttpPost> NtfyChannel<H> {
    pub fn new(setting: NtfySetting, token: Option<String>, http: H) -> Self {
        Self {
            setting,
            token,
            http,
        }
    }

    fn request(&self, alert: &Alert) -> PostRequest {
        let mut headers = vec![(
            "Content-Type".to_owned(),
            "text/plain; charset=utf-8".to_owned(),
        )];
        if let Some(token) = &self.token {
            headers.push(("Authorization".to_owned(), format!("Bearer {token}")));
        }
        PostRequest {
            url: format!("{}/{}", self.setting.server, self.setting.topic),
            headers,
            body: alert.text.clone(),
        }
    }
}

impl<H: HttpPost> AlertChannel for NtfyChannel<H> {
    fn send(&mut self, alert: &Alert) -> anyhow::Result<()> {
        // Names the server but not the topic or token, which work like passwords.
        self.http
            .post(&self.request(alert))
            .with_context(|| format!("sending the Alert to ntfy at {}", self.setting.server))
    }
}
