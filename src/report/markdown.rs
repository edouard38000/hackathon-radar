//! The report in Markdown.

use super::{Report, web_link};

pub(super) fn render(report: &Report) -> String {
    let mut out = String::from("# hackathon-radar report\n\n");
    out.push_str(&text(&report.summary()));
    out.push_str("\n\n");

    if !report.listed.is_empty() {
        out.push_str("## Closing soon\n\n");
        let soon = report.closing_soon();
        if soon.is_empty() {
            out.push_str(&format!(
                "No deadline in the next {} days.\n\n",
                super::CLOSING_SOON_DAYS
            ));
        }
        for listed in &soon {
            out.push_str(&format!(
                "- **{}**: {}\n",
                text(&listed.hackathon.name),
                text(&listed.application(report.today))
            ));
        }
        if !soon.is_empty() {
            out.push('\n');
        }

        out.push_str("## Hackathons\n");
        for listed in &report.listed {
            out.push_str(&format!("\n### {}\n\n", text(&listed.hackathon.name)));
            for (label, value) in report.details(listed) {
                out.push_str(&format!("- {label}: {}\n", text(&value)));
            }
            if let Some(link) = &listed.hackathon.link {
                match web_link(link) {
                    Some(link) if !link.contains(['<', '>']) => {
                        out.push_str(&format!("- Link: <{link}>\n"));
                    }
                    _ => out.push_str(&format!("- Link: {}\n", text(link))),
                }
            }
        }
        out.push('\n');
    }

    out.push_str("---\n\n");
    out.push_str(&text(&report.last_run_line()));
    out.push('\n');
    out
}

/// Escapes the characters Markdown would read as formatting in running text.
fn text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(
            c,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '#'
        ) {
            out.push('\\');
        }
        if c == '\n' || c == '\r' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_characters_are_escaped() {
        assert_eq!(
            text("AI_Hack *2026* [Paris] #1\nnext"),
            "AI\\_Hack \\*2026\\* \\[Paris\\] \\#1 next"
        );
        assert_eq!(text("Liège – 5–6 Dec"), "Liège – 5–6 Dec");
    }
}
