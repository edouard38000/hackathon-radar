//! Claude Code as the Agent, run headless with `claude -p` (ADR 0004).
//!
//! It runs with the person's own `claude` login. When `ANTHROPIC_API_KEY` is set
//! in the environment, `claude` itself picks it up; hackathon-radar neither
//! removes it nor sets it, and never signs anyone in.

use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use serde_json::Value;

use super::{Agent, JudgeRequest};
use crate::process::{Command, Output, ProcessRunner};

pub const PROGRAM: &str = "claude";

/// The only tools the Agent gets: web search and page reading (ADR 0004).
pub const ALLOWED_TOOLS: &str = "WebSearch,WebFetch";

/// How long one request may take before the Agent is stopped.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30 * 60);

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

    /// The whole argument list. It allows web search and page reading only, and
    /// never bypasses permissions: `dontAsk` refuses any tool not listed.
    pub fn args(&self, schema: &str) -> Vec<String> {
        [
            "-p",
            "--permission-mode",
            "dontAsk",
            "--allowedTools",
            ALLOWED_TOOLS,
            "--max-turns",
            &self.max_turns.to_string(),
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
                "Claude Code used all {} steps before answering; raise agent.max_turns in your Profile \
                 if this keeps happening",
                self.max_turns
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
        let output = self.runner.run(&Command {
            program: PROGRAM.to_owned(),
            args: self.args(request.schema),
            stdin: request.prompt.clone(),
            working_dir: Some(dir.path().to_owned()),
            timeout: self.timeout,
        })?;
        self.answer(&output)
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

fn exit(output: &Output) -> String {
    match output.code {
        Some(code) => format!("exit code {code}"),
        None => "stopped by a signal".to_owned(),
    }
}

/// The end of a long error output, which is where the reason usually is.
fn tail(text: &str) -> String {
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= 500 {
        return text.to_owned();
    }
    format!("…{}", chars[chars.len() - 500..].iter().collect::<String>())
}
