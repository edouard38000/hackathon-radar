//! The ntfy Alert channel: one POST of the Alert text to the Profile's topic.

use anyhow::Context;

use crate::alert::{Alert, AlertChannel};
use crate::http::{HttpPost, PostRequest};
use crate::profile::NtfySetting;

/// Where an access token is read from, for ntfy servers that need one. The
/// Profile never holds it, since the Profile has no secrets.
pub const TOKEN_ENV_VAR: &str = "HACKATHON_RADAR_NTFY_TOKEN";

/// The access token from [`TOKEN_ENV_VAR`], if it is set and not empty.
pub fn token_from_env() -> Option<String> {
    std::env::var(TOKEN_ENV_VAR)
        .ok()
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

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
