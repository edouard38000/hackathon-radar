# Local SQLite is the source of truth; a Google Sheet is only an export

Results live in a local SQLite database owned by the Rust CLI. Reports (Markdown/HTML) and an optional Google Sheet are generated from it, never read back. The personal version used a Google Sheet as its store; we moved away from that so data stays on the person's machine, nobody needs a Google account to try the tool, and the CLI can test and query its own schema.
