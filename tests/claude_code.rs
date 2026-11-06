//! Seam 2: the Claude Code adapter against its contract, with a fake process
//! runner instead of the real `claude`, and recorded `claude -p` results.

mod support;

use std::cell::RefCell;

use hackathon_radar::agent::claude_code::{self, ClaudeCode};
use hackathon_radar::agent::{
    self, Agent, ApplicationStatus, JudgeRequest, Judgement, SCHEMA, TravelFunded,
};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::profile::{AgentSettings, DEFAULT_MAX_TURNS, Profile};
use support::*;

/// Records each command and answers with a canned result instead of running it.
struct FakeRunner {
    reply: Result<Output, String>,
    commands: RefCell<Vec<Command>>,
}

impl FakeRunner {
    fn printing(stdout: &str, code: i32) -> Self {
        Self {
            reply: Ok(Output {
                code: Some(code),
                stdout: stdout.to_owned(),
                stderr: String::new(),
            }),
            commands: RefCell::new(Vec::new()),
        }
    }

    fn failing(error: &str) -> Self {
        Self {
            reply: Err(error.to_owned()),
            commands: RefCell::new(Vec::new()),
        }
    }
}

impl ProcessRunner for &FakeRunner {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        self.commands.borrow_mut().push(command.clone());
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

fn test_profile() -> Profile {
    profile(&["Belgium"])
}

fn ask(runner: &FakeRunner, max_turns: u32) -> anyhow::Result<String> {
    let candidates = [
        candidate("Ghent Build Weekend"),
        candidate("Liège Data Days"),
    ];
    let ids = agent::numbered_ids(candidates.len());
    let profile = test_profile();
    let request = JudgeRequest::new(&candidates, &ids, &profile);
    ClaudeCode::new(runner, max_turns).judge(&request)
}

#[test]
fn runs_claude_headless_with_web_search_and_page_reading_only() {
    let runner = FakeRunner::printing(&fixture("claude-code/success.json"), 0);
    let candidates = [candidate("Ghent Build Weekend")];
    let ids = agent::numbered_ids(1);
    let profile = test_profile();
    let request = JudgeRequest::new(&candidates, &ids, &profile);

    ClaudeCode::new(&runner, 37).judge(&request).unwrap();

    let commands = runner.commands.borrow();
    assert_eq!(commands.len(), 1);
    let command = &commands[0];
    assert_eq!(command.program, "claude");
    assert_eq!(
        command.args,
        [
            "-p",
            "--permission-mode",
            "dontAsk",
            "--tools",
            "WebSearch,WebFetch",
            "--allowedTools",
            "WebSearch,WebFetch",
            "--strict-mcp-config",
            "--setting-sources",
            "",
            "--max-turns",
            "37",
            "--output-format",
            "json",
            "--json-schema",
            SCHEMA,
        ]
    );
    assert_eq!(command.stdin, request.prompt, "the prompt goes on stdin");
    assert_eq!(command.timeout, claude_code::DEFAULT_TIMEOUT);
    let dir = command.working_dir.as_deref().expect("a working directory");
    assert_ne!(
        dir,
        std::env::current_dir().unwrap(),
        "never the current directory"
    );
    assert!(
        !dir.exists(),
        "the empty working directory is removed afterwards"
    );
}

#[test]
fn never_passes_a_bypass_or_a_write_permission() {
    let runner = FakeRunner::printing(&fixture("claude-code/success.json"), 0);
    ask(&runner, DEFAULT_MAX_TURNS).unwrap();

    let args = &runner.commands.borrow()[0].args;
    let forbidden = [
        "bypassPermissions",
        "--dangerously-skip-permissions",
        "--allow-dangerously-skip-permissions",
        "acceptEdits",
        "--add-dir",
        "--mcp-config",
        "--bare",
        "Bash",
        "Write",
        "Edit",
        "NotebookEdit",
    ];
    for arg in args {
        for word in forbidden {
            assert!(!arg.contains(word), "{arg:?} contains {word:?}");
        }
    }
    let mode = args.iter().position(|a| a == "--permission-mode").unwrap();
    assert_eq!(args[mode + 1], "dontAsk");
    let tools = args.iter().position(|a| a == "--allowedTools").unwrap();
    assert_eq!(args[tools + 1], "WebSearch,WebFetch");
    assert_eq!(
        args.iter()
            .filter(|a| a.starts_with("--permission-mode"))
            .count(),
        1
    );
}

#[test]
fn the_persons_own_settings_hooks_and_mcp_servers_are_shut_out() {
    let runner = FakeRunner::printing(&fixture("claude-code/success.json"), 0);
    ask(&runner, DEFAULT_MAX_TURNS).unwrap();

    let args = &runner.commands.borrow()[0].args;
    let value_of = |flag: &str| {
        let at = args.iter().position(|a| a == flag);
        at.map(|i| args[i + 1].as_str())
    };
    // Only web search and page reading exist in the session, not merely are approved.
    assert_eq!(value_of("--tools"), Some("WebSearch,WebFetch"));
    // No user, project or local settings: none of their allow rules or hooks.
    assert_eq!(value_of("--setting-sources"), Some(""));
    // No MCP servers: strict, with no --mcp-config to take them from.
    assert!(args.iter().any(|a| a == "--strict-mcp-config"));
    assert!(!args.iter().any(|a| a.starts_with("--mcp-config")));
    assert!(!args.iter().any(|a| a.starts_with("--settings")));
}

#[test]
fn the_step_cap_comes_from_the_profile_and_has_a_default() {
    assert_eq!(AgentSettings::default().max_turns, DEFAULT_MAX_TURNS);
    let runner = FakeRunner::printing(&fixture("claude-code/success.json"), 0);
    ask(&runner, DEFAULT_MAX_TURNS).unwrap();
    let args = &runner.commands.borrow()[0].args;
    let cap = args.iter().position(|a| a == "--max-turns").unwrap();
    assert_eq!(args[cap + 1], DEFAULT_MAX_TURNS.to_string());
    assert_eq!(
        agent::from_settings(&AgentSettings::default()).name(),
        "claude-code"
    );
}

#[test]
fn a_recorded_result_parses_into_judgements() {
    let runner = FakeRunner::printing(&fixture("claude-code/success.json"), 0);
    let candidates = [
        candidate("Ghent Build Weekend"),
        candidate("Liège Data Days"),
    ];
    let ids = agent::numbered_ids(2);
    let profile = test_profile();
    let request = JudgeRequest::new(&candidates, &ids, &profile);

    let answer = ClaudeCode::new(&runner, 20).judge(&request).unwrap();
    let judged = request.read_judgements(&answer).unwrap();

    let judged: Vec<(&str, Judgement)> = judged
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
                    deadline: Some(date(2026, 11, 28)),
                    status: ApplicationStatus::Open,
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
                    deadline: None,
                    status: ApplicationStatus::OpenUntilFull,
                    reason: "Participants build data projects on open city data during the two days.".into(),
                }
            ),
        ]
    );
}

#[test]
fn the_deadline_and_status_are_required_in_structured_output_and_the_deadline_may_be_null() {
    let candidates = [
        candidate("Ghent Build Weekend"),
        candidate("Liège Data Days"),
    ];
    let ids = agent::numbered_ids(2);
    let profile = test_profile();
    let request = JudgeRequest::new(&candidates, &ids, &profile);
    let read = |change: &dyn Fn(&mut serde_json::Value)| {
        let mut result: serde_json::Value =
            serde_json::from_str(&fixture("claude-code/success.json")).unwrap();
        change(&mut result["structured_output"]["judgements"][0]);
        let runner = FakeRunner::printing(&result.to_string(), 0);
        let answer = ClaudeCode::new(&runner, 20).judge(&request).unwrap();
        request
            .read_judgements(&answer)
            .map(|judged| judged[0].1.clone())
    };

    let nulled = read(&|item| item["application_deadline"] = serde_json::Value::Null).unwrap();
    assert_eq!(nulled.deadline, None);
    for key in ["application_deadline", "application_status"] {
        let err = read(&|item| {
            item.as_object_mut().unwrap().remove(key);
        })
        .unwrap_err();
        assert!(err.to_string().contains(key), "{key}: {err}");
    }
    assert!(read(&|item| item["application_status"] = serde_json::Value::Null).is_err());
    assert!(read(&|item| item["application_status"] = "soon".into()).is_err());
    assert!(read(&|item| item["application_deadline"] = "28/11/2026".into()).is_err());
}

#[test]
fn the_result_text_is_used_when_there_is_no_structured_output() {
    let mut result: serde_json::Value =
        serde_json::from_str(&fixture("claude-code/success.json")).unwrap();
    result.as_object_mut().unwrap().remove("structured_output");
    let runner = FakeRunner::printing(&result.to_string(), 0);

    let answer = ask(&runner, 20).unwrap();

    assert!(answer.contains("on site in Ghent"), "{answer}");
}

#[test]
fn structured_output_that_does_not_match_the_schema_is_rejected() {
    let mut result: serde_json::Value =
        serde_json::from_str(&fixture("claude-code/success.json")).unwrap();
    result["structured_output"]["judgements"][0]["travel_funded"] = "yes".into();
    let runner = FakeRunner::printing(&result.to_string(), 0);
    let candidates = [
        candidate("Ghent Build Weekend"),
        candidate("Liège Data Days"),
    ];
    let ids = agent::numbered_ids(2);
    let profile = test_profile();
    let request = JudgeRequest::new(&candidates, &ids, &profile);

    let answer = ClaudeCode::new(&runner, 20).judge(&request).unwrap();

    let err = request.read_judgements(&answer).unwrap_err();
    assert!(
        err.to_string()
            .contains("doesn't match the judgements schema"),
        "{err}"
    );
}

#[test]
fn running_out_of_steps_is_an_error_that_names_the_setting() {
    let runner = FakeRunner::printing(&fixture("claude-code/max-turns.json"), 1);
    let err = ask(&runner, 12).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("used all 12 steps"), "{message}");
    assert!(message.contains("agent.max_turns"), "{message}");
}

#[test]
fn a_signed_out_claude_is_an_error_that_says_to_sign_in_yourself() {
    // Recorded from `claude -p` whose login had expired.
    let runner = FakeRunner::printing(&fixture("claude-code/not-signed-in.json"), 1);
    let err = ask(&runner, 20).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("Failed to authenticate"), "{message}");
    assert!(message.contains("run `claude` once yourself"), "{message}");
}

#[test]
fn output_that_is_not_a_result_is_an_error() {
    let runner = FakeRunner {
        reply: Ok(Output {
            code: Some(1),
            stdout: String::new(),
            stderr: "Error: --json-schema is not a valid JSON Schema".into(),
        }),
        commands: RefCell::new(Vec::new()),
    };
    let err = ask(&runner, 20).unwrap_err();
    let message = format!("{err:#}");
    assert!(
        message.contains("gave no JSON result (exit code 1)"),
        "{message}"
    );
    assert!(message.contains("not a valid JSON Schema"), "{message}");
}

#[test]
fn a_timeout_or_missing_claude_is_passed_on() {
    let runner = FakeRunner::failing("`claude` ran for longer than 1800 seconds and was stopped");
    let err = ask(&runner, 20).unwrap_err();
    assert!(format!("{err:#}").contains("was stopped"));
}

#[test]
fn a_whole_run_through_the_adapter_keeps_rejected_candidates_for_the_next_run() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let signed_out = FakeRunner::printing(&fixture("claude-code/not-signed-in.json"), 1);
    let claude = ClaudeCode::new(&signed_out, 20);

    let outcome = harness.run_with_agent(
        &claude,
        &[hackathon_be_from_fixture()],
        date(2026, 11, 9),
        DRY_RUN,
    );

    assert!(
        outcome
            .agent_failure
            .unwrap()
            .contains("Failed to authenticate")
    );
    assert!(harness.stored_names().is_empty());
    assert_eq!(harness.unjudged_names().len(), 5);
}
