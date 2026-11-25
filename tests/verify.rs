//! The `verify` skill's harness (`.claude/skills/verify/SKILL.md`): boots the
//! built binary, drives one whole radar flow in a throwaway directory and
//! prints what it saw, to paste as a pull request's Evidence:
//!
//! ```sh
//! cargo test --locked --test verify -- --nocapture
//! ```
//!
//! The binary really fetches a calendar, runs an Agent and sends Alerts, all
//! on this machine: a local server holds the calendar and takes the ntfy
//! Alerts, and a shell script stands in for the Agent through the `command`
//! adapter. The server is also the binary's proxy and refuses everything else,
//! so no request leaves the machine. Secrets come from the environment, so the
//! keychain is never read.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use chrono::{Days, NaiveDate, Utc};
use hackathon_radar::secrets::SecretName;
use hackathon_radar::wiring::TEST_ALERT;
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_hackathon-radar");
const REPO: &str = env!("CARGO_MANIFEST_DIR");

/// Answers the judging and travel requests: every Candidate is an open
/// Hackathon, except one whose name says it is a Meetup.
const AGENT: &str = r#"awk -v deadline="$1" '
/^## Destinations/ { travel = 1 }
/"id": / { match($0, /"id": "[0-9]+"/); id = substr($0, RSTART + 7, RLENGTH - 8) }
travel && /"id": / {
    out = out sep "{\"id\":\"" id "\",\"train_hours\":0.6,\"airport\":null}"; sep = ","
}
!travel && /"name": / {
    is = /Meetup/ ? "false" : "true"
    out = out sep "{\"id\":\"" id "\",\"is_hackathon\":" is ",\"eligible\":true," \
        "\"eligibility_restriction\":null,\"travel_funded\":\"none\",\"travel_detail\":null," \
        "\"application_deadline\":\"" deadline "\",\"application_status\":\"open\"," \
        "\"reason\":\"Stand-in Agent.\"}"; sep = ","
}
END { print "{\"" (travel ? "travel" : "judgements") "\":[" out "]}" }
'
"#;

#[derive(Default)]
struct Seen {
    /// The bodies POSTed to the ntfy topic.
    alerts: Vec<String>,
    /// The targets of requests refused because they would leave the machine.
    refused: Vec<String>,
}

/// Serves `calendar`, records Alerts and refuses everything else.
fn serve(calendar: String) -> (String, Arc<Mutex<Seen>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Seen::default()));
    let record = Arc::clone(&seen);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // A refused client may hang up before reading the whole reply.
            let _ = answer(stream, &calendar, &record);
        }
    });
    (url, seen)
}

/// Answers one request; a connection the client drops is left unanswered.
fn answer(mut stream: TcpStream, calendar: &str, seen: &Mutex<Seen>) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut length = 0;
    let mut header = String::new();
    while reader.read_line(&mut header)? > 2 {
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
        header.clear();
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;

    let mut words = request.split_whitespace();
    let (method, target) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    let (status, reply) = match (method, target) {
        ("GET", "/calendar.ics") => ("200 OK", calendar),
        ("POST", "/verify") => {
            let body = String::from_utf8_lossy(&body).into_owned();
            seen.lock().unwrap().alerts.push(body);
            ("200 OK", "{}")
        }
        _ => {
            seen.lock().unwrap().refused.push(target.to_owned());
            ("502 Bad Gateway", "")
        }
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    )
}

/// A throwaway home directory the binary runs in.
struct Sandbox {
    dir: TempDir,
    proxy: String,
}

struct Ran {
    stdout: String,
    stderr: String,
}

impl Sandbox {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Runs the binary with a clean environment, and expects it to succeed.
    fn run(&self, args: &[&str]) -> Ran {
        let home = self.dir.path();
        let mut command = Command::new(BINARY);
        command
            .args(args)
            .current_dir(home)
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", home.join("bin").display()),
            )
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env("ALL_PROXY", &self.proxy)
            .env("NO_PROXY", "127.0.0.1");
        for name in SecretName::ALL {
            command.env(name.env_var(), "verify-placeholder");
        }
        let output = command.output().expect("the binary runs");
        let ran = Ran {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        };
        assert!(
            output.status.success(),
            "`hackathon-radar {}` failed ({}):\n{}\n{}",
            args.join(" "),
            output.status,
            ran.stdout,
            ran.stderr
        );
        ran
    }
}

fn expect(text: &str, wanted: &str) {
    assert!(text.contains(wanted), "expected {wanted:?} in:\n{text}");
}

/// Only the Hackathon that Fits: not the Meetup the Agent turned down, the one
/// that has passed, or the one outside the Countries.
fn expect_only_the_fitting_hackathon(text: &str) {
    expect(text, "Ghent Build Weekend");
    for left_out in ["Product Meetup", "Past Jam", "Tokyo Robot Hack"] {
        assert!(!text.contains(left_out), "{left_out:?} got into:\n{text}");
    }
}

fn event(name: &str, place: &str, start: NaiveDate) -> String {
    let end = start + Days::new(2);
    let slug = name.to_lowercase().replace(' ', "-");
    format!(
        "BEGIN:VEVENT\r\nUID:{slug}\r\nSUMMARY:{name}\r\nDTSTART;VALUE=DATE:{}\r\n\
         DTEND;VALUE=DATE:{}\r\nLOCATION:{place}\r\nURL:https://example.org/{slug}\r\nEND:VEVENT\r\n",
        start.format("%Y%m%d"),
        end.format("%Y%m%d")
    )
}

fn git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .current_dir(REPO)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

#[test]
fn the_built_binary_runs_a_whole_radar_flow() {
    let today = Utc::now().date_naive();
    let later = |days| today + Days::new(days);
    let calendar = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{}{}{}{}END:VCALENDAR\r\n",
        event("Ghent Build Weekend", "Ghent, Belgium", later(30)),
        event("Brussels Product Meetup", "Brussels, Belgium", later(31)),
        event("Leuven Past Jam", "Leuven, Belgium", today - Days::new(30)),
        event("Tokyo Robot Hack", "Tokyo, Japan", later(40)),
    );
    let (server, seen) = serve(calendar);
    let sandbox = Sandbox {
        dir: TempDir::new().unwrap(),
        proxy: server.clone(),
    };
    let alerts = || seen.lock().unwrap().alerts.clone();
    let mut evidence = Vec::new();
    let mut passed = |step: &str, saw: String| evidence.push(format!("PASS {step}: {saw}"));

    let version = sandbox.run(&["--version"]).stdout;
    expect(&version, "hackathon-radar ");
    passed("--version", version.trim().to_owned());

    // `init` only checks that the Agent's program is on the PATH.
    std::fs::create_dir(sandbox.path("bin")).unwrap();
    let claude = sandbox.path("bin/claude");
    std::fs::write(&claude, "#!/bin/sh\nexit 1\n").unwrap();
    assert!(
        Command::new("chmod")
            .arg("+x")
            .arg(&claude)
            .status()
            .unwrap()
            .success()
    );
    sandbox.run(&[
        "init",
        "--yes",
        "--home-city",
        "Brussels",
        "--countries",
        "Belgium",
        "--alert-channel",
        "none",
    ]);
    let profile_path = sandbox.path(".config/hackathon-radar/profile.toml");
    let written = std::fs::read_to_string(&profile_path).unwrap();
    expect(&written, "Brussels");
    passed("init --yes", "wrote a Profile with Home in Brussels".into());

    // From here, a Profile that points every outside dependency at this test.
    std::fs::write(sandbox.path("agent.sh"), AGENT).unwrap();
    std::fs::write(
        &profile_path,
        format!(
            "countries = [\"Belgium\"]\n\
             [home]\ncity = \"Brussels\"\ncountry = \"Belgium\"\nairports = [\"BRU\"]\n\
             [agent]\nadapter = \"command\"\ncommand = [\"sh\", {}, \"{}\"]\n\
             [alert_channel]\nkind = \"ntfy\"\nserver = \"{server}\"\ntopic = \"verify\"\n\
             [sources]\nics = [\"{server}/calendar.ics\"]\n",
            toml_path(&sandbox.path("agent.sh")),
            later(10),
        ),
    )
    .unwrap();

    sandbox.run(&["test-alert"]);
    assert_eq!(alerts(), [TEST_ALERT]);
    passed(
        "test-alert",
        "the ntfy server received the test Alert".into(),
    );

    let dry = sandbox.run(&["run", "--dry-run"]);
    // Four events from the calendar and nothing else: every other Source was refused.
    expect(&dry.stdout, "Collected 4 Candidates; 1 Hackathons fit");
    expect(&dry.stdout, "Dry run, so nothing was sent");
    expect_only_the_fitting_hackathon(&dry.stdout);
    assert_eq!(alerts().len(), 1, "a dry run sent an Alert");
    let unread = dry.stderr.matches("could not read the Source").count();
    assert!(
        unread > 0,
        "the built-in Sources were not asked:\n{}",
        dry.stderr
    );
    passed(
        "run --dry-run",
        format!(
            "4 Candidates, 1 fits, nothing sent; {unread} built-in Sources refused by the harness"
        ),
    );

    let live = sandbox.run(&["run"]);
    expect(&live.stdout, "Sent the Alert through ntfy.");
    let sent = alerts();
    assert_eq!(sent.len(), 2, "one Run sends one Alert: {sent:?}");
    expect_only_the_fitting_hackathon(&sent[1]);
    passed("run", "the ntfy server received one Alert (below)".into());

    let again = sandbox.run(&["run"]);
    expect(&again.stdout, "Nothing new to send.");
    assert_eq!(alerts().len(), 2, "the same Hackathon was alerted twice");
    passed("run again", "Nothing new to send".into());

    let report = sandbox.run(&["report"]).stdout;
    expect_only_the_fitting_hackathon(&report);
    passed("report", "lists only the Hackathon that Fits".into());

    let ics = sandbox.run(&["export", "ics"]).stdout;
    expect(&ics, "BEGIN:VEVENT");
    expect_only_the_fitting_hackathon(&ics);
    passed("export ics", "a calendar event for its deadline".into());

    sandbox.run(&["export", "sheet", "--out", "sheet.csv"]);
    let imported = sandbox.run(&["import-sheet", "sheet.csv", "--db", "second.sqlite"]);
    expect(&imported.stdout, "Imported 1 new Hackathons");
    let second = sandbox.run(&["report", "--db", "second.sqlite"]).stdout;
    expect_only_the_fitting_hackathon(&second);
    passed(
        "export sheet, import-sheet",
        "the exported Sheet imports into a new database".into(),
    );

    let refused = seen.lock().unwrap().refused.len();
    let dirty = if git(&["status", "--porcelain"]).is_empty() {
        ""
    } else {
        " with uncommitted changes"
    };
    println!("---- paste into the pull request's Evidence ----");
    println!("```");
    println!(
        "verify hackathon-radar @ {}{dirty}, {} {}",
        git(&["rev-parse", "--short", "HEAD"]),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for line in &evidence {
        println!("{line}");
    }
    println!("{refused} outbound requests refused by the harness; none left the machine");
    println!("\nThe Alert the ntfy server received:\n{}", sent[1]);
    println!("RESULT: PASS ({} steps)", evidence.len());
    println!("```");
}
