# Setup guide

hackathon-radar runs on your own macOS or Linux machine, using your own Agent and Alert channel. It is for personal use only: each Source's terms apply, and collected data must not be redistributed. The usual starting point is to open the repo in Claude Code or Codex and say "set me up"; the [setup skill](https://github.com/edouard38000/hackathon-radar/blob/main/.claude/skills/setup-hackathon-radar/SKILL.md) drives the same `init` and `doctor` commands described here.

## Install options

### Release archive (no Rust needed)

Download the archive and its `.sha256` from [GitHub Releases](https://github.com/edouard38000/hackathon-radar/releases). Choose `aarch64-apple-darwin` for Apple silicon, `x86_64-apple-darwin` for Intel Macs, `x86_64-unknown-linux-gnu` for Intel/AMD Linux or `aarch64-unknown-linux-gnu` for ARM64 Linux. Linux binaries are built on Ubuntu 22.04 and require **glibc 2.35 or later**, covering Ubuntu 22.04 and Debian 12. Older distributions or musl-based systems need a source build.

Verify the checksum before extracting. For example, for version 0.1.0 on Apple silicon, in the download directory:

```sh
shasum -a 256 -c hackathon-radar-0.1.0-aarch64-apple-darwin.tar.gz.sha256
mkdir -p "$HOME/.local/opt" "$HOME/.local/bin"
tar -xzf hackathon-radar-0.1.0-aarch64-apple-darwin.tar.gz -C "$HOME/.local/opt"
ln -sfn "$HOME/.local/opt/hackathon-radar-0.1.0-aarch64-apple-darwin/hackathon-radar" "$HOME/.local/bin/hackathon-radar"
export PATH="$HOME/.local/bin:$PATH"
hackathon-radar --version
```

Use the filename and directory name for your version and target. Each archive extracts into one `hackathon-radar-<version>-<target>/` directory; the example puts it under `~/.local/opt/`. On Linux, `sha256sum -c` can replace `shasum -a 256 -c`. Add `~/.local/bin` to your shell's PATH permanently. Keep that versioned directory intact, with `sidecar/` next to the real binary; a symlink from a PATH directory works because lookup follows the binary's symlinks. The directory also includes the README, both licences and this guide under `docs/`.

The binaries are not signed or notarized. If macOS Gatekeeper blocks a downloaded binary, first verify its origin and checksum. You can then remove the downloaded-file quarantine attribute from that binary only:

```sh
xattr -d com.apple.quarantine "$HOME/.local/opt/hackathon-radar-0.1.0-aarch64-apple-darwin/hackathon-radar"
```

This removes the quarantine marker; it does not sign the binary. Only do it for a binary you trust, and only if the attribute is present.

### crates.io

With Rust 1.88 or later and cargo on PATH:

```sh
cargo install hackathon-radar --locked
hackathon-radar --version
```

Cargo installs the binary in `~/.cargo/bin` by default; put it on PATH. The crate includes the Photon sidecar, including its lockfile and `.npmrc`, but no `node_modules`. Photon still needs Node.js and `npm ci` in the directory `doctor` names, normally inside Cargo's registry source cache. Keep that source directory available. To avoid depending on the cache, copy the complete sidecar directory to a durable location and point `HACKATHON_RADAR_PHOTON_SIDECAR_DIR` there, or put it at `~/.cargo/bin/sidecar/photon`. A custom environment override must also be available to scheduled Runs; the executable-relative layout avoids that requirement.

### Build from a checkout

From this repository's root, with Rust 1.88 or later:

```sh
cargo install --path . --locked
hackathon-radar --version
```

Keep the checkout at the same path while using its sidecar, or copy the sidecar beside the installed binary as above. This option also works before the first release is published.

## Costs

- **Agent:** running hackathon-radar costs nothing extra when you use the Claude or Codex subscription you already pay for, within that subscription's usage limits. Runs consume your plan's allowance: a Run sends up to `agent.judging_requests_per_run` judging requests (3 by default, each of at most 20 listings and 50 steps), plus smaller requests for Duplicate listings, travel, rechecks and, without SerpApi, Fares. Raising `judging_requests_per_run` keeps up with more listings and uses more of your allowance; lowering it uses less and leaves more listings waiting. If you choose an API key instead, you pay the provider's per-token API prices: [Anthropic API pricing](https://platform.claude.com/docs/en/about-claude/pricing) and [OpenAI API pricing](https://developers.openai.com/api/docs/pricing). hackathon-radar does not sell a subscription or store these Agent API keys; sign in through your Agent's own CLI. Scheduled Runs do not inherit API keys from your interactive shell, so arrange authentication through your Agent for unattended use.
- **SerpApi:** the [free plan](https://serpapi.com/pricing) has 250 searches a month. At the defaults, two Runs a week and at most 10 Fare lookups per Run use about 87 searches a month (10 × 2 × 52 / 12). Each lookup includes all Home airports in one search; caching and the 10-week horizon usually reduce usage. Without a key, the Agent estimates Unverified fares. Extra manual Runs also count.
- **Photon:** [Photon Cloud offers a free managed shared line](https://photon.codes/pricing). Check the current free-tier limits before signing up; no paid line is needed for personal Alerts.
- **ntfy.sh:** [free, with no account needed](https://docs.ntfy.sh/faq/#can-i-use-this-in-my-app-will-it-stay-free). It is an alternative to Photon.

## Agent options

**Claude Code is the default and recommended Agent.** Install it separately, put `claude` on PATH and run `claude` yourself to sign in with your subscription or API account. `init` detects the installed Agent and lets you choose it.

**Codex is experimental.** Install and sign in through its own CLI. Choosing it during setup requires `--agent codex --experimental`. Its read-only lockdown has not been verified against a real Codex CLI. If Codex ignored one of the settings, it could retain a shell that reads your whole disk while reading arbitrary web pages. Only opt in if you accept that warning; Runs also print an experimental warning. Using Codex to drive the setup skill does not require choosing it as the Agent for Runs.

**A generic command template must be read-only.** Use `--agent command --agent-command <COMMAND>` only with an Agent whose flags restrict it to web search and page reading: no shell, file writes, MCP servers or permission bypasses. See [the README's template reference](https://github.com/edouard38000/hackathon-radar#the-agent). JSON validation cannot enforce another Agent's tool permissions. [ADR 0004](https://github.com/edouard38000/hackathon-radar/blob/main/docs/adr/0004-agent-is-read-only.md) explains why the Agent only searches and reads and the Rust CLI does all writing.

## Profile and first Alert

Run the installed binary in your own terminal:

```sh
hackathon-radar init
```

It asks about your Home, Countries, Train limit, Flight budget, Travel-funded rule, Themes, Eligibility, Agent and Alert channel, stores secrets in the OS keychain (or an owner-only fallback file), installs Photon dependencies if needed, sends a test Alert and offers to install the schedule. Never paste keys into a chat or command line. Re-running `init` keeps existing answers as defaults and preserves comments. For flags and automation, use `hackathon-radar init --help` or the setup skill; enter secrets only at hidden terminal prompts or through its documented stdin flags.

Setup ends when the test Alert actually arrives on your phone. A successful send alone does not establish delivery. The first scheduled Run sends a Welcome Alert for Hackathons that already Fit; subsequent Alerts cover new Fits, new travel funding and Deadline reminders.

### Setting up by hand

Without a coding agent, install the binary (`cargo install --path . --locked` from this checkout) and run:

```sh
hackathon-radar init
```

`init` walks you through everything once: your Home city (it suggests the airports near it), the Countries or regions you'll travel to, your Train limit, your Flight budget in your currency (taken from Home's country), the Travel-funded rule (on by default: a Travel-funded Hackathon then counts anywhere in the world), your Themes and Eligibility, your Agent (it looks for `claude` and `codex` on your `PATH`), an optional [SerpApi](#fares-and-the-flight-budget) key, and your Alert channel ([Photon](#photon-imessage-alerts) or ntfy), then sends a test Alert and offers to install the [schedule](#scheduling-and-doctor). It writes a complete Profile at `~/.config/hackathon-radar/profile.toml`. Run it again to change anything: it shows your current values as defaults, changes only what you answer differently, and keeps your own comments and any section it doesn't ask about.

`hackathon-radar test-alert` sends one short Alert through your Alert channel at any time and says whether it was sent.

### Your keys and tokens

Your Profile holds no secrets, so it is safe to share. `init` stores your keys in your operating system's keychain (the macOS Keychain, or the Secret Service on Linux) under the service `hackathon-radar`. On a machine with no keychain, such as a headless Linux server, they go to `~/.config/hackathon-radar/secrets.toml` instead, readable only by you; hackathon-radar refuses to read that file if anyone else can. Scheduled Runs read them from there, so they work without your shell's environment.

| Secret | Keychain account | Environment variable |
| --- | --- | --- |
| Photon project ID | `photon-project-id` | `PHOTON_PROJECT_ID` |
| Photon project secret | `photon-project-secret` | `PHOTON_PROJECT_SECRET` |
| SerpApi key | `serpapi-api-key` | `SERPAPI_API_KEY` |
| ntfy access token | `ntfy-token` | `HACKATHON_RADAR_NTFY_TOKEN` |

Each one is read from its environment variable first, when it is set (for CI, or to try a key out), then the keychain, then the file. hackathon-radar never prints a secret. Your Anthropic key isn't one of them: Claude Code runs on your own `claude` sign-in (see [The Agent](https://github.com/edouard38000/hackathon-radar#the-agent)).

### Setting up without questions

Coding agents (and scripts) drive `init` with flags: each one answers its question, and `--yes` answers every other question with its default, which is the value your Profile already has. Answers that have no default (a new Profile's Home city, for example) are listed together, and nothing is written until they're all given. Lists are comma-separated, and `none` clears an optional value.

```sh
printf '%s\n' "$SERPAPI_KEY" | hackathon-radar init --yes \
  --home-city Brussels --countries "Belgium, France, Schengen" \
  --flight-budget 200 --themes "AI agents, climate" \
  --agent claude-code --serpapi-key-stdin \
  --alert-channel ntfy --ntfy-topic hackathon-radar-k3f9q2m8x1zt
```

| Flag | Answers |
| --- | --- |
| `--yes` | Ask nothing; keep current values and take defaults; list what's missing |
| `--profile <PATH>`, `--db <PATH>` | Where the Profile and the database are |
| `--home-city <CITY>` | Home's city |
| `--home-country <COUNTRY>` | Home's country, or `none` (default: from the city) |
| `--airports <CODES>` | The airports you fly out of (default: the ones near Home, for the cities init knows) |
| `--countries <LIST>` | Countries, US states or regions (default: Home's country) |
| `--currency <CODE>` | Your currency (default: Home country's) |
| `--train-limit <HOURS>` | The Train limit (default: 4) |
| `--flight-budget <AMOUNT>` | The Flight budget, or `none` |
| `--travel-funded-anywhere <true\|false>` | The Travel-funded rule (default: true for a new Profile) |
| `--themes <LIST>` | Your Themes, or `none` |
| `--student <true\|false\|none>`, `--nationalities <LIST>`, `--age <AGE>` | Your Eligibility |
| `--agent <claude-code\|codex\|command>` | Your Agent (default: claude-code) |
| `--experimental` | Opts in to the experimental Codex adapter |
| `--agent-command <COMMAND>` | The command template for `--agent command`, split on spaces |
| `--serpapi-key-stdin` | Reads your SerpApi key from stdin |
| `--alert-channel <photon\|ntfy\|none>` | Your Alert channel |
| `--photon-to <NUMBER>` | Your number for Photon, in E.164 format |
| `--photon-credentials-stdin` | Reads your Photon project ID, then its secret, from stdin |
| `--skip-npm-ci` | Doesn't install the Photon sidecar's dependencies |
| `--ntfy-topic <TOPIC>`, `--ntfy-server <URL>` | Your ntfy topic and server (default: https://ntfy.sh) |
| `--ntfy-token-stdin` | Reads an ntfy access token from stdin |
| `--test-alert <true\|false>` | Whether to send a test Alert (default: true) |
| `--install-schedule <true\|false>` | Whether to install the schedule (default: false with `--yes`) |

Secrets never come from a flag, so they don't end up in your shell history: pipe them on stdin, one per line, in this order: the SerpApi key, the ntfy token, the Photon project ID, the Photon project secret (only those whose `--…-stdin` flag you pass). `init` exits non-zero if the test Alert or the schedule install fails, after saying why; the Profile is saved either way.

## Photon and ntfy

For Photon, sign up at [the dashboard](https://app.photon.codes), create a project and enable iMessage. **Text the assigned line once from the phone number you want Alerts on.** The free tier uses a shared line, which can only message numbers that have texted it first. Enter your project ID and secret only when `init` prompts. See [Photon pricing](https://photon.codes/pricing) for current user, messaging and shared-line limits; do not assume a free line has dedicated-line or cold-outreach features.

Photon needs **Node.js 20.18.1 or later** and npm on PATH. `init` runs `npm ci` in the resolved sidecar directory. After selecting Photon, these checks identify that directory even if dependency installation failed:

```sh
hackathon-radar doctor --offline
```

Find `alert.photon.sidecar` and `alert.photon.dependencies` (or their human-readable equivalents). Run `npm ci` in the exact directory they name, for example:

```sh
cd "/path/reported/by/doctor"
npm ci
```

This applies to release archives, crates.io installs and checkout builds. The sidecar lookup order is:

1. `HACKATHON_RADAR_PHOTON_SIDECAR_DIR`, if set and non-empty. This explicit override is always used, even if `send.mjs` is missing, so diagnostics name the directory you set.
2. `<real executable directory>/sidecar/photon`.
3. `<real executable directory>/../share/hackathon-radar/sidecar/photon`.
4. The compile-time source-tree copy, including the packaged registry source for crates.io installs.

Without a non-empty override, the first directory in steps 2–4 containing `send.mjs` wins. If none exists, errors name the source-tree fallback. Preserve the sidecar's `.npmrc` when moving it: dependency install scripts are disabled there.

For **ntfy**, install its phone app and subscribe to a long, hard-to-guess topic on ntfy.sh before setup sends the test Alert. No account, Node or sidecar is needed. Anyone who knows the topic can read it. Choose ntfy in `init`; your own ntfy server can also be configured there.

### Photon (iMessage) Alerts

[Photon](https://photon.codes) is a hosted iMessage API with a free tier. Its messages come from Photon's own line, so they notify you like any text (an iMessage from your own Apple ID to yourself wouldn't). `init` walks you through it and does steps 3 and 4 for you; by hand, it is:

1. Sign up at [app.photon.codes](https://app.photon.codes), create a project and turn on iMessage (Spectrum). Note the project's ID and secret, and the iMessage line Photon gives you.
2. **Text your Photon line once from your phone**, with anything. A shared (free-tier) line can only message a number that has texted it first; until you do, sends fail with "Photon won't message … yet".
3. Install [Node.js](https://nodejs.org) 20.18.1 or later. After selecting Photon in `init`, run `hackathon-radar doctor --offline`: its `alert.photon.sidecar` and `alert.photon.dependencies` checks name the resolved directory. Install the sidecar's pinned dependencies there once (replace the example path with that directory):

   ```sh
   cd "/path/reported/by/doctor" && npm ci
   ```

4. Put your number in the Profile (E.164 format, or a Photon space ID), and store your project's ID and secret in your keychain with `hackathon-radar init` (or set `PHOTON_PROJECT_ID` and `PHOTON_PROJECT_SECRET` in the environment). They never go in the Profile.

   ```toml
   [alert_channel]
   kind = "photon"
   to = "+32470123456"
   ```

Then `hackathon-radar test-alert` checks that an Alert reaches you.

### Why there's some Node here

Everything in hackathon-radar is Rust except [`sidecar/photon/`](https://github.com/edouard38000/hackathon-radar/tree/main/sidecar/photon), because Photon only ships a TypeScript SDK ([ADR 0005](https://github.com/edouard38000/hackathon-radar/blob/main/docs/adr/0005-photon-node-sidecar.md)). For each Alert, the CLI runs `node` with `send.mjs` in the resolved sidecar directory, with your number and the Alert text as JSON on stdin and the two credentials in its environment; the sidecar sends one message with the SDK, prints one line of JSON and exits. Its dependencies are pinned in `package-lock.json`, and their install scripts are turned off. A non-empty `HACKATHON_RADAR_PHOTON_SIDECAR_DIR` always overrides automatic lookup, even if the sidecar is missing; see [the lookup order above](#photon-and-ntfy). With `cargo install` from crates.io, keep that packaged source copy available and run `npm ci` in the directory `doctor` names.

## Getting there

Each Hackathon that Fits is labelled with how you'd get there from Home: `local` (in your Home city, or in Home's country within the Train limit), by train within the Train limit (`train ~2h`), or by flight to its nearest served airport (`fly BRU→MUC`). The label is stored and shown in the Alert entry. The Train limit is `travel.train_limit_hours` in your Profile (4 by default).

The Agent estimates the fastest train journey from Home and the nearest airport with flights from your Home airports once per destination city, in a second, smaller request with its own built-in prompt and schema, run with the same read-only command line and, for Agents that take a step cap, capped by `agent.travel_max_turns` (15 steps by default). The answers are cached per Home city and destination city, so later Runs never ask about a city again; a malformed answer is rejected and its cities are asked about on the next Run. If an estimate is wrong, add a `[[travel.overrides]]` entry for that city (see [`examples/profile.toml`](https://github.com/edouard38000/hackathon-radar/blob/main/examples/profile.toml)); overrides always win over the cache.

## Fares and the Flight budget

Set `travel.flight_budget` in your Profile (the most you'd pay for a return flight, in your Profile's `currency`, by default that of Home's country, or EUR) and each flight within your Countries is checked against it: it Fits only when its Fare, the cheapest return economy flight for one adult from any of your Home airports, leaving the day before the Hackathon starts and coming back the day after it ends, is at or under the budget. Under the Travel-funded rule (`travel.travel_funded_anywhere = true`), a Travel-funded Hackathon skips the budget, as it skips Countries; without the rule, its flight is checked against the budget like any other. The Alert entry shows the Fare after the trip, `fly BRU→MUC · ~€185`, `fly BRU→MUC · ~€96 from CRL` when another Home airport is cheaper, or `fly BRU→MUC · ~€185 (unverified)`; a Travel-funded entry shows its funding instead. A flight starting further away than `fares.horizon_weeks` (10) isn't held back: it Fits and is announced with `fare checked closer to the date`, so its application deadline can't pass while it waits, and the budget decides it once it is inside the horizon. If it then turns out over budget, it simply isn't announced again; no retraction is sent. Inside the horizon, a flight that has no Fare yet (the per-Run cap was reached, or the lookup failed) is held back from Alerts, Deadline reminders included, not dropped, until it has one, usually a Run or two later. Devpost listings give a submission period rather than the event's days, so they get no Fare and the budget leaves them unchecked. Without a Flight budget, Fares aren't looked up and flights stay in scope.

Fares come from Google Flights through [SerpApi](https://serpapi.com/google-flights-api), on your own key, which `init` stores in your keychain (it is never stored in the Profile and never printed; `SERPAPI_API_KEY` in the environment overrides it). Its free plan allows 250 searches a month, so a Run only looks up Fares for flights to Hackathons that Fit, aren't exempt from the budget by the Travel-funded rule, haven't started and start within `fares.horizon_weeks` (10); keeps each Fare for `fares.refresh_days` (14) before looking it up again; and makes at most `fares.lookups_per_run` lookups (10), soonest Hackathon first. Every Home airport goes into one search, so each Fare costs one search. At the defaults, two Runs a week use at most about 87 searches a month.

Without a key, your Agent estimates the Fares from web search in a separate, smaller read-only request with its own built-in prompt and schema, capped by `agent.fare_max_turns` (10 steps by default), and they are stored and shown as Unverified fares. A malformed answer is rejected and asked again on the next Run. Once you add a key, Unverified fares are replaced by real ones.

## Scheduling and doctor

Install first, then schedule the installed binary from its permanent location:

```sh
hackathon-radar schedule install
hackathon-radar schedule status
hackathon-radar doctor --json
```

The default is Monday and Thursday at 08:00 local time. To change it, edit the Profile's `[schedule]` section using [the example Profile](https://github.com/edouard38000/hackathon-radar/blob/main/examples/profile.toml), then run `schedule install` again. macOS uses launchd; Linux uses a systemd user timer. On Linux, enable lingering with `loginctl enable-linger` if you want the timer to run while logged out.

`doctor` exits 1 when a check fails, and its JSON names a reason and fix for each check. It checks Sources over the network but never asks an Agent to judge anything. `--offline` skips network checks. It sends no Alert unless you explicitly pass `--send-test-alert`; only retry a test after fixing a delivery problem. `hackathon-radar run --dry-run` previews the Alert without sending or marking Hackathons as Announced, but does use your Agent and Sources.

### What doctor checks

```sh
hackathon-radar doctor                    # pass --profile and --db as for `run`
hackathon-radar doctor --offline          # skip the network checks
hackathon-radar doctor --send-test-alert  # also send one test Alert
hackathon-radar doctor --json             # machine-readable, for the setup skill
```

`doctor` checks everything a Run depends on and says plainly what's wrong and how to fix it. Each check prints `pass`, `warn` or `fail` (or `skip`), a one-line reason and, when there is something to do, a one-line fix:

- **Profile**: it is there, parses and is valid, with every problem listed.
- **Secrets**: each secret your Profile needs (the Photon credentials, the ntfy token if you set one, the SerpApi key when you have a Flight budget) can be read, and from where: an environment variable, the keychain or the secrets file. Never its value. On macOS it reads the Keychain with prompts turned off, so an item a scheduled Run couldn't read (after a rebuild changed the binary the item trusts, macOS asks for approval, and a scheduled Run can't answer) fails here, with the fix: run `hackathon-radar init` again and choose Always Allow. A locked or missing keyring, which Runs quietly fall back from, is a warning, and the fallback file must be readable only by you. A custom `XDG_CONFIG_HOME` is a warning too, since scheduled Runs don't see it.
- **Agent**: its program is on your `PATH`; Claude Code is signed in (`claude auth status`, which costs nothing); Codex has its experimental opt-in. No Agent is ever asked to judge anything.
- **Alert channel**: ntfy's topic is set and its server answers; for Photon, Node.js 20.18.1 or later, the sidecar and its `node_modules` (else run `npm ci` in the resolved directory the check names), and a reminder to text your Photon line once. With `--send-test-alert`, one test Alert goes out as `test-alert` sends it; without it, nothing is sent.
- **Schedule**: installed or not, and whether it starts this binary, with the same Profile and database, and a `PATH` that finds your Agent's program (and `node` for Photon). On Linux, whether linger is on.
- **Sources**: one light, polite request to each Source a Run reads. A Source that doesn't answer is a warning, since sites go down for a while.
- **Last Run**: when it ran, whether it succeeded and why not. If it is older than twice the longest gap in your schedule, scheduled Runs may not be firing (the Mac was off at Run time, or the schedule isn't loaded).

`doctor` exits 0 when nothing failed (warnings are fine) and 1 when a check failed. `--json` prints the same results on stdout, in a shape that only changes with its `version`:

```json
{
  "version": 1,
  "ok": false,
  "counts": { "pass": 23, "warn": 2, "fail": 1, "skip": 0 },
  "checks": [
    {
      "id": "alert.photon.node",
      "section": "alert-channel",
      "name": "Node.js",
      "status": "fail",
      "reason": "Node.js 20.11.0 at /opt/homebrew/bin/node is older than 20.18.1, which the Photon sidecar needs.",
      "fix": "Install Node.js 20.18.1 or later from https://nodejs.org; if Runs are scheduled, run `hackathon-radar schedule install` again afterwards.",
      "details": []
    }
  ]
}
```

- `ok` is false exactly when a check has `"status": "fail"`, which is also when the exit code is 1.
- `counts` counts the checks by status.
- Each check has a stable `id` (such as `profile`, `secrets.photon-project-secret`, `agent.sign-in`, `alert.ntfy.server`, `schedule.path`, `source.devpost`, `last-run.age`), its `section` (`profile`, `secrets`, `agent`, `alert-channel`, `schedule`, `sources`, `last-run`), a `name` for people, its `status` (`pass`, `warn`, `fail` or `skip`), a one-line `reason`, a one-line `fix` or `null`, and `details`: more lines when one isn't enough, such as each problem in the Profile.
- Checks appear in the order above; which ones appear depends on the Profile (a Photon check only for Photon, a `schedule.linger` check only on Linux), and when the Profile doesn't load, the Agent, Alert channel and Sources sections are one `skip` each.

Limits: the Keychain check reads as this binary, so if the schedule starts a different binary (`schedule.binary` says so), a prompt that binary would meet isn't seen. The private URLs of your own calendars stay out of the output; only their host is shown.

## Updating and uninstalling

Update a release install by verifying and extracting the new archive under `~/.local/opt/`, then repointing `~/.local/bin/hackathon-radar` to the binary inside the new versioned directory, as in the install example. Keep the binary and sidecar together and run `schedule install` again to use the new binary. For a crates.io install, re-run `cargo install hackathon-radar --locked`; Cargo upgrades when a newer version is available. For a checkout install, pull the intended revision and re-run `cargo install --path . --locked`. After any update, run `init` if needed to refresh Photon dependencies, or `npm ci` in the newly resolved directory. On macOS the Keychain may ask again for access when a rebuilt or replaced binary reads existing secrets: run `init` in your terminal and choose Always Allow. Run `schedule install` again after moving the binary or installing tools, so the schedule captures the right binary and PATH, then run `doctor`.

To stop tracking:

```sh
hackathon-radar schedule uninstall
```

Then remove the release directory and its PATH symlink, or run `cargo uninstall hackathon-radar` for a Cargo install. To erase local data, remove the directories below after backing up anything you want to keep:

- `~/.config/hackathon-radar`: Profile and any owner-only `secrets.toml` fallback.
- `~/.local/share/hackathon-radar`: SQLite database.
- `~/.local/state/hackathon-radar`: Run logs.

These respect `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_STATE_HOME`; remove any custom `--profile` or `--db` paths separately. Keychain items remain under service `hackathon-radar`; remove them with your operating system's credential manager if desired. Removing the app does not cancel paid provider plans or remove those accounts.

## Releasing (maintainers)

1. Bump `package.version` in `Cargo.toml` and refresh `Cargo.lock`. Run the CI checks and review `cargo package --list --allow-dirty`; the crate should include the runtime sources and Photon sidecar but no `node_modules` or test fixtures. Commit the reviewed version change.
2. From that commit, run the live end-to-end test on your Mac (or a Linux machine) with the network and a signed-in `claude` on the `PATH`. It must print `PASSED`; a `SKIPPED` line means it checked nothing:

   ```sh
   cargo test --locked --test live -- --ignored
   ```

   It repeats the manual release check against the real world in about ten minutes: `init` with a test Alert over ntfy, `doctor --json`, two dry runs around a real Run whose Welcome Alert must reach ntfy.sh, and `schedule install` and `uninstall`. It uses a temporary Profile, database and directories, and leaves nothing behind but a few ntfy messages on a random topic. It refuses to start while a hackathon-radar schedule is installed, so run `schedule uninstall` first on a machine that has one, and `schedule install` again afterwards. See the README's [Development](https://github.com/edouard38000/hackathon-radar#the-live-end-to-end-test) section for what it checks and leaves behind.
3. Optionally run the Release workflow with `workflow_dispatch` from that commit. It builds and uploads Actions artifacts for all four targets, without creating or uploading a GitHub Release.
4. Tag and push the version (for example, after bumping to 0.1.0):

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

5. Check the workflow and [GitHub Release](https://github.com/edouard38000/hackathon-radar/releases). The tag must equal `v` plus the Cargo version or validation fails. Each archive must contain one `hackathon-radar-<version>-<target>/` directory holding the executable, `sidecar/photon` (with `.npmrc`, without `node_modules`), README, both licences and this guide, plus a separate SHA-256 checksum for the archive. Smoke-test the extracted binaries on their platforms, especially the cross-compiled Intel macOS build.
6. Publish to crates.io **manually**, from the clean, reviewed tagged checkout with your own crates.io credentials:

   ```sh
   cargo publish --dry-run --locked
   cargo publish --locked
   ```

   These are the `cargo publish --dry-run` and `cargo publish` steps with lockfile enforcement. CI never publishes the crate. Confirm public-release prerequisites in issue #1, including the switch-over comparison, before publishing.
