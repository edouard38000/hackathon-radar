//! Codex as the Agent, run headless with `codex exec` in a read-only sandbox
//! (ADR 0004).
//!
//! It runs with the person's own `codex` login. When `CODEX_API_KEY` is set in
//! the environment, `codex exec` itself picks it up; hackathon-radar neither
//! removes it nor sets it, and never signs anyone in.
//!
//! Codex has no step cap of its own, so neither `agent.max_turns` nor
//! `agent.travel_max_turns` (a request's `step_cap`) applies to it;
//! the time limit does.
//!
//! Experimental: the read-only lockdown below comes from OpenAI's docs and is
//! unverified against a real Codex CLI, so a Profile must opt in with
//! `experimental = true`, and every Run prints [`WARNING`].

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use super::{Agent, JudgeRequest, Scratch, exit, tail};
use crate::process::{Command, Output, ProcessRunner};

pub const PROGRAM: &str = "codex";

/// How long one request may take before the Agent is stopped.
pub const DEFAULT_TIMEOUT: Duration = super::DEFAULT_TIMEOUT;

/// Printed on every Run that uses Codex, until a recorded run confirms its flags.
pub const WARNING: &str = "the Codex adapter is experimental: its read-only lockdown is \
     unverified against a real Codex CLI (you opted in with agent.experimental = true)";

const SIGN_IN_HINT: &str = "If Codex isn't signed in, run `codex login` once yourself with your \
     own account, or set CODEX_API_KEY.";

pub struct Codex<R> {
    runner: R,
    timeout: Duration,
}

impl<R: ProcessRunner> Codex<R> {
    pub fn new(runner: R) -> Self {
        Self {
            runner,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// The whole argument list (ADR 0004). The session can search the web and
    /// read pages and nothing else, and never gets a writable sandbox:
    ///
    /// - `--sandbox read-only` with `approval_policy="never"`: nothing may be
    ///   written, and nothing is ever escalated or approved.
    /// - `features.shell_tool=false` and `features.view_image=false` take away
    ///   the command and local-image tools, so no shell exists at all.
    /// - `web_search="live"` turns on live web search (what `--search` does in
    ///   the interactive CLI), which also opens pages.
    /// - `--ignore-user-config` loads none of the person's `config.toml` (so none
    ///   of their MCP servers, sandbox or approval settings) and `--ignore-rules`
    ///   none of their execpolicy rules. Their sign-in still works.
    /// - `--ephemeral` keeps no session files; `--skip-git-repo-check` lets it
    ///   run in the empty, non-Git working directory.
    /// - `--json` prints events as JSON Lines; the prompt is read from stdin (`-`).
    pub fn args(&self, schema_file: &Path) -> Vec<String> {
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
            &schema_file.to_string_lossy(),
            "--json",
            "-",
        ]
        .map(str::to_owned)
        .to_vec()
    }

    /// Reads the answer out of `codex exec --json`'s events: the text of the
    /// last `agent_message` item, which `--output-schema` makes JSON.
    pub fn answer(&self, output: &Output) -> anyhow::Result<String> {
        let events: Vec<Event> = output
            .stdout
            .lines()
            .filter_map(|line| serde_json::from_str(line.trim()).ok())
            .collect();
        if events.is_empty() {
            anyhow::bail!(
                "Codex gave no JSON events ({}): {}\n{SIGN_IN_HINT}",
                exit(output),
                tail(&output.stderr)
            );
        }
        let mut answer = None;
        let mut turn_failure = None;
        let mut error = None;
        for event in events {
            match event {
                Event::ItemCompleted {
                    item: Item::AgentMessage { text },
                } => answer = Some(text),
                Event::TurnFailed { error: e } => turn_failure = Some(e.message),
                Event::Error { message } => error = Some(message),
                _ => {}
            }
        }
        let succeeded = output.success() && turn_failure.is_none();
        match answer {
            Some(text) if succeeded && !text.trim().is_empty() => Ok(text),
            _ if succeeded && error.is_none() => {
                anyhow::bail!("Codex finished without an answer")
            }
            _ => {
                // An `error` event alone can be a retry Codex recovered from, so
                // it only explains a Run that also failed or gave no answer.
                let detail = turn_failure
                    .or(error)
                    .filter(|m| !m.trim().is_empty())
                    .unwrap_or_else(|| tail(&output.stderr));
                anyhow::bail!("Codex failed ({}): {detail}\n{SIGN_IN_HINT}", exit(output))
            }
        }
    }
}

impl<R: ProcessRunner> Agent for Codex<R> {
    fn name(&self) -> &str {
        "codex"
    }

    fn warning(&self) -> Option<&str> {
        Some(WARNING)
    }

    fn judge(&self, request: &JudgeRequest<'_>) -> anyhow::Result<String> {
        let scratch = Scratch::new(request.schema)?;
        let output = self.runner.run(&Command {
            program: PROGRAM.to_owned(),
            args: self.args(&scratch.schema_file),
            stdin: request.prompt.clone(),
            working_dir: Some(scratch.work.path().to_owned()),
            timeout: self.timeout,
            env: Vec::new(),
        })?;
        self.answer(&output)
    }
}

// The events of `codex exec --json` that matter here; others are skipped.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum Event {
    #[serde(rename = "item.completed")]
    ItemCompleted { item: Item },
    #[serde(rename = "turn.failed")]
    TurnFailed { error: EventError },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Item {
    #[serde(rename = "agent_message")]
    AgentMessage { text: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct EventError {
    message: String,
}
