# hackathon-radar

A self-hosted hackathon tracker. An AI agent running on your own machine finds hackathons that fit where you live and what you care about, tracks application deadlines, and texts you when something new or travel-funded appears.

> **Status:** early development. Runs read the global Sources (Devpost, MLH, ETHGlobal, the CASSINI Hackathons, dev.events), the Source pack for your Home (Europe or North America) and your own ICS calendars, merge Duplicate listings, and send Alerts by iMessage through Photon, or through ntfy.

## Principles

- **Self-hosted only.** Everything runs and stays on your machine. There is no hosted service.
- **Bring your own agent and messaging.** You run your own AI agent and your own iMessage line (Photon).
- **Personal use only.** It reads public hackathon listings for your own tracking. Respect each source's terms of use.
- **Rust first** wherever the platform allows.

## Try it

`hackathon-radar run --dry-run` reads the Sources (the global ones, [Devpost](https://devpost.com), [MLH](https://www.mlh.com), [ETHGlobal](https://ethglobal.com/events), the [CASSINI Hackathons](https://www.cassini.eu/hackathons/) and [dev.events](https://dev.events); the Source pack Home's country picks, Europe with [hackathon.be](https://hackathon.be), [hackathonhub.eu](https://hackathonhub.eu) and two Luma calendars, or North America with Luma's San Francisco, New York and Boston calendars; any packs you add; and any ICS calendars you add under `[sources]` in your Profile), merges listings of the same Hackathon from different Sources, keeps the listings that haven't ended and are in your Profile's Countries (which can name regions such as `"Schengen"`, `"European Union"`, `"North America"` or `"US West Coast"`), asks your Agent (Claude Code, Codex or another coding agent) which of them are real Hackathons you're eligible for and which are Travel-funded, stores the results in a local SQLite database, and prints the Alert it would send. A dry run sends nothing and marks nothing as Announced.

```sh
cargo run -- run --dry-run --profile examples/profile.toml
```

Without `--dry-run`, the Alert goes to the Alert channel in your Profile: an iMessage from your own [Photon](#photon-imessage-alerts) line, or a push through [ntfy](https://ntfy.sh). For ntfy, which needs no account, install the ntfy app, subscribe to a hard-to-guess topic, and uncomment the `[alert_channel]` section of the example Profile with that topic. Your first Alert is a Welcome Alert listing up to 6 Hackathons that already Fit. Later Runs only tell you about Hackathons that newly Fit, each one once, about known ones that newly became Travel-funded, and give a Deadline reminder 3 days before an open Hackathon that Fits closes applications; Travel-funded ones come first, at most 6 per Alert (the rest come in the next one), and nothing is sent when nothing is new. Each entry shows the dates and city, the trip or travel funding, the application status and deadline, and the link. Each Run also has your Agent recheck the deadline, status and travel funding of a few known Hackathons (see `[deadlines]` in the example Profile). If your ntfy server needs an access token, set it in `HACKATHON_RADAR_NTFY_TOKEN`; it never goes in the Profile. With no Alert channel set, the Alert is printed instead and nothing is marked as Announced.

Your Profile is a small TOML file with your Home, Countries, currency and Alert channel; see [`examples/profile.toml`](examples/profile.toml). By default it is read from `~/.config/hackathon-radar/profile.toml` and the database lives at `~/.local/share/hackathon-radar/hackathon-radar.sqlite` (both honour `XDG_CONFIG_HOME` and `XDG_DATA_HOME`). Pass `--profile` and `--db` to use other paths.

### Photon (iMessage) Alerts

[Photon](https://photon.codes) is a hosted iMessage API with a free tier. Its messages come from Photon's own line, so they notify you like any text (an iMessage from your own Apple ID to yourself wouldn't). Set it up once:

1. Sign up at [app.photon.codes](https://app.photon.codes), create a project and turn on iMessage (Spectrum). Note the project's ID and secret, and the iMessage line Photon gives you.
2. **Text your Photon line once from your phone**, with anything. A shared (free-tier) line can only message a number that has texted it first; until you do, sends fail with "Photon won't message … yet".
3. Install [Node.js](https://nodejs.org) 20.18.1 or later, then install the sidecar's pinned dependencies once:

   ```sh
   cd sidecar/photon && npm ci
   ```

4. Put your number in the Profile (E.164 format, or a Photon space ID) and your credentials in the environment. They never go in the Profile.

   ```toml
   [alert_channel]
   kind = "photon"
   to = "+32470123456"
   ```

   ```sh
   export PHOTON_PROJECT_ID=...
   export PHOTON_PROJECT_SECRET=...
   ```

Reading the credentials from the environment is a stopgap: they will move to your OS keychain, and `hackathon-radar init` and `doctor` will do steps 3 and 4 for you and check the rest. Until then, a scheduled Run only gets the `PATH` from `schedule install`, not these variables, so use ntfy for scheduled Runs or run `hackathon-radar run` from a shell that has them.

**Why there's some Node here.** Everything in hackathon-radar is Rust except [`sidecar/photon/`](sidecar/photon/), because Photon only ships a TypeScript SDK ([ADR 0005](docs/adr/0005-photon-node-sidecar.md)). For each Alert, the CLI runs `node sidecar/photon/send.mjs` once, with your number and the Alert text as JSON on stdin and the two credentials in its environment; the sidecar sends one message with the SDK, prints one line of JSON and exits. Its dependencies are pinned in `package-lock.json`, and their install scripts are turned off. If you installed the binary from somewhere other than this checkout, point `HACKATHON_RADAR_PHOTON_SIDECAR_DIR` at the `sidecar/photon` directory.

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

Each Run sends the listings that passed the Profile's rules to your Agent, chosen with `agent.adapter` in your Profile: `claude-code` (the default), `codex`, or `command` for any other coding agent. Whichever it is, the Agent may only search and read the web. It runs in an empty temporary directory, gets the prompt on stdin (or, with a command template, wherever you place it), and must answer with JSON matching the schema. The prompt and schema are built into hackathon-radar, and the answer is checked against the schema; if the Agent fails or answers in the wrong shape, the listings are kept and sent again on the next Run. A request that runs longer than 30 minutes is stopped. `[eligibility]` holds the facts organisers restrict on. hackathon-radar never signs you in to an Agent.

**Claude Code** (`adapter = "claude-code"`) runs headless on your own `claude` login (or `ANTHROPIC_API_KEY`, if you set it). If `claude` isn't signed in, run `claude` once yourself.

```sh
claude -p --permission-mode dontAsk --tools WebSearch,WebFetch --allowedTools WebSearch,WebFetch --strict-mcp-config --setting-sources "" --max-turns <cap> --output-format json --json-schema <schema>
```

It loads none of your Claude Code settings, hooks or MCP servers, so nothing you've allowed for your own work applies to it. The step cap is `agent.max_turns` in your Profile (50 by default). A Run makes at most two Agent requests: judging, capped by `agent.max_turns`, and travel estimates (see Getting there), capped by `agent.travel_max_turns` (15 by default).

**Codex** (`adapter = "codex"`, experimental) runs headless on your own `codex` login (or `CODEX_API_KEY`, if you set it). If `codex` isn't signed in, run `codex login` once yourself.

```sh
codex exec --sandbox read-only --skip-git-repo-check --ephemeral --ignore-user-config --ignore-rules -c approval_policy="never" -c web_search="live" -c features.shell_tool=false -c features.view_image=false --output-schema <schema file> --json -
```

The sandbox is read-only and nothing is ever approved; the shell and local-image tools are turned off, so live web search (which also opens pages) is all it has. It loads none of your `config.toml` (so none of your MCP servers or sandbox settings) and none of your execpolicy rules, and keeps no session files. Codex has no step cap of its own, so neither `agent.max_turns` nor `agent.travel_max_turns` applies to it; the 30-minute limit does.

**The Codex adapter is experimental.** Its read-only lockdown comes from OpenAI's documentation and is unverified against a real Codex CLI: if Codex ignored one of these settings, it could keep a shell that reads your whole disk while it reads arbitrary web pages. So it only runs when you opt in knowingly, and every Run that uses it prints a warning:

```toml
[agent]
adapter = "codex"
experimental = true
```

Without `experimental = true`, the Profile is refused; use `adapter = "claude-code"` instead. The opt-in stays until a recorded Codex run confirms the flags.

**Any other coding agent** (`adapter = "command"`) runs from a command template in your Profile:

```toml
[agent]
adapter = "command"
command = ["my-agent", "--json", "--schema", "{schema_file}"]
```

The first entry is the program; the rest are its arguments. The command runs without a shell, so each entry reaches the program as one argument, exactly as written: no quoting, globbing, `~` or `$VAR` expansion. These placeholders are filled in, alone or inside an argument (write `{{` and `}}` for literal braces):

| Placeholder | Becomes |
| --- | --- |
| `{schema_file}` | the path of a file holding the JSON schema |
| `{schema}` | the JSON schema itself |
| `{prompt}` | the whole prompt; when you use it, stdin stays empty |
| `{max_turns}` | `agent.max_turns` from your Profile, or `agent.travel_max_turns` for travel estimates |

The agent must print only the JSON answer on stdout and exit with code 0; anything else counts as the Agent failing, and the listings wait for the next Run.

**A generic agent must be read-only, and that is up to you.** hackathon-radar can't see inside another agent, so your command must give it web search and page reading and nothing else: no shell, no file writes, no MCP servers or other tools, and no flag that skips permissions or makes its sandbox writable. It reads arbitrary web pages on every Run, unattended, and must not be able to act on anything it reads there. hackathon-radar rejects any answer that doesn't match the schema, but it can't stop an agent you've given more power from using it.

### Getting there

Each Hackathon that Fits is labelled with how you'd get there from Home: `local` (in your Home city, or in Home's country within the Train limit), by train within the Train limit (`train ~2h`), or by flight to its nearest served airport (`fly BRU→MUC`). The label is stored and shown in the Alert entry. The Train limit is `travel.train_limit_hours` in your Profile (4 by default).

The Agent estimates the fastest train journey from Home and the nearest airport with flights from your Home airports once per destination city, in a second, smaller request with its own built-in prompt and schema, run with the same read-only command line and, for Agents that take a step cap, capped by `agent.travel_max_turns` (15 steps by default). The answers are cached per Home city and destination city, so later Runs never ask about a city again; a malformed answer is rejected and its cities are asked about on the next Run. If an estimate is wrong, add a `[[travel.overrides]]` entry for that city (see [`examples/profile.toml`](examples/profile.toml)); overrides always win over the cache. For now every trip within your Countries stays in scope, flights included: the Flight budget, which will decide flights, comes with Fares.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
(cd sidecar/photon && npm ci && npm test)   # the Photon sidecar
```

Tests never touch the network: Sources read saved fixtures from `tests/fixtures/`, no test runs a real Agent, and no test reaches Photon. The Photon channel's Rust tests and the sidecar's own tests (with a fake Photon SDK) check the same recorded sidecar outputs in `tests/fixtures/photon/`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
