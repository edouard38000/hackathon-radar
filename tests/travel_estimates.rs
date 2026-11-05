//! Seam 2: the Agent's travel answers against their schema, from saved
//! fixtures, and the travel request through the Claude Code adapter with a
//! fake process runner instead of the real `claude`.

mod support;

use std::cell::RefCell;

use hackathon_radar::agent::claude_code::{ALLOWED_TOOLS, ClaudeCode};
use hackathon_radar::agent::travel::{Destination, Estimate, TRAVEL_SCHEMA, TravelRequest};
use hackathon_radar::agent::{self, Agent};
use hackathon_radar::process::{Command, Output, ProcessRunner};
use support::*;

const MALFORMED: [&str; 12] = [
    "not-json.txt",
    "judgements.json",
    "bare-array.json",
    "unknown-id.json",
    "twice.json",
    "missing-airport.json",
    "extra-key.json",
    "airport-name.json",
    "zero-hours.json",
    "negative-hours.json",
    "days-of-train.json",
    "hours-as-text.json",
];

fn destinations() -> Vec<Destination> {
    [
        ("Paris", "France"),
        ("Munich", "Germany"),
        ("Bastia", "France"),
    ]
    .iter()
    .map(|(city, country)| Destination {
        city: (*city).into(),
        country: (*country).into(),
    })
    .collect()
}

fn expected(destinations: &[Destination]) -> Vec<(&Destination, Estimate)> {
    let estimate = |train_minutes, airport: &str| Estimate {
        train_minutes,
        airport: Some(airport.into()),
    };
    vec![
        (&destinations[0], estimate(Some(84), "CDG")),
        (&destinations[1], estimate(Some(384), "MUC")),
        (&destinations[2], estimate(None, "BIA")),
    ]
}

#[test]
fn a_saved_answer_becomes_estimates() {
    let destinations = destinations();
    let ids = agent::numbered_ids(destinations.len());
    let profile = travel_profile(&["France"], "");
    let request = TravelRequest::new(&destinations, &ids, &profile);

    let estimated = request
        .read_estimates(&fixture("travel/answer.json"))
        .unwrap();

    assert_eq!(estimated, expected(&destinations));
}

#[test]
fn malformed_answers_are_rejected_whole() {
    let destinations = destinations();
    let ids = agent::numbered_ids(destinations.len());
    let profile = travel_profile(&["France"], "");
    let request = TravelRequest::new(&destinations, &ids, &profile);

    for name in MALFORMED {
        let answer = fixture(&format!("travel/malformed/{name}"));
        let err = request.read_estimates(&answer).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("the Agent's answer doesn't match the travel schema"),
            "{name}: {err}"
        );
    }
}

/// Records each command and prints a canned `claude -p` result instead of running it.
struct FakeRunner {
    stdout: String,
    commands: RefCell<Vec<Command>>,
}

impl ProcessRunner for &FakeRunner {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        self.commands.borrow_mut().push(command.clone());
        Ok(Output {
            code: Some(0),
            stdout: self.stdout.clone(),
            stderr: String::new(),
        })
    }
}

#[test]
fn claude_code_runs_the_travel_request_read_only_with_its_own_schema() {
    let runner = FakeRunner {
        stdout: fixture("travel/claude-code-result.json"),
        commands: RefCell::new(Vec::new()),
    };
    let claude = ClaudeCode::new(&runner, 20);
    let destinations = destinations();
    let ids = agent::numbered_ids(destinations.len());
    let profile = travel_profile(&["France"], "");
    let request = TravelRequest::new(&destinations, &ids, &profile);

    let answer = claude.judge(&request.agent_request()).unwrap();

    assert_eq!(
        request.read_estimates(&answer).unwrap(),
        expected(&destinations)
    );
    let commands = runner.commands.borrow();
    let [command] = &commands[..] else {
        panic!("expected one command, got {}", commands.len());
    };
    // The same read-only command line as judging, carrying the travel schema.
    assert_eq!(command.args, claude.args(TRAVEL_SCHEMA));
    assert!(
        command
            .args
            .windows(2)
            .any(|w| w == ["--tools", ALLOWED_TOOLS])
    );
    assert_eq!(command.stdin, request.prompt);
    assert!(command.stdin.contains("\"city\": \"Bastia\""));
}
