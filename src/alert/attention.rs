//! The "needs attention" part of an Alert, sent once after failed Runs in a row
//! (ticket 10). It goes out through the same Alert channel as any Alert, merged
//! into the Run's Alert when there is one, so a Run still sends at most one.

use super::Alert;

/// The longest error shown in the "needs attention" part, so it fits on a phone.
pub const MAX_ERROR_CHARS: usize = 200;

/// Tells the person that hackathon-radar needs attention after `failed_runs`
/// failed Runs in a row, with the last error in one line.
pub fn needs_attention(failed_runs: usize, last_error: &str) -> Alert {
    Alert {
        text: format!(
            "hackathon-radar needs attention: the last {failed_runs} Runs failed.\n\
             Last error: {}\n\
             Run `hackathon-radar doctor` to find out what's wrong.",
            shortened(&one_line(last_error), MAX_ERROR_CHARS)
        ),
    }
}

/// The Alert to send: the "needs attention" part first, then the Run's own
/// Alert, as one Alert. `None` when there is neither.
pub fn merged(attention: Option<&Alert>, alert: Option<&Alert>) -> Option<Alert> {
    match (attention, alert) {
        (Some(attention), Some(alert)) => Some(Alert {
            text: format!("{}\n\n{}", attention.text, alert.text),
        }),
        (one, other) => one.or(other).cloned(),
    }
}

/// `text` on one line, with every run of whitespace (newlines included) made one space.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn shortened(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max_chars - 1).collect();
    format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_error_is_shown_on_one_short_line() {
        let alert = needs_attention(2, "Claude Code failed:\n  overloaded\n");
        assert_eq!(
            alert.text,
            "hackathon-radar needs attention: the last 2 Runs failed.\n\
             Last error: Claude Code failed: overloaded\n\
             Run `hackathon-radar doctor` to find out what's wrong."
        );

        let long = "x".repeat(500);
        let alert = needs_attention(3, &long);
        let line = alert.text.lines().nth(1).unwrap();
        assert_eq!(line.chars().count(), "Last error: ".len() + MAX_ERROR_CHARS);
        assert!(line.ends_with('…'));
    }

    #[test]
    fn attention_comes_first_in_the_merged_alert() {
        let attention = Alert {
            text: "attention".into(),
        };
        let alert = Alert {
            text: "hackathons".into(),
        };
        assert_eq!(
            merged(Some(&attention), Some(&alert)).unwrap().text,
            "attention\n\nhackathons"
        );
        assert_eq!(merged(Some(&attention), None), Some(attention));
        assert_eq!(merged(None, Some(&alert)), Some(alert));
        assert_eq!(merged(None, None), None);
    }
}
