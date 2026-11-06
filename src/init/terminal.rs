//! Where `init` talks to the person: the real terminal, or scripted input and
//! captured output in tests.

use std::io::{self, BufRead, IsTerminal, Write};

/// The person at the other end of `init`.
pub trait Terminal {
    /// Shows some text, followed by a new line.
    fn say(&mut self, text: &str) -> io::Result<()>;
    /// Shows `prompt` and reads the answer, or `None` when input has ended.
    fn ask(&mut self, prompt: &str) -> io::Result<Option<String>>;
    /// Like [`Terminal::ask`], but the answer is a secret: it isn't echoed
    /// when the input is a real terminal, and is never written to the output.
    fn ask_secret(&mut self, prompt: &str) -> io::Result<Option<String>>;
    /// Reads one line without a prompt, such as a secret piped on stdin.
    fn read_line(&mut self) -> io::Result<Option<String>>;
}

/// Any input and output: scripted in tests, or piped stdin and stdout. A
/// secret is read like any line, and nothing of it is written out.
pub struct IoTerminal<R, W> {
    input: R,
    output: W,
}

impl<R: BufRead, W: Write> IoTerminal<R, W> {
    pub fn new(input: R, output: W) -> Self {
        Self { input, output }
    }

    pub fn output(&self) -> &W {
        &self.output
    }

    pub fn into_output(self) -> W {
        self.output
    }
}

impl<R: BufRead, W: Write> Terminal for IoTerminal<R, W> {
    fn say(&mut self, text: &str) -> io::Result<()> {
        writeln!(self.output, "{text}")
    }

    fn ask(&mut self, prompt: &str) -> io::Result<Option<String>> {
        write!(self.output, "{prompt}")?;
        self.output.flush()?;
        let answer = self.read_line()?;
        // Piped input isn't echoed, so end the prompt's line ourselves.
        writeln!(self.output)?;
        Ok(answer)
    }

    fn ask_secret(&mut self, prompt: &str) -> io::Result<Option<String>> {
        self.ask(prompt)
    }

    fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        if self.input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        Ok(Some(line.trim_end_matches(['\n', '\r']).to_owned()))
    }
}

/// The process's own stdin and stdout. When stdin is a real terminal,
/// secrets are typed without being echoed.
pub struct StdTerminal {
    interactive: bool,
}

impl StdTerminal {
    pub fn new() -> Self {
        Self {
            interactive: io::stdin().is_terminal(),
        }
    }
}

impl Default for StdTerminal {
    fn default() -> Self {
        Self::new()
    }
}

impl Terminal for StdTerminal {
    fn say(&mut self, text: &str) -> io::Result<()> {
        writeln!(io::stdout(), "{text}")
    }

    fn ask(&mut self, prompt: &str) -> io::Result<Option<String>> {
        let mut stdout = io::stdout();
        write!(stdout, "{prompt}")?;
        stdout.flush()?;
        let answer = self.read_line()?;
        if !self.interactive {
            writeln!(stdout)?;
        }
        Ok(answer)
    }

    fn ask_secret(&mut self, prompt: &str) -> io::Result<Option<String>> {
        if self.interactive {
            rpassword::prompt_password(prompt).map(Some)
        } else {
            self.ask(prompt)
        }
    }

    fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line)? == 0 {
            return Ok(None);
        }
        Ok(Some(line.trim_end_matches(['\n', '\r']).to_owned()))
    }
}
