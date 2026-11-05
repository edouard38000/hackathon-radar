//! Claude Code as the Agent, run headless with `claude -p` (ADR 0004).
//!
//! It runs with the person's own `claude` login. When `ANTHROPIC_API_KEY` is set
//! in the environment, `claude` itself picks it up; hackathon-radar neither
//! removes it nor sets it, and never signs anyone in.

use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use serde_json::Value;

use super::{Agent, JudgeRequest, StepCap, exit, tail};
use crate::process::{Command, Output, ProcessRunner};

pub const PROGRAM: &str = "claude";

/// The only tools the Agent gets: web search and page reading (ADR 0004).
pub const ALLOWED_TOOLS: &str = "WebSearch,WebFetch";

/// How long one request may take before the Agent is stopped.
pub const DEFAULT_TIMEOUT: Duration = super::DEFAULT_TIMEOUT;

const SIGN_IN_HINT: &str = "If Claude Code isn't signed in, run `claude` once yourself and sign in \
     with your own account, or set ANTHROPIC_API_KEY.";

pub struct ClaudeCode<R> {
    runner: R,
    max_turns: u32,
    timeout: Duration,
}

impl<R: ProcessRunner> ClaudeCode<R> {
    pub fn new(runner: R, max_turns: u32) -> Self {
        Self {
            runner,
            max_turns,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// The whole argument list (ADR 0004). The session has web search and page
    /// reading and nothing else, and never bypasses permissions:
    ///
    /// - `--tools` makes those two the only built-in tools that exist, and
    ///   `--allowedTools` approves them; `dontAsk` refuses anything else.
    /// - `--strict-mcp-config` without `--mcp-config` loads no MCP servers.
    /// - `--setting-sources ""` loads no user, project or local settings, so the
    ///   person's own allow rules and hooks don't apply. Their sign-in still does.
    pub fn args(&self, schema: &str) -> Vec<String> {
        self.args_with_cap(schema, self.max_turns)
    }

    /// [`Self::args`] with a request's own step cap, such as the travel request's.
    pub fn args_with_cap(&self, schema: &str, max_turns: u32) -> Vec<String> {
        [
            "-p",
            "--permission-mode",
            "dontAsk",
            "--tools",
            ALLOWED_TOOLS,
            "--allowedTools",
            ALLOWED_TOOLS,
            "--strict-mcp-config",
            "--setting-sources",
            "",
            "--max-turns",
            &max_turns.to_string(),
            "--output-format",
            "json",
            "--json-schema",
            schema,
        ]
        .map(str::to_owned)
        .to_vec()
    }

    /// Reads the answer out of `claude -p --output-format json`'s result: its
    /// `structured_output`, or else its `result` text.
    pub fn answer(&self, output: &Output) -> anyhow::Result<String> {
        self.answer_with_cap(output, self.cap())
    }

    /// The adapter's own step cap, the Profile's `agent.max_turns`.
    fn cap(&self) -> StepCap {
        StepCap {
            max_turns: self.max_turns,
            setting: "agent.max_turns",
        }
    }

    fn answer_with_cap(&self, output: &Output, cap: StepCap) -> anyhow::Result<String> {
        let result: Option<ResultMessage> = serde_json::from_str(output.stdout.trim()).ok();
        let Some(result) = result else {
            anyhow::bail!(
                "Claude Code gave no JSON result ({}): {}\n{SIGN_IN_HINT}",
                exit(output),
                tail(&output.stderr)
            );
        };
        if result.subtype.as_deref() == Some("error_max_turns") {
            anyhow::bail!(
                "Claude Code used all {} steps before answering; raise {} in your Profile \
                 if this keeps happening",
                cap.max_turns,
                cap.setting
            );
        }
        if result.is_error || !output.success() || result.subtype.as_deref() != Some("success") {
            let detail = result
                .result
                .as_deref()
                .filter(|r| !r.trim().is_empty())
                .map_or_else(|| tail(&output.stderr), str::to_owned);
            // `claude` reports some errors, such as a failed sign-in, with subtype `success`.
            let kind = match result.subtype.as_deref() {
                Some(subtype) if subtype != "success" => format!("{subtype}, "),
                _ => String::new(),
            };
            anyhow::bail!(
                "Claude Code failed ({kind}{}): {detail}\n{SIGN_IN_HINT}",
                exit(output)
            );
        }
        match (result.structured_output, result.result) {
            (Some(answer), _) if !answer.is_null() => Ok(answer.to_string()),
            (_, Some(text)) => Ok(text),
            _ => anyhow::bail!("Claude Code's result holds no answer"),
        }
    }
}

impl<R: ProcessRunner> Agent for ClaudeCode<R> {
    fn name(&self) -> &str {
        "claude-code"
    }

    fn judge(&self, request: &JudgeRequest<'_>) -> anyhow::Result<String> {
        // An empty working directory, so no project's files or settings are in reach.
        let dir = tempfile::tempdir().context("creating a working directory for Claude Code")?;
        let cap = request.step_cap.unwrap_or_else(|| self.cap());
        let output = self.runner.run(&Command {
            program: PROGRAM.to_owned(),
            args: self.args_with_cap(request.schema, cap.max_turns),
            stdin: request.prompt.clone(),
            working_dir: Some(dir.path().to_owned()),
            timeout: self.timeout,
        })?;
        self.answer_with_cap(&output, cap)
    }
}

// The fields of `claude -p --output-format json`'s result that matter here.
#[derive(Deserialize)]
struct ResultMessage {
    subtype: Option<String>,
    #[serde(default)]
    is_error: bool,
    result: Option<String>,
    structured_output: Option<Value>,
}
