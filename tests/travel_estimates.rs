//! Seam 2: the Agent's travel answers against their schema, from saved
//! fixtures, and the travel request through the Claude Code adapter with a
//! fake process runner instead of the real `claude`.

mod support;

use std::cell::RefCell;

use hackathon_radar::agent::claude_code::{ALLOWED_TOOLS, ClaudeCode};
use hackathon_radar::agent::command::{CommandTemplate, Generic};
use hackathon_radar::agent::travel::{Destination, Estimate, TRAVEL_SCHEMA, TravelRequest};
use hackathon_radar::agent::{self, Agent, JudgeRequest, SCHEMA};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::profile::{DEFAULT_MAX_TURNS, DEFAULT_TRAVEL_MAX_TURNS};
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
    let profile = travel_profile(&["France"], "");
    let claude = ClaudeCode::new(&runner, profile.agent.max_turns);
    let destinations = destinations();
    let ids = agent::numbered_ids(destinations.len());
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
    // The same read-only command line as judging, carrying the travel schema
    // and the travel step cap.
    assert_eq!(
        command.args,
        claude.args_with_cap(TRAVEL_SCHEMA, DEFAULT_TRAVEL_MAX_TURNS)
    );
    assert!(
        command
            .args
            .windows(2)
            .any(|w| w == ["--tools", ALLOWED_TOOLS])
    );
    assert_eq!(command.stdin, request.prompt);
    assert!(command.stdin.contains("\"city\": \"Bastia\""));
}

/// The value after `--max-turns` in a command line.
fn max_turns(args: &[String]) -> &str {
    let at = args.iter().position(|a| a == "--max-turns").unwrap();
    &args[at + 1]
}

#[test]
fn the_travel_request_is_capped_by_travel_max_turns_and_judging_by_max_turns() {
    let runner = FakeRunner {
        stdout: fixture("travel/claude-code-result.json"),
        commands: RefCell::new(Vec::new()),
    };
    let profile = travel_profile(
        &["France"],
        "[agent]\nmax_turns = 40\ntravel_max_turns = 7\n",
    );
    let claude = ClaudeCode::new(&runner, profile.agent.max_turns);

    let destinations = destinations();
    let ids = agent::numbered_ids(destinations.len());
    let travel = TravelRequest::new(&destinations, &ids, &profile);
    claude.judge(&travel.agent_request()).unwrap();
    let candidates = [Candidate {
        source: "fake".into(),
        source_key: "1".into(),
        name: "Paris Hack".into(),
        start: date(2026, 12, 5),
        end: date(2026, 12, 6),
        city: Some("Paris".into()),
        location: None,
        country: Some("France".into()),
        link: None,
    }];
    let candidate_ids = agent::numbered_ids(1);
    let judging = JudgeRequest::new(&candidates, &candidate_ids, &profile);
    // The canned output is a travel answer; only the command line matters here.
    let _ = claude.judge(&judging);

    let commands = runner.commands.borrow();
    assert_eq!(commands.len(), 2);
    assert_eq!(max_turns(&commands[0].args), "7");
    assert_eq!(max_turns(&commands[1].args), "40");
    assert_eq!(commands[1].args, claude.args(SCHEMA));
}

#[test]
fn the_default_travel_cap_is_15_steps_whatever_max_turns_is() {
    let runner = FakeRunner {
        stdout: fixture("travel/claude-code-result.json"),
        commands: RefCell::new(Vec::new()),
    };
    let profile = travel_profile(&["France"], "");
    assert_eq!(profile.agent.max_turns, DEFAULT_MAX_TURNS);
    let claude = ClaudeCode::new(&runner, profile.agent.max_turns);
    let destinations = destinations();
    let ids = agent::numbered_ids(destinations.len());

    claude
        .judge(&TravelRequest::new(&destinations, &ids, &profile).agent_request())
        .unwrap();

    assert_eq!(max_turns(&runner.commands.borrow()[0].args), "15");
}

#[test]
fn running_out_of_travel_steps_names_travel_max_turns() {
    // Recorded from `claude -p` stopped by its step cap.
    let runner = FakeRunner {
        stdout: fixture("claude-code/max-turns.json"),
        commands: RefCell::new(Vec::new()),
    };
    let profile = travel_profile(&["France"], "[agent]\ntravel_max_turns = 3\n");
    let claude = ClaudeCode::new(&runner, profile.agent.max_turns);
    let destinations = destinations();
    let ids = agent::numbered_ids(destinations.len());

    let err = claude
        .judge(&TravelRequest::new(&destinations, &ids, &profile).agent_request())
        .unwrap_err();

    let message = format!("{err:#}");
    assert!(message.contains("used all 3 steps"), "{message}");
    assert!(
        message.contains("raise agent.travel_max_turns"),
        "{message}"
    );
}

#[test]
fn the_command_adapter_fills_max_turns_from_travel_max_turns_for_the_travel_request() {
    let runner = FakeRunner {
        stdout: fixture("travel/answer.json"),
        commands: RefCell::new(Vec::new()),
    };
    let template = CommandTemplate::parse(&[
        "my-agent".into(),
        "--steps={max_turns}".into(),
        "{schema_file}".into(),
    ])
    .unwrap();
    let profile = travel_profile(
        &["France"],
        "[agent]\nmax_turns = 40\ntravel_max_turns = 7\n",
    );
    let generic = Generic::new(&runner, template, profile.agent.max_turns);
    let destinations = destinations();
    let ids = agent::numbered_ids(destinations.len());
    let travel = TravelRequest::new(&destinations, &ids, &profile);

    let answer = generic.judge(&travel.agent_request()).unwrap();
    let judging_ids = agent::numbered_ids(0);
    let _ = generic.judge(&JudgeRequest::new(&[], &judging_ids, &profile));

    assert_eq!(
        travel.read_estimates(&answer).unwrap(),
        expected(&destinations)
    );
    let commands = runner.commands.borrow();
    assert_eq!(
        commands[0].args[0], "--steps=7",
        "travel: agent.travel_max_turns"
    );
    assert_eq!(
        commands[1].args[0], "--steps=40",
        "judging: agent.max_turns"
    );
    assert_eq!(commands[0].stdin, travel.prompt);
}
