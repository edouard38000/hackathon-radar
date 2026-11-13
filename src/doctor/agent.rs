//! The Agent: its program is on PATH, Claude Code is signed in (`claude auth
//! status`, which costs nothing), and Codex is opted in to. No judging
//! request is ever made.

use std::time::Duration;

use serde::Deserialize;

use super::{Check, FindProgram, Section, agent_program, error_line};
use crate::agent::codex;
use crate::process::{Command, ProcessRunner};
use crate::profile::{AgentAdapter, Profile};

const S: Section = Section::Agent;

/// How long `claude auth status` may take.
const AUTH_STATUS_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct Context<'a> {
    pub shell_path: &'a str,
    pub find_program: FindProgram<'a>,
    pub runner: &'a dyn ProcessRunner,
}

pub(super) fn check(profile: &Profile, cx: &Context<'_>, checks: &mut Vec<Check>) {
    let adapter = profile.agent.adapter;
    let program = agent_program(profile);
    let found = (cx.find_program)(&program, cx.shell_path);
    checks.push(match &found {
        Some(path) => Check::pass(
            S,
            "agent.program",
            "Agent program",
            format!(
                "{} runs {program}, found at {}.",
                adapter_name(adapter),
                path.display()
            ),
        ),
        None => {
            let (reason, fix) = match adapter {
                AgentAdapter::ClaudeCode => (
                    "`claude` isn't on your PATH, so Runs can't ask Claude Code to judge.",
                    "Install Claude Code (https://claude.com/claude-code) so `claude` is on your \
                     PATH.",
                ),
                AgentAdapter::Codex => (
                    "`codex` isn't on your PATH, so Runs can't ask Codex to judge.",
                    "Install Codex so `codex` is on your PATH.",
                ),
                AgentAdapter::Command => (
                    "The command template's program isn't on your PATH or isn't executable, so \
                     Runs can't ask your Agent to judge.",
                    "Install it, or fix agent.command in your Profile (or run `hackathon-radar \
                     init --agent command --agent-command \"...\"`).",
                ),
            };
            Check::fail(
                S,
                "agent.program",
                "Agent program",
                format!("{reason} (looked for {program})"),
                fix,
            )
        }
    });

    match adapter {
        AgentAdapter::ClaudeCode => {
            if let Some(path) = &found {
                checks.push(sign_in(path.to_string_lossy().as_ref(), cx.runner));
            }
        }
        AgentAdapter::Codex => checks.push(if profile.agent.experimental {
            Check::warn(
                S,
                "agent.experimental",
                "Codex opt-in",
                format!("You opted in: {}.", codex::WARNING),
                "Nothing to do if you accept that; to switch, run `hackathon-radar init --agent \
                 claude-code`.",
            )
        } else {
            Check::fail(
                S,
                "agent.experimental",
                "Codex opt-in",
                "Codex is selected without the experimental opt-in, so Runs refuse it.",
                "Run `hackathon-radar init --agent codex --experimental`, or pick claude-code.",
            )
        }),
        AgentAdapter::Command => {}
    }
}

fn adapter_name(adapter: AgentAdapter) -> &'static str {
    match adapter {
        AgentAdapter::ClaudeCode => "Claude Code",
        AgentAdapter::Codex => "Codex",
        AgentAdapter::Command => "Your Agent's command",
    }
}

/// What `claude auth status` prints; everything else in it (such as the
/// account's email) is left alone.
#[derive(Deserialize)]
struct AuthStatus {
    #[serde(rename = "loggedIn")]
    logged_in: bool,
    #[serde(rename = "authMethod")]
    auth_method: Option<String>,
}

fn sign_in(claude: &str, runner: &dyn ProcessRunner) -> Check {
    const ID: &str = "agent.sign-in";
    const NAME: &str = "Claude Code sign-in";
    let output = runner.run(&Command {
        program: claude.to_owned(),
        args: vec!["auth".into(), "status".into()],
        stdin: String::new(),
        working_dir: None,
        timeout: AUTH_STATUS_TIMEOUT,
        env: Vec::new(),
    });
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            return Check::warn(
                S,
                ID,
                NAME,
                format!(
                    "Couldn't run `claude auth status`: {}.",
                    error_line(&error, None)
                ),
                "Run `claude auth status` yourself; if it isn't signed in, run `claude` and sign \
                 in.",
            );
        }
    };
    // It exits non-zero when signed out, with the same JSON.
    let status = output
        .stdout
        .find('{')
        .and_then(|start| serde_json::from_str::<AuthStatus>(output.stdout[start..].trim()).ok());
    match status {
        Some(AuthStatus {
            logged_in: true,
            auth_method,
        }) => {
            let how = auth_method
                .filter(|m| {
                    m.len() <= 32
                        && m.chars()
                            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                })
                .map(|m| format!(" ({m})"))
                .unwrap_or_default();
            Check::pass(S, ID, NAME, format!("Claude Code is signed in{how}."))
        }
        Some(AuthStatus {
            logged_in: false, ..
        }) => Check::fail(
            S,
            ID,
            NAME,
            "Claude Code isn't signed in, so Runs can't judge anything.",
            "Run `claude` once and sign in with your subscription or an Anthropic Console \
             account (or run `claude auth login`).",
        ),
        None => Check::warn(
            S,
            ID,
            NAME,
            "Couldn't tell whether Claude Code is signed in: `claude auth status` gave no answer \
             hackathon-radar understands.",
            "Update Claude Code, then check `claude auth status` yourself.",
        ),
    }
}
