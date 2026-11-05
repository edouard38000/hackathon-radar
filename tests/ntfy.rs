//! Seam 2: the ntfy Alert channel builds exactly the request in its recorded fixtures.

mod support;

use hackathon_radar::alert::ntfy::NtfyChannel;
use hackathon_radar::alert::{Alert, AlertChannel};
use hackathon_radar::profile::NtfySetting;
use support::*;

/// A recorded request; the editor's final newline is not part of the body.
fn recorded(name: &str) -> String {
    let text = fixture(name);
    text.strip_suffix('\n').unwrap_or(&text).to_owned()
}

fn ntfy(server: &str, topic: &str) -> NtfySetting {
    NtfySetting {
        server: server.into(),
        topic: topic.into(),
    }
}

#[test]
fn posts_the_alert_text_to_the_topic_on_ntfy_sh() {
    let http = RecordingPost::ok();
    let mut channel = NtfyChannel::new(
        ntfy("https://ntfy.sh", "hackathon-radar-test-topic"),
        None,
        &http,
    );
    let alert = Alert {
        text: "hackathon-radar: welcome! 2 Hackathons already fit you

Odoo Hackathon #6
Ramillies, 20–22 Nov 2026
https://hackathon.be/hackathons/odoo-hackathon-6-2026/

Hacking Health Liège
Liège, 12 Mar 2027
https://hackathon.be/hackathons/hacking-health-liege-2027/"
            .into(),
    };

    channel.send(&alert).unwrap();

    let requests = http.requests.borrow();
    assert_eq!(requests.len(), 1, "one Alert, one request");
    assert_eq!(
        render_request(&requests[0]),
        recorded("ntfy/alert-request.http")
    );
}

#[test]
fn sends_an_access_token_to_a_self_hosted_server() {
    let http = RecordingPost::ok();
    let mut channel = NtfyChannel::new(
        ntfy("https://ntfy.example.org", "radar"),
        Some("tk_fixture_not_a_real_token".into()),
        &http,
    );
    let alert = Alert {
        text: "hackathon-radar: 1 Hackathon newly fits you\n\nMystery Hack\n1 Dec 2026".into(),
    };

    channel.send(&alert).unwrap();

    assert_eq!(
        render_request(&http.requests.borrow()[0]),
        recorded("ntfy/alert-request-with-token.http")
    );
}

#[test]
fn a_refused_request_is_an_error_that_names_the_server_but_not_the_secrets() {
    let http = RecordingPost {
        reply: Err("http status: 403".into()),
        ..RecordingPost::ok()
    };
    let mut channel = NtfyChannel::new(
        ntfy("https://ntfy.sh", "secret-topic"),
        Some("tk_secret".into()),
        &http,
    );

    let error = channel
        .send(&Alert {
            text: "hello".into(),
        })
        .unwrap_err();

    let message = format!("{error:#}");
    assert_eq!(
        message,
        "sending the Alert to ntfy at https://ntfy.sh: http status: 403"
    );
    assert!(!message.contains("secret"), "{message}");
}
