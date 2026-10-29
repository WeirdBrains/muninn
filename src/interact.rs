//! Prompting and confirmation policy shared by the CLI and library code.
use crate::error::{Error, Result};
use std::io::{BufRead, IsTerminal, Write};

#[derive(Debug, Clone, Copy)]
pub struct Interact {
    /// Prompts allowed (not --non-interactive and stdin is a TTY).
    pub interactive: bool,
    /// -y/--yes
    pub yes: bool,
    /// -q/--quiet
    pub quiet: bool,
}

impl Interact {
    pub fn new(non_interactive: bool, yes: bool, quiet: bool) -> Self {
        let interactive = !non_interactive && std::io::stdin().is_terminal();
        Self { interactive, yes, quiet }
    }

    /// Non-interactive policy (tests, scripts, TUI-driven calls).
    pub fn batch(yes: bool) -> Self {
        Self { interactive: false, yes, quiet: true }
    }

    /// Prompt for a value. Errors with `hint` when prompting is not allowed.
    pub fn prompt(&self, what: &'static str, question: &str, default: Option<&str>, hint: &str) -> Result<String> {
        if !self.interactive {
            return Err(Error::missing(what, hint.to_string()));
        }
        let mut err = std::io::stderr();
        match default {
            Some(d) => write!(err, "{question} [{d}]: ")?,
            None => write!(err, "{question}: ")?,
        }
        err.flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        let v = line.trim().to_string();
        if v.is_empty() {
            match default {
                Some(d) => Ok(d.to_string()),
                None => Err(Error::missing(what, hint.to_string())),
            }
        } else {
            Ok(v)
        }
    }

    /// Ask for confirmation. `--yes` answers yes; without a TTY this is an error.
    pub fn confirm(&self, question: &str) -> Result<bool> {
        if self.yes {
            return Ok(true);
        }
        if !self.interactive {
            return Err(Error::NeedsConfirmation(question.to_string()));
        }
        let mut err = std::io::stderr();
        write!(err, "{question} [y/N]: ")?;
        err.flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
    }

    /// Hidden password prompt.
    pub fn password(&self, question: &str, hint: &str) -> Result<String> {
        if !self.interactive {
            return Err(Error::missing("password", hint.to_string()));
        }
        Ok(rpassword::prompt_password(format!("{question}: "))?)
    }

    /// Informational output (suppressed by --quiet).
    pub fn info(&self, msg: impl AsRef<str>) {
        if !self.quiet {
            println!("{}", msg.as_ref());
        }
    }

    /// Informational output on stderr, for commands whose stdout is data.
    pub fn note(&self, msg: impl AsRef<str>) {
        if !self.quiet {
            eprintln!("{}", msg.as_ref());
        }
    }

    pub fn warn(&self, msg: impl AsRef<str>) {
        if !self.quiet {
            eprintln!("warning: {}", msg.as_ref());
        }
    }
}
