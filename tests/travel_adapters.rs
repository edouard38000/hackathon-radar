//! Seam 1: whole Runs through each real adapter (Claude Code, Codex and the
//! generic command adapter), with a fake process runner instead of a real
//! Agent. Each Run makes two requests, judging and travel estimates, and the
//! travel request carries the travel schema and, where the adapter takes one,
//! the `agent.travel_max_turns` step cap.

mod support;

use std::cell::RefCell;

use hackathon_radar::agent::Agent;
use hackathon_radar::agent::claude_code::ClaudeCode;
use hackathon_radar::agent::codex::Codex;
use hackathon_radar::agent::command::{CommandTemplate, Generic};
use hackathon_radar::agent::travel::TRAVEL_SCHEMA;
use hackathon_radar::agent::{SCHEMA, numbered_ids};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::sources::Source;
use support::*;

/// How an adapter's CLI wraps the answer on stdout.
#[derive(Clone, Copy)]
enum Wrapping {
    ClaudeCode,
    Codex,
    Raw,
}

/// Answers a judging request with a judgement and a travel request with an
/// estimate for Paris, as each adapter's CLI would print them, and records
/// every command along with the schema it was given.
struct ScriptedRunner {
    wrapping: Wrapping,
    commands: RefCell<Vec<Command>>,
    schemas: RefCell<Vec<String>>,
}

impl ScriptedRunner {
    fn new(wrapping: Wrapping) -> Self {
        Self {
            wrapping,
            commands: RefCell::new(Vec::new()),
            schemas: RefCell::new(Vec::new()),
        }
    }
}

/// The schema a command was given: inline after `--json-schema`, or in the
/// file named after `--output-schema` or `--schema`.
fn schema_of(command: &Command) -> String {
    let after = |flag: &str| {
        let at = command.args.iter().position(|a| a == flag)?;
        command.args.get(at + 1).cloned()
    };
    if let Some(inline) = after("--json-schema") {
        return inline;
    }
    let path = after("--output-schema")
        .or_else(|| after("--schema"))
        .expect("the command names its schema");
    std::fs::read_to_string(path).unwrap()
}

impl ProcessRunner for &ScriptedRunner {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        let schema = schema_of(command);
        self.commands.borrow_mut().push(command.clone());
        self.schemas.borrow_mut().push(schema.clone());
        let answer = if schema == TRAVEL_SCHEMA {
            assert!(command.stdin.contains("\"city\": \"Paris\""));
            serde_json::json!({"travel": [{"id": "1", "train_hours": 1.4, "airport": "CDG"}]})
        } else {
            let ids = numbered_ids(1);
            serde_json::json!({"judgements": [{
                "id": ids[0], "is_hackathon": true, "eligible": true,
                "eligibility_restriction": null, "travel_funded": "none",
                "travel_detail": null, "application_deadline": null,
                "application_status": "unknown", "reason": "Teams build software over a weekend.",
            }]})
        };
        let stdout = match self.wrapping {
            Wrapping::ClaudeCode => serde_json::json!({
                "type": "result", "subtype": "success", "is_error": false,
                "result": answer.to_string(), "structured_output": answer,
            })
            .to_string(),
            Wrapping::Codex => [
                serde_json::json!({"type": "thread.started", "thread_id": "t"}),
                serde_json::json!({"type": "item.completed",
                    "item": {"id": "item_0", "type": "agent_message", "text": answer.to_string()}}),
                serde_json::json!({"type": "turn.completed", "usage": {"output_tokens": 40}}),
            ]
            .map(|e| e.to_string())
            .join("\n"),
            Wrapping::Raw => answer.to_string(),
        };
        Ok(Output {
            code: Some(0),
            stdout,
            stderr: String::new(),
        })
    }
}

fn paris() -> Vec<Box<dyn Source>> {
    vec![Box::new(FakeSource {
        name: "fake".into(),
        result: Ok(vec![Candidate {
            source: "fake".into(),
            source_key: "Paris Hack".into(),
            name: "Paris Hack".into(),
            start: date(2026, 12, 5),
            end: date(2026, 12, 6),
            city: Some("Paris".into()),
            location: None,
            country: Some("France".into()),
            link: None,
        }]),
    })]
}

/// A Run through `agent`, which uses `runner`; returns the recorded commands.
fn run_through(agent: &dyn Agent, runner: &ScriptedRunner) -> Vec<Command> {
    let mut harness = Harness::new(travel_profile(
        &["France"],
        "[agent]\nmax_turns = 40\ntravel_max_turns = 7\n",
    ));

    let outcome = harness.run_with_agent(agent, &paris(), date(2026, 11, 9), DRY_RUN);

    assert_eq!(outcome.agent_failure, None);
    assert_eq!(outcome.travel_failure, None);
    assert_eq!(outcome.unestimated, 0);
    assert_eq!(harness.stored_names(), ["Paris Hack"]);
    assert_eq!(
        harness.trip_label("Paris Hack").as_deref(),
        Some("train ~1h30")
    );
    assert!(
        outcome
            .alert
            .unwrap()
            .text
            .contains("Paris, 5–6 Dec 2026 · train ~1h30")
    );
    assert_eq!(*runner.schemas.borrow(), [SCHEMA, TRAVEL_SCHEMA]);
    runner.commands.borrow().clone()
}

/// The value right after `flag` in a command line.
fn value_after<'a>(command: &'a Command, flag: &str) -> &'a str {
    let at = command.args.iter().position(|a| a == flag).unwrap();
    &command.args[at + 1]
}

#[test]
fn a_run_through_claude_code_caps_the_travel_request_by_travel_max_turns() {
    let runner = ScriptedRunner::new(Wrapping::ClaudeCode);
    let claude = ClaudeCode::new(&runner, 40);

    let commands = run_through(&claude, &runner);

    assert_eq!(value_after(&commands[0], "--max-turns"), "40");
    assert_eq!(value_after(&commands[1], "--max-turns"), "7");
}

#[test]
fn a_run_through_the_command_adapter_fills_max_turns_from_travel_max_turns() {
    let runner = ScriptedRunner::new(Wrapping::Raw);
    let template = CommandTemplate::parse(&[
        "my-agent".into(),
        "--schema".into(),
        "{schema_file}".into(),
        "--steps".into(),
        "{max_turns}".into(),
    ])
    .unwrap();
    let generic = Generic::new(&runner, template, 40);

    let commands = run_through(&generic, &runner);

    assert_eq!(value_after(&commands[0], "--steps"), "40");
    assert_eq!(value_after(&commands[1], "--steps"), "7");
}

#[test]
fn a_run_through_codex_sends_the_travel_request_with_its_schema() {
    let runner = ScriptedRunner::new(Wrapping::Codex);
    let codex = Codex::new(&runner);

    let commands = run_through(&codex, &runner);

    // Codex has no step cap, so its command line is the same for both requests.
    assert_eq!(commands.len(), 2);
    assert!(!commands[1].args.iter().any(|a| a.contains("turns")));
    assert_eq!(commands[0].args.len(), commands[1].args.len());
}
