# hackathon-radar

A self-hosted hackathon tracker. An AI agent running on your own machine finds hackathons that fit where you live and what you care about, tracks application deadlines, and texts you when something new or travel-funded appears.

> **Status:** early development. Runs read one Source and send Alerts through ntfy.

## Principles

- **Self-hosted only.** Everything runs and stays on your machine. There is no hosted service.
- **Bring your own agent and messaging.** You run your own AI agent and your own iMessage line (Photon).
- **Personal use only.** It reads public hackathon listings for your own tracking. Respect each source's terms of use.
- **Rust first** wherever the platform allows.

## Try it

`hackathon-radar run --dry-run` reads the [hackathon.be](https://hackathon.be) calendar, keeps the listings that haven't ended and are in your Profile's Countries, asks your Agent (Claude Code) which of them are real Hackathons you're eligible for and which are Travel-funded, stores the results in a local SQLite database, and prints the Alert it would send. A dry run sends nothing and marks nothing as Announced.

```sh
cargo run -- run --dry-run --profile examples/profile.toml
```

Without `--dry-run`, the Alert goes to the Alert channel in your Profile. For now that is [ntfy](https://ntfy.sh): install the ntfy app, subscribe to a hard-to-guess topic, and uncomment the `[alert_channel]` section of the example Profile with that topic. Your first Alert is a Welcome Alert listing up to 6 Hackathons that already Fit. Later Runs only tell you about Hackathons that newly Fit, each one once, at most 6 per Alert (the rest come in the next one), and send nothing when nothing is new. If your ntfy server needs an access token, set it in `HACKATHON_RADAR_NTFY_TOKEN`; it never goes in the Profile. With no Alert channel set, the Alert is printed instead and nothing is marked as Announced.

Your Profile is a small TOML file with your Home, Countries and Alert channel; see [`examples/profile.toml`](examples/profile.toml). By default it is read from `~/.config/hackathon-radar/profile.toml` and the database lives at `~/.local/share/hackathon-radar/hackathon-radar.sqlite` (both honour `XDG_CONFIG_HOME` and `XDG_DATA_HOME`). Pass `--profile` and `--db` to use other paths.

## Schedule

`hackathon-radar schedule install` runs a Run for you every Monday and Thursday at 08:00 local time, or on the days and time in your Profile's optional `[schedule]` section (see [`examples/profile.toml`](examples/profile.toml)). On macOS it installs a launchd agent in `~/Library/LaunchAgents`; on Linux, a systemd user timer in `~/.config/systemd/user`. Each Run appends its output to `~/.local/state/hackathon-radar/run.log` (honouring `XDG_STATE_HOME`).

```sh
hackathon-radar schedule install    # pass --profile and --db as for `run`; run it again after changing the schedule
hackathon-radar schedule status
hackathon-radar schedule uninstall
```

launchd and systemd give scheduled jobs only a minimal `PATH`, so `schedule install` saves your shell's `PATH` into the schedule; that is how a Run finds your Agent's CLI (`claude`, `codex`). If your shell has no `PATH`, it falls back to `/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin` on macOS or `/usr/local/bin:/usr/bin:/bin` on Linux. If you install or move a tool later, run `schedule install` again to refresh it.

The schedule runs the binary you installed it from, so install the binary first (`cargo install --path .`) rather than scheduling a `target/` build. On Linux, systemd only runs user timers while you're logged in unless you enable lingering (`loginctl enable-linger`).

### The Agent

Each Run sends the listings that passed the Profile's rules to [Claude Code](https://claude.com/claude-code), run headless on your own `claude` login (or `ANTHROPIC_API_KEY`, if you set it). hackathon-radar never signs you in; if `claude` isn't signed in, run `claude` once yourself. The Agent may only search and read the web:

```sh
claude -p --permission-mode dontAsk --allowedTools WebSearch,WebFetch --max-turns <cap> --output-format json --json-schema <schema>
```

The prompt goes on stdin. The prompt and schema are built into hackathon-radar, and its answer is checked against the schema; if the Agent fails or answers in the wrong shape, the listings are kept and sent again on the next Run. The step cap is `agent.max_turns` in your Profile (50 by default), and `[eligibility]` holds the facts organisers restrict on.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Tests never touch the network: Sources read saved fixtures from `tests/fixtures/`, and no test runs a real Agent.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
