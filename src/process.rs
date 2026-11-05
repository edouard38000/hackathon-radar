//! Running another program, such as an Agent's CLI, with a time limit. Agent
//! adapters take a [`ProcessRunner`] so tests can record the command instead of
//! starting it.

use std::fmt;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;

/// A program to run, with the text to write to its stdin.
#[derive(Clone, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: String,
    /// The directory it runs in; the current one when `None`.
    pub working_dir: Option<PathBuf>,
    /// Stopped and reported as an error once it runs this long.
    pub timeout: Duration,
    /// Variables set for the program on top of the environment it inherits.
    /// They may hold secrets, so `Debug` shows their names only.
    pub env: Vec<(String, String)>,
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env: Vec<&str> = self.env.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("Command")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("stdin", &self.stdin)
            .field("working_dir", &self.working_dir)
            .field("timeout", &self.timeout)
            .field("env", &env)
            .finish()
    }
}

/// What a finished program left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The exit code, or `None` when a signal ended it.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

pub trait ProcessRunner {
    /// Runs `command` to the end. An error means it couldn't start or ran out of
    /// time; a program that ran and failed is an `Ok` with its exit code.
    fn run(&self, command: &Command) -> anyhow::Result<Output>;
}

/// Starts real programs.
pub struct SystemRunner;

impl ProcessRunner for SystemRunner {
    fn run(&self, command: &Command) -> anyhow::Result<Output> {
        let mut process = std::process::Command::new(&command.program);
        process
            .args(&command.args)
            .envs(command.env.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &command.working_dir {
            process.current_dir(dir);
        }
        let mut child = process.spawn().with_context(|| {
            format!(
                "could not start `{}`; is it installed and on your PATH?",
                command.program
            )
        })?;

        // Write and read on threads so a full pipe can't stall either side.
        let mut stdin = child.stdin.take().expect("stdin is piped");
        let input = command.stdin.clone();
        let writer = thread::spawn(move || stdin.write_all(input.as_bytes()));
        let stdout = read_on_thread(child.stdout.take().expect("stdout is piped"));
        let stderr = read_on_thread(child.stderr.take().expect("stderr is piped"));

        let status = wait_with_timeout(&mut child, command.timeout).with_context(|| {
            format!(
                "`{}` ran for longer than {} seconds and was stopped",
                command.program,
                command.timeout.as_secs()
            )
        })?;
        // A program may exit without reading all its input; that's its business.
        let _ = writer.join();
        Ok(Output {
            code: status.code(),
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        })
    }
}

fn read_on_thread(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
) -> anyhow::Result<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("timed out");
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str, timeout: Duration) -> Command {
        Command {
            program: "sh".into(),
            args: vec!["-c".into(), script.into()],
            stdin: "hello from stdin".into(),
            working_dir: None,
            timeout,
            env: Vec::new(),
        }
    }

    #[test]
    fn passes_stdin_and_collects_both_outputs() {
        let output = SystemRunner
            .run(&sh("cat; echo oops >&2; exit 3", Duration::from_secs(10)))
            .unwrap();
        assert_eq!(
            output,
            Output {
                code: Some(3),
                stdout: "hello from stdin".into(),
                stderr: "oops\n".into(),
            }
        );
        assert!(!output.success());
    }

    #[test]
    fn a_program_that_runs_too_long_is_stopped() {
        let err = SystemRunner
            .run(&sh("sleep 5", Duration::from_millis(200)))
            .unwrap_err();
        assert!(format!("{err:#}").contains("was stopped"), "{err:#}");
    }

    #[test]
    fn sets_extra_variables_and_never_debug_prints_their_values() {
        let mut command = sh("printf %s \"$RADAR_TEST_VAR\"", Duration::from_secs(10));
        command.env = vec![("RADAR_TEST_VAR".into(), "s3cret-value".into())];
        let output = SystemRunner.run(&command).unwrap();
        assert_eq!(output.stdout, "s3cret-value");
        let debug = format!("{command:?}");
        assert!(debug.contains("RADAR_TEST_VAR"), "{debug}");
        assert!(!debug.contains("s3cret-value"), "{debug}");
    }

    #[test]
    fn a_missing_program_says_so() {
        let mut command = sh("", Duration::from_secs(1));
        command.program = "hackathon-radar-no-such-program".into();
        let err = SystemRunner.run(&command).unwrap_err();
        assert!(format!("{err:#}").contains("is it installed"), "{err:#}");
    }
}
