<!-- Improved compatibility of back to top link: See: https://github.com/othneildrew/Best-README-Template/pull/73 -->
<a id="readme-top"></a>

<!-- PROJECT SHIELDS -->
[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![License: MIT OR Apache-2.0][license-shield]][license-url]
[![CI][ci-shield]][ci-url]



<!-- PROJECT LOGO -->
<br />
<div align="center">
  <h3 align="center">hackathon-radar</h3>

  <p align="center">
    A self-hosted hackathon tracker. An AI agent running on your own machine finds hackathons that fit where you live and what you care about, tracks application deadlines, and texts you when something new or travel-funded appears.
    <br />
    <a href="docs/setup-guide.md"><strong>Explore the docs »</strong></a>
    <br />
    <br />
    <a href="#about-the-project">View Demo</a>
    &middot;
    <a href="https://github.com/edouard38000/hackathon-radar/issues/new?template=bug_report.yml">Report Bug</a>
    &middot;
    <a href="https://github.com/edouard38000/hackathon-radar/issues/new?template=feature_request.yml">Request Feature</a>
  </p>
</div>

> [!IMPORTANT]
> **Personal use only.** hackathon-radar reads public hackathon listings whose terms vary by Source. Respect those terms and do not redistribute collected data ([ADR 0001](docs/adr/0001-self-hosted-only.md)).

> [!TIP]
> **Quick start:** open this repo in Claude Code and say **"set me up"**. See [Installation](#installation).



<!-- TABLE OF CONTENTS -->
<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
      <ul>
        <li><a href="#principles">Principles</a></li>
        <li><a href="#built-with">Built With</a></li>
      </ul>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li>
      <a href="#usage">Usage</a>
      <ul>
        <li><a href="#commands">Commands</a></li>
        <li><a href="#try-it">Try it</a></li>
        <li><a href="#check-your-setup">Check your setup</a></li>
        <li><a href="#schedule">Schedule</a></li>
        <li><a href="#the-agent">The Agent</a></li>
        <li><a href="#reports-and-exports">Reports and exports</a></li>
        <li><a href="#coming-from-a-spreadsheet">Coming from a spreadsheet</a></li>
        <li><a href="#more-in-the-setup-guide">More in the setup guide</a></li>
      </ul>
    </li>
    <li><a href="#roadmap">Roadmap</a></li>
    <li>
      <a href="#contributing">Contributing</a>
      <ul>
        <li><a href="#development">Development</a></li>
        <li><a href="#top-contributors">Top contributors</a></li>
      </ul>
    </li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>



<!-- ABOUT THE PROJECT -->
## About The Project

A self-hosted hackathon tracker. An AI agent running on your own machine finds hackathons that fit where you live and what you care about, tracks application deadlines, and texts you when something new or travel-funded appears.

![hackathon-radar setting up a Profile in Brussels, importing a few hackathons in Europe and the US, and listing the ones that fit](demo/demo.gif)

The demo runs offline, on the made-up hackathons in [`demo/hackathons.csv`](demo/hackathons.csv). [`demo/demo.tape`](demo/demo.tape) records it with [VHS](https://github.com/charmbracelet/vhs) and says how to render it again.

> **Status:** early development. Runs read the global Sources (Devpost, MLH, ETHGlobal, the CASSINI Hackathons, dev.events), the Source pack for your Home (Europe or North America) and your own ICS calendars, merge Duplicate listings, and send Alerts by iMessage through Photon, or through ntfy.

### Principles

- **Self-hosted only.** Everything runs and stays on your machine. There is no hosted service.
- **Bring your own agent and messaging.** You run your own AI agent, and Alerts come through your own Photon project (iMessage) or ntfy topic.
- **Personal use only.** It reads public hackathon listings for your own tracking. Respect each Source's terms of use and do not redistribute collected data.
- **Rust first** wherever the platform allows.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

### Built With

* [![Rust][Rust-badge]][Rust-url] The CLI, in Rust 1.88 or later.
* [![SQLite][SQLite-badge]][SQLite-url] The local database, through [rusqlite](https://github.com/rusqlite/rusqlite).
* [![clap][clap-badge]][clap-url] The command line.
* [![Claude Code][Claude-badge]][Claude-url] Your own coding Agent judges the listings: Claude Code by default, Codex (experimental), or any other agent through a command template.
* [![Photon][Photon-badge]][Photon-url] iMessage Alerts, through a small Node.js sidecar.
* [![ntfy][ntfy-badge]][ntfy-url] Push Alerts, with no account needed.
* [![SerpApi][SerpApi-badge]][SerpApi-url] Flight Fares from Google Flights, on your own key.

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- GETTING STARTED -->
## Getting Started

hackathon-radar runs on your own macOS or Linux machine. The quickest way in is to let your coding agent set it up for you.

### Prerequisites

* **The binary:** a [release archive](https://github.com/edouard38000/hackathon-radar/releases) (no Rust needed) once the first release is out, or Rust 1.88 or later to build it with cargo.
* **An Agent:** [Claude Code](https://docs.anthropic.com/en/docs/claude-code) with `claude` on your `PATH` and signed in (run `claude` once yourself). Codex works too, behind an experimental opt-in, or any other coding agent through a command template; see [The Agent](#the-agent).
* **For Photon (iMessage) Alerts only:** [Node.js](https://nodejs.org) 20.18.1 or later. ntfy needs no Node.

### Installation

1. Clone the repo
   ```sh
   git clone https://github.com/edouard38000/hackathon-radar.git
   cd hackathon-radar
   ```
2. Open it in Claude Code (or Codex) and say **"set me up"**.

Open this repo in Claude Code (or Codex) and say "set me up". Your coding agent then follows the setup skill in [`.claude/skills/setup-hackathon-radar/SKILL.md`](.claude/skills/setup-hackathon-radar/SKILL.md): it checks what you have installed, installs a release binary or uses `cargo install`, asks you about your Home, Countries, budget, Themes, Agent and Alert channel, runs `hackathon-radar init` and `hackathon-radar doctor` for you, explains anything that needs fixing, and finishes once a test Alert has reached your phone. It never asks for your keys in the chat: you type them into `init`'s hidden prompts in your own terminal, and they go to your keychain. Claude Code picks the skill up on its own; Codex and other agents are pointed at it by [`AGENTS.md`](AGENTS.md). Claude Code is recommended; the Codex adapter for Runs requires an experimental opt-in.

#### Installing the binary yourself

Install the archive for your platform from [GitHub Releases](https://github.com/edouard38000/hackathon-radar/releases), keeping its `sidecar/` folder next to the binary, or install from crates.io with `cargo install hackathon-radar --locked` (Rust 1.88 or later). Neither is published yet; until the first release, build from a checkout with `cargo install --path . --locked`.

```sh
cargo install hackathon-radar --locked   # from crates.io, once it is published
cargo install --path . --locked          # from this checkout
```

See the [setup guide](docs/setup-guide.md) for install details, costs, Photon caveats, updating and uninstalling.

#### Setting up by hand

Without a coding agent, install the binary (`cargo install --path . --locked` from this checkout) and run:

```sh
hackathon-radar init
```

`init` asks about your Home, Countries, budget, Themes, Eligibility, Agent, keys and Alert channel, writes your Profile at `~/.config/hackathon-radar/profile.toml`, sends a test Alert and offers to install the [schedule](#schedule). `hackathon-radar test-alert` sends one short Alert through your Alert channel at any time and says whether it was sent. The setup guide covers [every question `init` asks](docs/setup-guide.md#setting-up-by-hand), [where your keys and tokens go](docs/setup-guide.md#your-keys-and-tokens) and [driving `init` with flags](docs/setup-guide.md#setting-up-without-questions).

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- USAGE EXAMPLES -->
## Usage

### Commands

<!-- Generated by mdsh from the binary's own help; run `mdsh` to update it. tests/readme.rs fails when it is out of date. -->
<!-- `$ cargo run --quiet -- --help` -->
```
A self-hosted tracker that finds hackathons that fit you and alerts you about them.

Usage: hackathon-radar <COMMAND>

Commands:
  init          Set up hackathon-radar, or change your Profile: Home, Countries, budget, Agent, keys and Alert channel
  run           Collect Hackathons from the Sources, store those that Fit, and compose the Alert
  test-alert    Send one short test Alert through the Profile's Alert channel
  schedule      Install, remove or check the recurring Run (launchd on macOS, systemd on Linux)
  report        Write a Markdown (or HTML) report of the Hackathons that Fit, sorted by date
  export        Export the deadlines as a calendar file, or the Hackathons as a CSV for Google Sheets
  import-sheet  Import the personal tracker: a CSV export of its Sheet's Hackathons tab
  doctor        Check everything a Run depends on, and say how to fix what's wrong. Exits 1 if a check fails
  help          Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

`hackathon-radar <command> --help` lists each command's flags.

### Try it

`hackathon-radar run --dry-run` reads the Sources (the global ones, [Devpost](https://devpost.com), [MLH](https://www.mlh.com), [ETHGlobal](https://ethglobal.com/events), the [CASSINI Hackathons](https://www.cassini.eu/hackathons/) and [dev.events](https://dev.events); the Source pack Home's country picks, Europe with [hackathon.be](https://hackathon.be), [hackathonhub.eu](https://hackathonhub.eu) and two Luma calendars, or North America with Luma's San Francisco, New York and Boston calendars; any packs you add; and any ICS calendars you add under `[sources]` in your Profile), merges listings of the same Hackathon from different Sources, keeps the listings that haven't ended and are in your Profile's Countries (which can name regions such as `"Schengen"`, `"European Union"`, `"North America"` or `"US West Coast"`), asks your Agent (Claude Code, Codex or another coding agent) which of them are real Hackathons you're eligible for and which are Travel-funded, stores the results in a local SQLite database, and prints the Alert it would send. A dry run sends nothing and marks nothing as Announced.

```sh
cargo run -- run --dry-run --profile examples/profile.toml
```

Without `--dry-run`, the Alert goes to the Alert channel in your Profile: an iMessage from your own [Photon](docs/setup-guide.md#photon-imessage-alerts) line, or a push through [ntfy](https://ntfy.sh). For ntfy, which needs no account, install the ntfy app, subscribe to a hard-to-guess topic, and uncomment the `[alert_channel]` section of the example Profile with that topic. Your first Alert is a Welcome Alert listing the first 6 Hackathons that already Fit; when more Fit, it counts the rest and they are Announced too, and `hackathon-radar report` shows them all (see [Reports and exports](#reports-and-exports)). Later Runs only tell you about Hackathons that newly Fit, each one once, about known ones that newly became Travel-funded, and give a Deadline reminder 3 days before an open Hackathon that Fits closes applications; Travel-funded ones come first, at most 6 per Alert (the rest come in the next one), and nothing is sent when nothing is new. Each entry shows the dates and city, the trip or travel funding, the application status and deadline, and the link. Each Run also has your Agent recheck the deadline, status and travel funding of a few known Hackathons (see `[deadlines]` in the example Profile), including, less often, closed ones that could still reopen (deadline unknown or not passed, not started yet); one that reopens is announced then. If your ntfy server needs an access token, `init` stores it in your keychain; it never goes in the Profile. With no Alert channel set, the Alert is printed instead and nothing is marked as Announced.

Your Profile is a small TOML file with your Home, Countries, currency, Themes, Eligibility, Agent and Alert channel, which `init` writes for you; see [`examples/profile.toml`](examples/profile.toml) for every setting. By default it is read from `~/.config/hackathon-radar/profile.toml` and the database lives at `~/.local/share/hackathon-radar/hackathon-radar.sqlite` (both honour `XDG_CONFIG_HOME` and `XDG_DATA_HOME`). Pass `--profile` and `--db` to use other paths.

How each Hackathon that Fits is labelled with the trip from Home, and how Fares are checked against your Flight budget, are in the setup guide's [Getting there](docs/setup-guide.md#getting-there) and [Fares and the Flight budget](docs/setup-guide.md#fares-and-the-flight-budget).

### Check your setup

```sh
hackathon-radar doctor                    # pass --profile and --db as for `run`
hackathon-radar doctor --offline          # skip the network checks
hackathon-radar doctor --send-test-alert  # also send one test Alert
hackathon-radar doctor --json             # machine-readable, for the setup skill
```

`doctor` checks everything a Run depends on and says plainly what's wrong and how to fix it. Each check prints `pass`, `warn` or `fail` (or `skip`), a one-line reason and, when there is something to do, a one-line fix. `doctor` exits 0 when nothing failed (warnings are fine) and 1 when a check failed. The setup guide lists [what each check looks at, and the `--json` output](docs/setup-guide.md#what-doctor-checks).

### Schedule

`hackathon-radar schedule install` runs a Run for you every Monday and Thursday at 08:00 local time, or on the days and time in your Profile's optional `[schedule]` section (see [`examples/profile.toml`](examples/profile.toml)). On macOS it installs a launchd agent in `~/Library/LaunchAgents`; on Linux, a systemd user timer in `~/.config/systemd/user`. Each Run appends its output to `~/.local/state/hackathon-radar/run.log` (honouring `XDG_STATE_HOME`).

```sh
hackathon-radar schedule install    # pass --profile and --db as for `run`; run it again after changing the schedule
hackathon-radar schedule status
hackathon-radar schedule uninstall
```

launchd and systemd give scheduled jobs only a minimal `PATH`, so `schedule install` saves your shell's `PATH` into the schedule; that is how a Run finds your Agent's CLI (`claude`, `codex`). If your shell has no `PATH`, it falls back to `/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin` on macOS or `/usr/local/bin:/usr/bin:/bin` on Linux. If you install or move a tool later, run `schedule install` again to refresh it.

The schedule runs the binary you installed it from, so install the binary first (`cargo install --path . --locked`) rather than scheduling a `target/` build. On Linux, systemd only runs user timers while you're logged in unless you enable lingering (`loginctl enable-linger`).

### The Agent

Each Run sends the listings that passed the Profile's rules to your Agent, chosen with `agent.adapter` in your Profile: `claude-code` (the default), `codex`, or `command` for any other coding agent. Whichever it is, the Agent may only search and read the web. It runs in an empty temporary directory, gets the prompt on stdin (or, with a command template, wherever you place it), and must answer with JSON matching the schema. The prompt and schema are built into hackathon-radar, and the answer is checked against the schema; if the Agent fails or answers in the wrong shape, the listings are kept and sent again on the next Run. A request that runs longer than 30 minutes is stopped. `[eligibility]` holds the facts organisers restrict on. hackathon-radar never signs you in to an Agent.

**Claude Code** (`adapter = "claude-code"`) runs headless on your own `claude` login (or `ANTHROPIC_API_KEY`, if you set it). If `claude` isn't signed in, run `claude` once yourself, with your subscription or an Anthropic Console account. hackathon-radar never stores your Anthropic key, and scheduled Runs don't see `ANTHROPIC_API_KEY` from your shell, so sign `claude` in for them.

```sh
claude -p --permission-mode dontAsk --tools WebSearch,WebFetch --allowedTools WebSearch,WebFetch --strict-mcp-config --setting-sources "" --max-turns <cap> --output-format json --json-schema <schema>
```

It loads none of your Claude Code settings, hooks or MCP servers, so nothing you've allowed for your own work applies to it. Every Agent request has its own step cap. A Run sends up to `agent.judging_requests_per_run` judging requests (3 by default), one after another, each of at most 20 listings and each capped by `agent.max_turns` (50 by default); it stops early once nothing waits, or after a request fails, and the rest wait for the next Run. Travel estimates (see [Getting there](docs/setup-guide.md#getting-there)) are capped by `agent.travel_max_turns` (15 by default). More judging requests keep up with more listings but use more of your subscription; at the defaults, two Runs a week judge up to 120 listings.

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
| `{max_turns}` | `agent.max_turns` from your Profile, or `agent.travel_max_turns` for travel estimates and `agent.fare_max_turns` for Fare estimates |

The agent must print only the JSON answer on stdout and exit with code 0; anything else counts as the Agent failing, and the listings wait for the next Run.

**A generic agent must be read-only, and that is up to you.** hackathon-radar can't see inside another agent, so your command must give it web search and page reading and nothing else: no shell, no file writes, no MCP servers or other tools, and no flag that skips permissions or makes its sandbox writable. It reads arbitrary web pages on every Run, unattended, and must not be able to act on anything it reads there. hackathon-radar rejects any answer that doesn't match the schema, but it can't stop an agent you've given more power from using it.

### Reports and exports

Beyond Alerts, you can browse what Fits you. Each command reads the local database (never the network), writes to stdout or to `--out <file>`, takes `--profile` and `--db` as for `run`, and with `--all` also lists the Hackathons that don't Fit (saying why) or are closed.

```sh
hackathon-radar report                  # Markdown: every Hackathon that Fits, soonest first, "Closing soon" on top
hackathon-radar report --html --out report.html
hackathon-radar export ics --out deadlines.ics   # an all-day "Apply: <name>" event on each open deadline, with an alarm [deadlines] reminder_days before
hackathon-radar export sheet --out hackathons.csv  # the personal tracker's Sheet columns, to import into Google Sheets
```

The database stays the source of truth: these files are generated from it and never read back (unless you import a Sheet CSV on purpose, below). Re-importing a newer calendar updates its events rather than duplicating them.

### Coming from a spreadsheet

If you already track hackathons in a Google Sheet laid out like the one `export sheet` writes, bring them in once:

```sh
hackathon-radar import-sheet hackathons.csv --dry-run   # say what would be imported, write nothing
hackathon-radar import-sheet hackathons.csv
```

Export the Sheet's Hackathons tab as CSV (File > Download > Comma-separated values). Each row keeps its deadline, status, travel funding, Fare and notes. Rows marked `🔔 notified` count as Announced, so you aren't told about them again; rows marked `OUT OF SCOPE` never Fit. A row that matches a Hackathon already stored is merged into it as a Duplicate listing, and importing the same file twice changes nothing. A CSV from `export sheet` imports too.

### More in the setup guide

The [setup guide](docs/setup-guide.md) holds the reference detail:

- [Install options](docs/setup-guide.md#install-options), [costs](docs/setup-guide.md#costs) and [Agent options](docs/setup-guide.md#agent-options)
- [Setting up by hand](docs/setup-guide.md#setting-up-by-hand): every question `init` asks
- [Your keys and tokens](docs/setup-guide.md#your-keys-and-tokens): the keychain, the secrets file and the environment variables
- [Setting up without questions](docs/setup-guide.md#setting-up-without-questions): every `init` flag
- [Getting there](docs/setup-guide.md#getting-there): the trip label, the Train limit and travel overrides
- [Fares and the Flight budget](docs/setup-guide.md#fares-and-the-flight-budget): SerpApi, Unverified fares and the Fare horizon
- [Photon (iMessage) Alerts](docs/setup-guide.md#photon-imessage-alerts), and [why there's some Node here](docs/setup-guide.md#why-theres-some-node-here)
- [What doctor checks](docs/setup-guide.md#what-doctor-checks), including its `--json` output
- [Updating and uninstalling](docs/setup-guide.md#updating-and-uninstalling), and [releasing](docs/setup-guide.md#releasing-maintainers) for the maintainer

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- ROADMAP -->
## Roadmap

- [x] Runs that read the global Sources, your Home's Source pack and your own ICS calendars, merge Duplicate listings, and have your own Agent judge what Fits
- [x] Alerts by iMessage through Photon, or through ntfy, with a Welcome Alert, Deadline reminders and rechecks of known Hackathons
- [x] `doctor`, `schedule`, reports, calendar and Sheet exports, and Sheet import
- [x] The "set me up" setup skill, and a live end-to-end test run before each release
- [ ] The maintainer's switch-over test from the old personal tracker ([#20][issue-20])
- [ ] The first release: archives on GitHub Releases and the crate on crates.io ([#20][issue-20])

The [v1 spec (#1)][issue-1] has the whole plan, and what is out of scope for now: a hosted service, Windows, Telegram, email or SMS Alerts, several Profiles on one install, and a web interface. See the [open issues][issues-url] for everything proposed (and known issues).

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- CONTRIBUTING -->
## Contributing

Contributions are welcome. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md), [`docs/background.md`](docs/background.md) for what the project is and why, and [`GLOSSARY.md`](GLOSSARY.md) for the words it uses (Profile, Run, Candidate, Alert, Announced…). Coding agents follow [`AGENTS.md`](AGENTS.md).

1. Find or open an issue, with the bug report or feature request template. Larger work is written up as a spec issue and split into tickets.
2. Fork the project and create a branch for that one issue (`git checkout -b 123-short-name`)
3. Build it Rust first (cargo, clippy, rustfmt), and run the checks in [Development](#development)
4. Commit your changes (`git commit -m '123: Short description'`) and push the branch (`git push origin 123-short-name`)
5. Open a pull request: one PR per issue, kept to its acceptance criteria, with `Closes #123` in the description and the pull request template filled in

### Development

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
(cd sidecar/photon && npm ci && npm test)   # the Photon sidecar
mdsh --frozen                               # the README's generated blocks (cargo install mdsh --locked)
```

The [Commands](#commands) block is generated by [mdsh](https://github.com/zimbatm/mdsh): after changing the CLI, run `mdsh` to update it. `tests/readme.rs` runs the binary with each generated block's arguments and fails when the README differs, so `cargo test` catches a stale block even without mdsh.

To render the demo again, run `cargo build --release --locked`, then `vhs demo/demo.tape` (VHS 0.12.1 or later).

Tests never touch the network or your keychain: Sources read saved fixtures from `tests/fixtures/`, no test runs a real Agent, no test reaches Photon, and secrets go to a fake keychain. The Photon channel's Rust tests and the sidecar's own tests (with a fake Photon SDK) check the same recorded sidecar outputs in `tests/fixtures/photon/`. `tests/setup_skill.rs` keeps the setup skill in step with the CLI: every flag it mentions must exist, every `init` question must have its flag in it, no secret may go on a command line, and its example `init` commands are run against a fake machine.

#### The live end-to-end test

`tests/live.rs` is the one test that touches the real world. It is ignored by default, so `cargo test` and CI never run it; the maintainer runs it on purpose before tagging a release (see the setup guide's [Releasing](docs/setup-guide.md#releasing-maintainers) section), on a machine with the network and a signed-in `claude` on the `PATH`:

```sh
cargo test --locked --test live -- --ignored
```

It drives the built binary with a temporary Profile, database and XDG directories: `init --yes` (Home Brussels, ntfy on a fresh random topic, a test Alert, no schedule) and a check that the test Alert reached ntfy.sh; `doctor --json` with `ok: true`; `run --dry-run`, which must collect Candidates and print a Welcome Alert; a real `run`, whose Alert must reach ntfy.sh and whose Hackathons must be Announced; a second `run --dry-run`, which must list no Announced Hackathon again (except as a Deadline reminder or newly Travel-funded); then `schedule install`, `status` and `uninstall`. It takes about ten minutes, most of it the Agent judging, and prints each step as it goes.

Without the network or a signed-in `claude`, it says why and passes as skipped. It refuses to start if a hackathon-radar schedule is already installed, because installing its own would replace yours; a drop guard removes the schedule it installs even when a step fails. On macOS the schedule commands get a temporary `HOME`, so even the plist stays in the temporary directory; on Linux the systemd units go in your own unit directory and are removed again. It never writes to your Profile, data or keychain, and leaves nothing behind but a few ntfy messages on its own topic. Runs still read the keychain as any Run does, so a SerpApi key you stored with `init` is used for Fares. The Agent is your real `claude`, so the test uses some of your Claude usage, and Claude Code keeps its usual session history.

### Top contributors

<a href="https://github.com/edouard38000/hackathon-radar/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=edouard38000/hackathon-radar" alt="contrib.rocks image" />
</a>

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- LICENSE -->
## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- CONTACT -->
## Contact

Maintainer: [@edouard38000](https://github.com/edouard38000)

Questions, bugs and ideas: [the project's issues][issues-url]. For a security problem, don't open an issue; follow [`SECURITY.md`](SECURITY.md).

Project Link: [https://github.com/edouard38000/hackathon-radar](https://github.com/edouard38000/hackathon-radar)

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- ACKNOWLEDGMENTS -->
## Acknowledgments

* [Best-README-Template](https://github.com/othneildrew/Best-README-Template), which this README follows
* [Photon](https://photon.codes), for iMessage Alerts
* [ntfy](https://ntfy.sh), for push Alerts with no account
* [SerpApi](https://serpapi.com/google-flights-api), for Google Flights Fares
* [Matt Pocock's skills](https://github.com/mattpocock/skills), the issue, triage and domain-doc workflow this repo was built with
* The Sources hackathon-radar reads: [Devpost](https://devpost.com), [MLH](https://www.mlh.com), [ETHGlobal](https://ethglobal.com/events), the [CASSINI Hackathons](https://www.cassini.eu/hackathons/), [dev.events](https://dev.events), [hackathon.be](https://hackathon.be), [hackathonhub.eu](https://hackathonhub.eu) and Luma's calendars
* [VHS](https://github.com/charmbracelet/vhs), which records the demo, and [mdsh](https://github.com/zimbatm/mdsh), which keeps the Commands block current
* [Shields.io](https://shields.io) and [contrib.rocks](https://contrib.rocks), for the badges and the contributors image

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->
[contributors-shield]: https://img.shields.io/github/contributors/edouard38000/hackathon-radar.svg?style=for-the-badge
[contributors-url]: https://github.com/edouard38000/hackathon-radar/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/edouard38000/hackathon-radar.svg?style=for-the-badge
[forks-url]: https://github.com/edouard38000/hackathon-radar/network/members
[stars-shield]: https://img.shields.io/github/stars/edouard38000/hackathon-radar.svg?style=for-the-badge
[stars-url]: https://github.com/edouard38000/hackathon-radar/stargazers
[issues-shield]: https://img.shields.io/github/issues/edouard38000/hackathon-radar.svg?style=for-the-badge
[issues-url]: https://github.com/edouard38000/hackathon-radar/issues
[license-shield]: https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg?style=for-the-badge
[license-url]: #license
[ci-shield]: https://img.shields.io/github/actions/workflow/status/edouard38000/hackathon-radar/ci.yml?branch=main&style=for-the-badge&label=CI
[ci-url]: https://github.com/edouard38000/hackathon-radar/actions/workflows/ci.yml
[issue-1]: https://github.com/edouard38000/hackathon-radar/issues/1
[issue-20]: https://github.com/edouard38000/hackathon-radar/issues/20
[Rust-badge]: https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white
[Rust-url]: https://www.rust-lang.org
[SQLite-badge]: https://img.shields.io/badge/SQLite-003B57?style=for-the-badge&logo=sqlite&logoColor=white
[SQLite-url]: https://www.sqlite.org
[clap-badge]: https://img.shields.io/badge/clap-4A4A55?style=for-the-badge&logo=rust&logoColor=white
[clap-url]: https://github.com/clap-rs/clap
[Claude-badge]: https://img.shields.io/badge/Claude_Code-D97757?style=for-the-badge&logo=claude&logoColor=white
[Claude-url]: https://docs.anthropic.com/en/docs/claude-code
[Photon-badge]: https://img.shields.io/badge/Photon_iMessage-34C759?style=for-the-badge&logo=imessage&logoColor=white
[Photon-url]: https://photon.codes
[ntfy-badge]: https://img.shields.io/badge/ntfy-317F6F?style=for-the-badge&logo=ntfy&logoColor=white
[ntfy-url]: https://ntfy.sh
[SerpApi-badge]: https://img.shields.io/badge/SerpApi-3B5998?style=for-the-badge
[SerpApi-url]: https://serpapi.com
