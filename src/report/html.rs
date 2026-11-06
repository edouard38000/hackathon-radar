//! The report as a standalone HTML page: the same content as the Markdown
//! report, with every value escaped and only http(s) links made clickable.

use super::{Report, web_link};

const STYLE: &str = "body{font-family:system-ui,sans-serif;max-width:46rem;margin:2rem auto;\
padding:0 1rem;line-height:1.5;color:#1f2328;background:#fff}\
h1{font-size:1.6rem}h2{margin-top:2rem;border-bottom:1px solid #d0d7de}\
article{margin:1.25rem 0}h3{margin-bottom:.25rem}\
dl{display:grid;grid-template-columns:max-content 1fr;gap:.1rem 1rem;margin:0}\
dt{color:#59636e}dd{margin:0}footer{margin-top:2rem;color:#59636e}\
@media (prefers-color-scheme:dark){body{color:#e6edf3;background:#0d1117}\
dt,footer{color:#9198a1}h2{border-color:#3d444d}a{color:#4493f8}}";

pub(super) fn render(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str(
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>hackathon-radar report</title>\n",
    );
    out.push_str(&format!("<style>{STYLE}</style>\n</head>\n<body>\n"));
    out.push_str("<h1>hackathon-radar report</h1>\n");
    out.push_str(&format!("<p>{}</p>\n", escape(&report.summary())));

    if !report.listed.is_empty() {
        out.push_str("<h2>Closing soon</h2>\n");
        let soon = report.closing_soon();
        if soon.is_empty() {
            out.push_str(&format!(
                "<p>No deadline in the next {} days.</p>\n",
                super::CLOSING_SOON_DAYS
            ));
        } else {
            out.push_str("<ul>\n");
            for listed in &soon {
                out.push_str(&format!(
                    "<li><strong>{}</strong>: {}</li>\n",
                    escape(&listed.hackathon.name),
                    escape(&listed.application(report.today))
                ));
            }
            out.push_str("</ul>\n");
        }

        out.push_str("<h2>Hackathons</h2>\n");
        for listed in &report.listed {
            out.push_str("<article>\n");
            let name = escape(&listed.hackathon.name);
            match listed.hackathon.link.as_deref().and_then(web_link) {
                Some(link) => out.push_str(&format!(
                    "<h3><a href=\"{}\">{name}</a></h3>\n",
                    escape(link)
                )),
                None => out.push_str(&format!("<h3>{name}</h3>\n")),
            }
            out.push_str("<dl>\n");
            for (label, value) in report.details(listed) {
                out.push_str(&format!(
                    "<dt>{}</dt><dd>{}</dd>\n",
                    escape(label),
                    escape(&value)
                ));
            }
            if let Some(link) = &listed.hackathon.link {
                let shown = escape(link);
                match web_link(link) {
                    Some(_) => out.push_str(&format!(
                        "<dt>Link</dt><dd><a href=\"{shown}\">{shown}</a></dd>\n"
                    )),
                    None => out.push_str(&format!("<dt>Link</dt><dd>{shown}</dd>\n")),
                }
            }
            out.push_str("</dl>\n</article>\n");
        }
    }

    out.push_str(&format!(
        "<footer>{}</footer>\n</body>\n</html>\n",
        escape(&report.last_run_line())
    ));
    out
}

/// Escapes text for an HTML element or a double-quoted attribute.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_escaped() {
        assert_eq!(
            escape("<script>alert('x')</script> & \"Hack\""),
            "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; &quot;Hack&quot;"
        );
    }
}
