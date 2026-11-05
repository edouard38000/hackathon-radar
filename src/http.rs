use std::time::Duration;

use anyhow::Context;

/// Sent with every request. It names the tool and nothing about the person running it.
pub const USER_AGENT: &str = concat!(
    "hackathon-radar/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/edouard38000/hackathon-radar)"
);

/// Fetches a page as text. Sources take one so tests can serve saved fixtures instead.
pub trait HttpGet {
    fn get_text(&self, url: &str) -> anyhow::Result<String>;
}

impl<T: HttpGet + ?Sized> HttpGet for &T {
    fn get_text(&self, url: &str) -> anyhow::Result<String> {
        (**self).get_text(url)
    }
}

/// The real HTTP client.
pub struct HttpClient {
    agent: ureq::Agent,
}

impl HttpClient {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self { agent }
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpGet for HttpClient {
    fn get_text(&self, url: &str) -> anyhow::Result<String> {
        self.agent
            .get(url)
            .call()
            .and_then(|mut response| response.body_mut().read_to_string())
            .with_context(|| format!("fetching {url}"))
    }
}
