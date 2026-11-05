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

/// A POST with a text body, spelt out so a test can compare it with a recorded request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Sends a POST. Alert channels take one so tests can record the request instead.
pub trait HttpPost {
    fn post(&self, request: &PostRequest) -> anyhow::Result<()>;
}

impl<T: HttpPost + ?Sized> HttpPost for &T {
    fn post(&self, request: &PostRequest) -> anyhow::Result<()> {
        (**self).post(request)
    }
}

impl HttpPost for HttpClient {
    fn post(&self, request: &PostRequest) -> anyhow::Result<()> {
        let mut builder = self.agent.post(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        // The URL stays out of the error: for ntfy it holds the topic, which works like a password.
        builder.send(&request.body)?;
        Ok(())
    }
}
