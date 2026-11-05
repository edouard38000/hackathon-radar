//! Seam 2: the generic adapter, which runs any coding agent from a command
//! template in the Profile, with a fake process runner instead of a real agent.

mod support;

use std::cell::RefCell;
use std::path::PathBuf;

use hackathon_radar::agent::command::{self, CommandTemplate, Generic};
use hackathon_radar::agent::{self, Agent, JudgeRequest, SCHEMA};
use hackathon_radar::candidate::Candidate;
use hackathon_radar::process::{Command, Output, ProcessRunner};
use hackathon_radar::profile::{AgentAdapter, AgentSettings, Profile};
use support::*;

/// Records each command, and what the file named by its `schema_arg`-th
/// argument held while it ran, and answers with canned output.
struct FakeRunner {
    reply: Result<Output, String>,
    schema_arg: Option<usize>,
    commands: RefCell<Vec<Command>>,
    schema_texts: RefCell<Vec<String>>,
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
            schema_arg: None,
            commands: RefCell::new(Vec::new()),
            schema_texts: RefCell::new(Vec::new()),
        }
    }

    fn reading_schema_at(mut self, arg: usize) -> Self {
        self.schema_arg = Some(arg);
        self
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
        if let Some(at) = self.schema_arg {
            let text = std::fs::read_to_string(&command.args[at]).unwrap_or_default();
            self.schema_texts.borrow_mut().push(text);
        }
        self.reply.clone().map_err(anyhow::Error::msg)
    }
}

fn template(args: &[&str]) -> CommandTemplate {
    CommandTemplate::parse(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>()).unwrap()
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

/// Runs `agent` on two Candidates and returns its answer and the prompt it was sent.
fn ask(agent: &dyn Agent) -> (anyhow::Result<String>, String) {
    let candidates = [
        candidate("Ghent Build Weekend"),
        candidate("Liège Data Days"),
    ];
    let ids = agent::numbered_ids(candidates.len());
    let profile = test_profile();
    let request = JudgeRequest::new(&candidates, &ids, &profile);
    (agent.judge(&request), request.prompt.clone())
}

/// An answer judging Candidates `1` to `count` as Hackathons the person can attend.
fn approving_answer(count: usize) -> String {
    let items: Vec<serde_json::Value> = agent::numbered_ids(count)
        .into_iter()
        .map(|id| {
            serde_json::json!({
                "id": id, "is_hackathon": true, "eligible": true,
                "eligibility_restriction": null, "travel_funded": "full",
                "travel_detail": "Flights and two hotel nights", "reason": "A 24-hour build event.",
            })
        })
        .collect();
    serde_json::json!({ "judgements": items }).to_string()
}

#[test]
fn runs_the_template_with_the_schema_file_and_the_prompt_on_stdin() {
    let runner = FakeRunner::printing(&approving_answer(2), 0).reading_schema_at(2);
    let agent = Generic::new(
        &runner,
        template(&["my-agent", "--json", "--schema", "{schema_file}"]),
        12,
    );

    let (answer, prompt) = ask(&agent);

    assert_eq!(answer.unwrap(), approving_answer(2));
    let command = runner.command();
    assert_eq!(command.program, "my-agent");
    let schema_file = PathBuf::from(&command.args[2]);
    assert_eq!(
        command.args,
        ["--json", "--schema", schema_file.to_str().unwrap()]
    );
    assert!(
        schema_file.ends_with("judgements.schema.json"),
        "{schema_file:?}"
    );
    assert_eq!(
        *runner.schema_texts.borrow(),
        [SCHEMA],
        "the schema file holds the built-in schema while the agent runs"
    );
    assert_eq!(command.stdin, prompt, "the prompt goes on stdin");
    assert_eq!(command.timeout, command::DEFAULT_TIMEOUT);
    let dir = command.working_dir.as_deref().expect("a working directory");
    assert_ne!(dir, std::env::current_dir().unwrap());
    assert!(!schema_file.starts_with(dir));
    assert!(!dir.exists(), "the working directory is removed afterwards");
    assert!(
        !schema_file.exists(),
        "the schema file is removed afterwards"
    );
}

#[test]
fn a_prompt_placeholder_puts_the_prompt_in_one_argument_and_leaves_stdin_empty() {
    let runner = FakeRunner::printing(&approving_answer(2), 0);
    let agent = Generic::new(
        &runner,
        template(&[
            "/opt/agents/my agent",
            "--steps={max_turns}",
            "--schema",
            "{schema}",
            "--prompt",
            "{prompt}",
        ]),
        7,
    );

    let (_, prompt) = ask(&agent);

    let command = runner.command();
    assert_eq!(command.program, "/opt/agents/my agent");
    assert_eq!(
        command.args,
        ["--steps=7", "--schema", SCHEMA, "--prompt", prompt.as_str()]
    );
    assert!(prompt.contains('\n') && prompt.contains(' '));
    assert_eq!(command.stdin, "");
    assert_eq!(agent.name(), "command:my agent");
}

#[test]
fn arguments_are_passed_as_written_with_no_shell_to_split_or_expand_them() {
    let runner = FakeRunner::printing(&approving_answer(2), 0);
    let written = [
        "my-agent",
        "--note",
        "two words",
        "$(touch /tmp/pwned)",
        "*",
        "a;b|c && d",
        "~/x",
        "'quoted'",
        "",
        "{{literal}}",
    ];
    let agent = Generic::new(&runner, template(&written), 3);

    ask(&agent).0.unwrap();

    assert_eq!(
        runner.command().args,
        [
            "--note",
            "two words",
            "$(touch /tmp/pwned)",
            "*",
            "a;b|c && d",
            "~/x",
            "'quoted'",
            "",
            "{literal}",
        ]
    );
}

#[cfg(unix)]
#[test]
fn a_real_program_receives_each_argument_untouched() {
    // `printf` stands in for an agent here; no shell is involved in starting it.
    let template = template(&[
        "printf",
        "<%s>",
        "two words",
        "$(echo injected)",
        "*",
        "{max_turns}",
    ]);
    let agent = Generic::new(hackathon_radar::process::SystemRunner, template, 4);

    let (answer, _) = ask(&agent);

    assert_eq!(answer.unwrap(), "<two words><$(echo injected)><*><4>");
}

#[test]
fn the_profile_selects_the_command_adapter() {
    let profile = Profile::from_toml(
        "countries = [\"Belgium\"]\n[home]\ncity = \"Brussels\"\nairports = [\"BRU\"]\n\
         [agent]\nadapter = \"command\"\ncommand = [\"/usr/local/bin/my-agent\", \"{schema_file}\"]\n",
        std::path::Path::new("test-profile.toml"),
    )
    .unwrap();
    assert_eq!(profile.agent.adapter, AgentAdapter::Command);
    assert_eq!(
        agent::from_settings(&profile.agent).name(),
        "command:my-agent"
    );
    assert_eq!(
        agent::from_settings(&profile.agent).warning(),
        None,
        "the command adapter needs no experimental opt-in"
    );
    assert_eq!(
        agent::from_settings(&AgentSettings::default()).name(),
        "claude-code",
        "Claude Code stays the default"
    );
}

#[test]
fn output_that_does_not_match_the_schema_is_rejected() {
    let runner = FakeRunner::printing("Sure! Both look like great hackathons.", 0);
    let agent = Generic::new(&runner, template(&["my-agent"]), 3);
    let candidates = [
        candidate("Ghent Build Weekend"),
        candidate("Liège Data Days"),
    ];
    let ids = agent::numbered_ids(2);
    let profile = test_profile();
    let request = JudgeRequest::new(&candidates, &ids, &profile);

    let answer = agent.judge(&request).unwrap();

    let err = request.read_judgements(&answer).unwrap_err();
    assert!(
        err.to_string()
            .contains("doesn't match the judgements schema"),
        "{err}"
    );
}

#[test]
fn a_failing_command_is_an_error_that_shows_stderr() {
    let runner = FakeRunner::replying(Ok(Output {
        code: Some(3),
        stdout: approving_answer(2),
        stderr: "my-agent: not signed in".into(),
    }));
    let agent = Generic::new(&runner, template(&["my-agent"]), 3);
    let message = format!("{:#}", ask(&agent).0.unwrap_err());
    assert!(
        message.contains("`my-agent` failed (exit code 3)"),
        "{message}"
    );
    assert!(message.contains("not signed in"), "{message}");
}

#[test]
fn printing_nothing_is_an_error() {
    let runner = FakeRunner::printing("  \n", 0);
    let agent = Generic::new(&runner, template(&["my-agent"]), 3);
    let message = format!("{:#}", ask(&agent).0.unwrap_err());
    assert!(message.contains("printed no answer"), "{message}");
}

#[test]
fn a_timeout_or_missing_program_is_passed_on() {
    let runner = FakeRunner::replying(Err(
        "`my-agent` ran for longer than 1800 seconds and was stopped".into(),
    ));
    let agent = Generic::new(&runner, template(&["my-agent"]), 3);
    assert!(format!("{:#}", ask(&agent).0.unwrap_err()).contains("was stopped"));
}

#[test]
fn a_whole_run_through_the_command_stores_what_it_judged() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    let runner = FakeRunner::printing(&approving_answer(5), 0);
    let agent = Generic::new(&runner, template(&["my-agent", "{schema_file}"]), 20);

    let outcome = harness.run_with_agent(
        &agent,
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
            .all(|j| j.agent == "command:my-agent")
    );
}

#[test]
fn a_whole_run_through_the_command_keeps_rejected_candidates_for_the_next_run() {
    let mut harness = Harness::new(profile(&["Belgium"]));
    // Judges a Candidate that was never sent, so the whole answer is rejected.
    let runner = FakeRunner::printing(&approving_answer(6), 0);
    let agent = Generic::new(&runner, template(&["my-agent"]), 20);

    let outcome = harness.run_with_agent(
        &agent,
        &[hackathon_be_from_fixture()],
        date(2026, 11, 9),
        DRY_RUN,
    );

    let failure = outcome.agent_failure.expect("the answer is rejected");
    assert!(failure.contains("\"6\", which was not sent"), "{failure}");
    assert!(harness.stored_names().is_empty());
    assert_eq!(harness.unjudged_names().len(), 5);
}
