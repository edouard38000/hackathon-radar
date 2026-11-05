//! Any other coding agent as the Agent, run from a command template in the
//! Profile (ADR 0004):
//!
//! ```toml
//! [agent]
//! adapter = "command"
//! command = ["my-agent", "--json", "--schema", "{schema_file}"]
//! ```
//!
//! The command runs without a shell, in an empty working directory, with the
//! prompt on stdin unless the template places `{prompt}` itself. It must print
//! JSON matching the schema on stdout, and nothing else. Making the agent
//! read-only (web search and page reading only) is up to the person; the README
//! says so. Its answer is checked against the schema like any Agent's.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use super::{Agent, JudgeRequest, Scratch, exit, tail};
use crate::process::{Command, Output, ProcessRunner};

/// How long one request may take before the Agent is stopped.
pub const DEFAULT_TIMEOUT: Duration = super::DEFAULT_TIMEOUT;

/// What a template may refer to, each written in braces, such as `{schema_file}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    /// The path of a file holding the JSON schema.
    SchemaFile,
    /// The JSON schema itself.
    Schema,
    /// The whole prompt. Without it, the prompt goes on stdin.
    Prompt,
    /// The Profile's `agent.max_turns`.
    MaxTurns,
}

impl Placeholder {
    pub const ALL: [Placeholder; 4] = [
        Placeholder::SchemaFile,
        Placeholder::Schema,
        Placeholder::Prompt,
        Placeholder::MaxTurns,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Placeholder::SchemaFile => "schema_file",
            Placeholder::Schema => "schema",
            Placeholder::Prompt => "prompt",
            Placeholder::MaxTurns => "max_turns",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// What each placeholder becomes for one request.
pub struct Values<'a> {
    pub schema_file: &'a Path,
    pub schema: &'a str,
    pub prompt: &'a str,
    pub max_turns: u32,
}

impl Values<'_> {
    fn get(&self, placeholder: Placeholder) -> String {
        match placeholder {
            Placeholder::SchemaFile => self.schema_file.to_string_lossy().into_owned(),
            Placeholder::Schema => self.schema.to_owned(),
            Placeholder::Prompt => self.prompt.to_owned(),
            Placeholder::MaxTurns => self.max_turns.to_string(),
        }
    }
}

/// A command from the Profile: the program, then its arguments, each of which
/// may hold placeholders. `{{` and `}}` stand for literal braces.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Vec<String>")]
pub struct CommandTemplate {
    program: String,
    args: Vec<Vec<Piece>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Slot(Placeholder),
}

/// Why a command template was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("agent.command: {0}")]
pub struct TemplateError(String);

impl CommandTemplate {
    pub fn parse(command: &[String]) -> Result<Self, TemplateError> {
        let Some((program, args)) = command.split_first() else {
            return Err(TemplateError(
                "the command is empty; give the program to run, then its arguments, \
                 such as [\"my-agent\", \"--schema\", \"{schema_file}\"]"
                    .to_owned(),
            ));
        };
        if program.trim().is_empty() {
            return Err(TemplateError(
                "the first entry, the program to run, is empty".to_owned(),
            ));
        }
        if program.contains(['{', '}']) {
            return Err(TemplateError(format!(
                "the first entry, the program to run, can't hold a placeholder: {program:?}"
            )));
        }
        let args = args
            .iter()
            .map(|arg| pieces(arg))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            program: program.clone(),
            args,
        })
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    /// Whether the template places the prompt itself, so stdin stays empty.
    pub fn uses(&self, placeholder: Placeholder) -> bool {
        self.args
            .iter()
            .flatten()
            .any(|piece| *piece == Piece::Slot(placeholder))
    }

    /// The arguments with every placeholder filled in. Each stays one argument,
    /// whatever it holds: there is no shell to split or interpret it.
    pub fn expand(&self, values: &Values<'_>) -> Vec<String> {
        self.args
            .iter()
            .map(|arg| {
                arg.iter()
                    .map(|piece| match piece {
                        Piece::Text(text) => text.clone(),
                        Piece::Slot(placeholder) => values.get(*placeholder),
                    })
                    .collect()
            })
            .collect()
    }
}

impl TryFrom<Vec<String>> for CommandTemplate {
    type Error = TemplateError;

    fn try_from(command: Vec<String>) -> Result<Self, Self::Error> {
        Self::parse(&command)
    }
}

/// Splits one argument into text and placeholders.
fn pieces(arg: &str) -> Result<Vec<Piece>, TemplateError> {
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut chars = arg.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => name.push(c),
                        None => {
                            return Err(TemplateError(format!(
                                "{arg:?} opens a placeholder with {{ but never closes it; \
                                 write {{{{ for a literal brace"
                            )));
                        }
                    }
                }
                let Some(placeholder) = Placeholder::parse(&name) else {
                    return Err(TemplateError(format!(
                        "{{{name}}} in {arg:?} is not a placeholder; the placeholders are {} \
                         (write {{{{ and }}}} for literal braces)",
                        Known
                    )));
                };
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Slot(placeholder));
            }
            '}' => {
                return Err(TemplateError(format!(
                    "{arg:?} has a }} that closes nothing; write }}}} for a literal brace"
                )));
            }
            c => text.push(c),
        }
    }
    if !text.is_empty() || pieces.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(pieces)
}

/// The known placeholders, for error messages.
struct Known;

impl fmt::Display for Known {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<String> = Placeholder::ALL
            .iter()
            .map(|p| format!("{{{}}}", p.name()))
            .collect();
        f.write_str(&names.join(", "))
    }
}

/// The Agent run from a [`CommandTemplate`].
pub struct Generic<R> {
    runner: R,
    template: CommandTemplate,
    max_turns: u32,
    timeout: Duration,
    name: String,
}

impl<R: ProcessRunner> Generic<R> {
    pub fn new(runner: R, template: CommandTemplate, max_turns: u32) -> Self {
        let program = Path::new(template.program())
            .file_name()
            .map_or_else(|| template.program().into(), |n| n.to_string_lossy());
        let name = format!("command:{program}");
        Self {
            runner,
            template,
            max_turns,
            timeout: DEFAULT_TIMEOUT,
            name,
        }
    }

    /// The command for one request, with `schema_file` as the schema's path.
    pub fn command(
        &self,
        request: &JudgeRequest<'_>,
        schema_file: &Path,
        working_dir: &Path,
    ) -> Command {
        let args = self.template.expand(&Values {
            schema_file,
            schema: request.schema,
            prompt: &request.prompt,
            max_turns: self.max_turns,
        });
        let stdin = if self.template.uses(Placeholder::Prompt) {
            String::new()
        } else {
            request.prompt.clone()
        };
        Command {
            program: self.template.program().to_owned(),
            args,
            stdin,
            working_dir: Some(working_dir.to_owned()),
            timeout: self.timeout,
        }
    }

    /// The answer is everything the command printed on stdout.
    pub fn answer(&self, output: &Output) -> anyhow::Result<String> {
        let program = self.template.program();
        if !output.success() {
            anyhow::bail!(
                "`{program}` failed ({}): {}",
                exit(output),
                tail(&output.stderr)
            );
        }
        if output.stdout.trim().is_empty() {
            anyhow::bail!(
                "`{program}` printed no answer on stdout: {}",
                tail(&output.stderr)
            );
        }
        Ok(output.stdout.clone())
    }
}

impl<R: ProcessRunner> Agent for Generic<R> {
    fn name(&self) -> &str {
        &self.name
    }

    fn judge(&self, request: &JudgeRequest<'_>) -> anyhow::Result<String> {
        let scratch = Scratch::new(request.schema)?;
        let command = self.command(request, &scratch.schema_file, scratch.work.path());
        let output = self.runner.run(&command)?;
        self.answer(&output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(args: &[&str]) -> Result<CommandTemplate, TemplateError> {
        CommandTemplate::parse(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    fn values() -> Values<'static> {
        Values {
            schema_file: Path::new("/tmp/x/judgements.schema.json"),
            schema: "{\"type\": \"object\"}",
            prompt: "Judge these, please.",
            max_turns: 9,
        }
    }

    #[test]
    fn fills_every_placeholder_inside_or_as_a_whole_argument() {
        let t = template(&[
            "agent",
            "--schema={schema_file}",
            "{schema}",
            "--steps",
            "{max_turns}",
            "-p",
            "{prompt}",
            "{max_turns}{max_turns}",
        ])
        .unwrap();
        assert_eq!(t.program(), "agent");
        assert_eq!(
            t.expand(&values()),
            [
                "--schema=/tmp/x/judgements.schema.json",
                "{\"type\": \"object\"}",
                "--steps",
                "9",
                "-p",
                "Judge these, please.",
                "99",
            ]
        );
        assert!(t.uses(Placeholder::Prompt));
    }

    #[test]
    fn doubled_braces_are_literal_and_empty_arguments_stay() {
        let t = template(&["agent", "{{\"a\": 1}}", "", "{{schema_file}}"]).unwrap();
        assert_eq!(t.expand(&values()), ["{\"a\": 1}", "", "{schema_file}"]);
        assert!(!t.uses(Placeholder::SchemaFile));
        assert!(!t.uses(Placeholder::Prompt));
    }

    #[test]
    fn bad_templates_are_refused_with_the_reason() {
        let cases: [(&[&str], &str); 6] = [
            (&[], "the command is empty"),
            (&[" "], "the program to run, is empty"),
            (&["{prompt}"], "can't hold a placeholder"),
            (
                &["agent", "--schema={schema_path}"],
                "{schema_path} in \"--schema={schema_path}\" is not a placeholder; the \
                 placeholders are {schema_file}, {schema}, {prompt}, {max_turns}",
            ),
            (&["agent", "{prompt"], "never closes it"),
            (&["agent", "a}b"], "closes nothing"),
        ];
        for (args, expected) in cases {
            let err = template(args).unwrap_err().to_string();
            assert!(err.starts_with("agent.command: "), "{err}");
            assert!(err.contains(expected), "{args:?}: {err}");
        }
    }
}
