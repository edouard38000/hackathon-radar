//! Seam 2: the Codex adapter against its contract, with a fake process runner
//! instead of the real `codex`.
//!
//! The `codex exec --json` event streams in `tests/fixtures/codex/` are
//! hand-made, not recorded: Codex wasn't installed where they were written.
//! They follow the event shapes in OpenAI's non-interactive mode docs.

mod support;

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use hackathon_radar::agent::codex::{self, Codex};
use hackathon_radar::agent::{self, Agent, JudgeRequest, Judgement, SCHEMA, TravelFunded};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::profile::{AgentAdapter, AgentSettings, Profile};
use support::*;

/// Records each command, and what its schema file and working directory held
/// while it ran, and answers with canned output instead of running it.
struct FakeRunner {
    reply: Result<Output, String>,
    commands: RefCell<Vec<Command>>,
    schema_files: RefCell<Vec<(PathBuf, String)>>,
    working_dir_entries: RefCell<Vec<usize>>,
}

impl FakeRunner {
    fn printing(stdout: &str, code: i32) -> Self {
        Self::replying(Ok(Output {
            code: Some(code),
            stdout: stdout.to_owned(),
            stderr: String::new(),
        }))
    }

    fn replying(reply: Result<Output, String>) -> Self {
        Self {
            reply,
            commands: RefCell::new(Vec::new()),
            schema_files: RefCell::new(Vec::new()),
            working_dir_entries: RefCell::new(Vec::new()),
        }
    }

    fn command(&self) -> Command {
        let commands = self.commands.borrow();
        assert_eq!(commands.len(), 1);
        commands[0].clone()
    }
}

impl ProcessRunner for &FakeRunner {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        self.commands.borrow_mut().push(command.clone());
        if let Some(at) = command.args.iter().position(|a| a == "--output-schema") {
            let path = PathBuf::from(&command.args[at + 1]);
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            self.schema_files.borrow_mut().push((path, text));
        }
        if let Some(dir) = &command.working_dir {
            let entries = std::fs::read_dir(dir).map_or(usize::MAX, |d| d.count());
            self.working_dir_entries.borrow_mut().push(entries);
        }
        self.reply.clone().map_err(anyhow::Error::msg)
    }
}

fn candidate(name: &str) -> Candidate {
    Candidate {
        source: "fake".into(),
        source_key: name.into(),
        name: name.into(),
        start: date(2026, 12, 5),
        end: date(2026, 12, 6),
        city: Some("Ghent".into()),
        location: None,
        country: Some("Belgium".into()),
        link: Some("https://example.org/hack".into()),
    }
}

fn ask(runner: &FakeRunner) -> anyhow::Result<String> {
    let candidates = [
        candidate("Ghent Build Weekend"),
        candidate("Liège Data Days"),
    ];
    let ids = agent::numbered_ids(candidates.len());
    let profile = profile(&["Belgium"]);
    let request = JudgeRequest::new(&candidates, &ids, &profile);
    Codex::new(runner).judge(&request)
}

/// A `codex exec --json` event stream whose final message is `answer`.
fn events_answering(answer: &str) -> String {
    let message = serde_json::json!({
        "type": "item.completed",
        "item": {"id": "item_3", "type": "agent_message", "text": answer},
    });
    format!(
        "{{\"type\": \"thread.started\", \"thread_id\": \"t-1\"}}\n{{\"type\": \"turn.started\"}}\n{message}\n\
         {{\"type\": \"turn.completed\", \"usage\": {{\"input_tokens\": 10, \"cached_input_tokens\": 0, \"output_tokens\": 5}}}}\n"
    )
}

/// An answer judging Candidates `1` to `count` as Hackathons the person can attend.
fn approving_answer(count: usize) -> String {
    let items: Vec<serde_json::Value> = agent::numbered_ids(count)
        .into_iter()
        .map(|id| {
            serde_json::json!({
                "id": id, "is_hackathon": true, "eligible": true,
                "eligibility_restriction": null, "travel_funded": "none",
                "travel_detail": null, "reason": "Participants build software on site.",
            })
        })
        .collect();
    serde_json::json!({ "judgements": items }).to_string()
}

#[test]
fn runs_codex_exec_read_only_with_live_search_and_the_schema_file() {
    let runner = FakeRunner::printing(&fixture("codex/success.jsonl"), 0);
    let candidates = [candidate("Ghent Build Weekend")];
    let ids = agent::numbered_ids(1);
    let profile = profile(&["Belgium"]);
    let request = JudgeRequest::new(&candidates, &ids, &profile);

    Codex::new(&runner).judge(&request).unwrap();

    let command = runner.command();
    let (schema_file, schema_text) = runner.schema_files.borrow()[0].clone();
    assert_eq!(command.program, "codex");
    assert_eq!(
        command.args,
        [
            "exec",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "-c",
            "approval_policy=\"never\"",
            "-c",
            "web_search=\"live\"",
            "-c",
            "features.shell_tool=false",
            "-c",
            "features.view_image=false",
            "--output-schema",
            schema_file.to_str().unwrap(),
            "--json",
            "-",
        ]
    );
    assert_eq!(command.stdin, request.prompt, "the prompt goes on stdin");
    assert_eq!(command.timeout, codex::DEFAULT_TIMEOUT);

    assert_eq!(
        schema_text, SCHEMA,
        "the schema file holds the built-in schema"
    );
    let dir = command.working_dir.as_deref().expect("a working directory");
    assert_ne!(dir, std::env::current_dir().unwrap());
    assert_eq!(
        *runner.working_dir_entries.borrow(),
        [0],
        "the working directory is empty while Codex runs"
    );
    assert!(
        !schema_file.starts_with(dir),
        "the schema file is outside the working directory"
    );
    assert!(!dir.exists(), "the working directory is removed afterwards");
    assert!(
        !schema_file.exists(),
        "the schema file is removed afterwards"
    );
}

#[test]
fn never_passes_a_writable_sandbox_or_a_bypass() {
    let runner = FakeRunner::printing(&fixture("codex/success.jsonl"), 0);
    ask(&runner).unwrap();

    let args = runner.command().args;
    let forbidden = [
        "danger-full-access",
        "workspace-write",
        "--dangerously-bypass-approvals-and-sandbox",
        "--yolo",
        "--full-auto",
        "--approve-for-me",
        "--not-so-yolo",
        "--dangerously-bypass-hook-trust",
        "--add-dir",
        "on-request",
        "sandbox_mode",
        "default_permissions",
        "permissions.",
        "network_access",
        "mcp_servers",
        "--remote",
        "--profile",
        "shell_tool=true",
    ];
    for arg in &args {
        for word in forbidden {
            assert!(!arg.contains(word), "{arg:?} contains {word:?}");
        }
    }
    let sandboxes: Vec<usize> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "--sandbox" || *a == "-s" || a.starts_with("--sandbox="))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(sandboxes.len(), 1, "exactly one sandbox flag: {args:?}");
    assert_eq!(args[sandboxes[0] + 1], "read-only");
    assert!(!args.iter().any(|a| a == "-a" || a == "--ask-for-approval"));
}

#[test]
fn the_persons_config_rules_and_shell_are_shut_out() {
    let runner = FakeRunner::printing(&fixture("codex/success.jsonl"), 0);
    ask(&runner).unwrap();

    let args = runner.command().args;
    let overrides: Vec<&str> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "-c")
        .map(|(i, _)| args[i + 1].as_str())
        .collect();
    assert_eq!(
        overrides,
        [
            "approval_policy=\"never\"",
            "web_search=\"live\"",
            "features.shell_tool=false",
            "features.view_image=false",
        ]
    );
    for flag in [
        "--ignore-user-config",
        "--ignore-rules",
        "--ephemeral",
        "--json",
    ] {
        assert!(args.iter().any(|a| a == flag), "{flag} is missing");
    }
    assert_eq!(args.first().map(String::as_str), Some("exec"));
    assert_eq!(
        args.last().map(String::as_str),
        Some("-"),
        "prompt on stdin"
    );
}

#[test]
fn the_profile_selects_codex_only_with_the_experimental_opt_in() {
    let text = |agent: &str| {
        format!(
            "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n[agent]\n{agent}"
        )
    };
    let refused = Profile::from_toml(&text("adapter = \"codex\"\n"), Path::new("p.toml"))
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("read-only lockdown is unverified against a real Codex CLI"),
        "{refused}"
    );

    let profile = Profile::from_toml(
        &text("adapter = \"codex\"\nexperimental = true\n"),
        Path::new("p.toml"),
    )
    .unwrap();
    assert_eq!(profile.agent.adapter, AgentAdapter::Codex);
    assert_eq!(agent::from_settings(&profile.agent).name(), "codex");
}

#[test]
fn every_run_with_codex_carries_a_one_line_warning() {
    let settings = AgentSettings {
        adapter: AgentAdapter::Codex,
        experimental: true,
        ..AgentSettings::default()
    };
    let warning = agent::from_settings(&settings)
        .warning()
        .expect("Codex warns")
        .to_owned();
    assert_eq!(warning, codex::WARNING);
    assert!(!warning.contains('\n'), "one line: {warning:?}");
    assert!(warning.contains("experimental"), "{warning}");
    assert!(
        warning.contains("unverified against a real Codex CLI"),
        "{warning}"
    );
    let runner = FakeRunner::printing("", 0);
    assert_eq!(Codex::new(&runner).warning(), Some(codex::WARNING));

    // The other adapters print none.
    assert_eq!(
        agent::from_settings(&AgentSettings::default()).warning(),
        None
    );
}

#[test]
fn a_hand_made_event_stream_parses_into_judgements() {
    let runner = FakeRunner::printing(&fixture("codex/success.jsonl"), 0);
    let candidates = [
        candidate("Ghent Build Weekend"),
        candidate("Liège Data Days"),
    ];
    let ids = agent::numbered_ids(2);
    let profile = profile(&["Belgium"]);
    let request = JudgeRequest::new(&candidates, &ids, &profile);

    let answer = Codex::new(&runner).judge(&request).unwrap();
    let judged: Vec<(&str, Judgement)> = request
        .read_judgements(&answer)
        .unwrap()
        .into_iter()
        .map(|(c, j)| (c.name.as_str(), j))
        .collect();

    assert_eq!(
        judged,
        [
            (
                "Ghent Build Weekend",
                Judgement {
                    is_hackathon: true,
                    eligible: true,
                    eligibility_restriction: None,
                    travel_funded: TravelFunded::None,
                    travel_detail: None,
                    reason: "Teams build software on site in Ghent over a weekend; the page states no participation restriction.".into(),
                }
            ),
            (
                "Liège Data Days",
                Judgement {
                    is_hackathon: true,
                    eligible: true,
                    eligibility_restriction: Some("Open to anyone aged 18 or over".into()),
                    travel_funded: TravelFunded::Partial,
                    travel_detail: Some("Lodging in Liège for participants travelling from abroad".into()),
                    reason: "Participants build data projects on open city data during the two days.".into(),
                }
            ),
        ]
    );
}

#[test]
fn the_last_agent_message_is_the_answer() {
    let stream = format!(
        "{}{}",
        events_answering("{\"draft\": true}"),
        events_answering(&approving_answer(2))
    );
    let runner = FakeRunner::printing(&stream, 0);
    assert_eq!(ask(&runner).unwrap(), approving_answer(2));
}

#[test]
fn an_answer_that_does_not_match_the_schema_is_rejected() {
    let runner = FakeRunner::printing(
        &events_answering(&approving_answer(2).replace("\"none\"", "\"yes\"")),
        0,
    );
    let candidates = [
        candidate("Ghent Build Weekend"),
        candidate("Liège Data Days"),
    ];
    let ids = agent::numbered_ids(2);
    let profile = profile(&["Belgium"]);
    let request = JudgeRequest::new(&candidates, &ids, &profile);

    let answer = Codex::new(&runner).judge(&request).unwrap();

    let err = request.read_judgements(&answer).unwrap_err();
    assert!(
        err.to_string()
            .contains("doesn't match the judgements schema"),
        "{err}"
    );
}

#[test]
fn a_failed_turn_is_an_error_that_says_to_sign_in_yourself() {
    let runner = FakeRunner::printing(&fixture("codex/turn-failed.jsonl"), 1);
    let message = format!("{:#}", ask(&runner).unwrap_err());
    assert!(message.contains("Codex failed (exit code 1)"), "{message}");
    assert!(message.contains("401 Unauthorized"), "{message}");
    assert!(
        message.contains("run `codex login` once yourself"),
        "{message}"
    );
}

#[test]
fn a_recovered_error_event_does_not_spoil_an_answer() {
    let stream = format!(
        "{{\"type\": \"error\", \"message\": \"Reconnecting... 1/5\"}}\n{}",
        events_answering(&approving_answer(2))
    );
    let runner = FakeRunner::printing(&stream, 0);
    assert_eq!(ask(&runner).unwrap(), approving_answer(2));
}

#[test]
fn a_failing_exit_is_an_error_even_with_a_message() {
    let runner = FakeRunner::printing(&events_answering(&approving_answer(2)), 1);
    let message = format!("{:#}", ask(&runner).unwrap_err());
    assert!(message.contains("Codex failed (exit code 1)"), "{message}");
}

#[test]
fn finishing_without_an_answer_is_an_error() {
    let runner = FakeRunner::printing(
        "{\"type\": \"thread.started\", \"thread_id\": \"t-1\"}\n{\"type\": \"turn.completed\", \"usage\": {}}\n",
        0,
    );
    let message = format!("{:#}", ask(&runner).unwrap_err());
    assert!(
        message.contains("Codex finished without an answer"),
        "{message}"
    );
}

#[test]
fn output_with_no_events_is_an_error_that_shows_stderr() {
    let runner = FakeRunner::replying(Ok(Output {
        code: Some(2),
        stdout: String::new(),
        stderr: "error: unexpected argument '--ignore-rules' found".into(),
    }));
    let message = format!("{:#}", ask(&runner).unwrap_err());
    assert!(
        message.contains("Codex gave no JSON events (exit code 2)"),
        "{message}"
    );
    assert!(message.contains("unexpected argument"), "{message}");
}

#[test]
fn a_timeout_or_missing_codex_is_passed_on() {
    let runner = FakeRunner::replying(Err(
        "could not start `codex`; is it installed and on your PATH?".into(),
    ));
    assert!(format!("{:#}", ask(&runner).unwrap_err()).contains("is it installed"));
}

#[test]
fn a_whole_run_through_codex_stores_what_it_judged() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let runner = FakeRunner::printing(&events_answering(&approving_answer(5)), 0);
    let codex = Codex::new(&runner);

    let outcome = harness.run_with_agent(
        &codex,
        &[hackathon_be_from_fixture()],
        date(2026, 11, 9),
        DRY_RUN,
    );

    assert_eq!(outcome.agent_failure, None);
    assert_eq!(outcome.judged, 5);
    assert_eq!(harness.stored_names().len(), 5);
    assert!(harness.unjudged_names().is_empty());
    assert!(
        harness
            .store
            .judgements()
            .unwrap()
            .iter()
            .all(|j| j.agent == "codex"),
        "each judgement records the Agent that made it"
    );
}

#[test]
fn a_whole_run_through_codex_keeps_rejected_candidates_for_the_next_run() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let runner = FakeRunner::printing(&events_answering("Sure! Here you go."), 0);
    let codex = Codex::new(&runner);

    let outcome = harness.run_with_agent(
        &codex,
        &[hackathon_be_from_fixture()],
        date(2026, 11, 9),
        DRY_RUN,
    );

    assert!(
        outcome
            .agent_failure
            .unwrap()
            .contains("doesn't match the judgements schema")
    );
    assert!(harness.stored_names().is_empty());
    assert_eq!(harness.unjudged_names().len(), 5);
}
