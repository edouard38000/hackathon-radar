---
name: setup-hackathon-radar
description: Sets a person up with hackathon-radar from this repo. Checks prerequisites, installs the binary, interviews them for their Profile, runs hackathon-radar init with flags and hackathon-radar doctor --json, and ends when a test Alert has reached their phone. Use when someone in this repo says "set me up", "install hackathon-radar", "get me started", "help me set up hackathon-radar" or "set up my Profile", or asks to change their Profile, add a key, or fix their hackathon-radar setup.
---

# Set someone up with hackathon-radar

You are setting up hackathon-radar for the person you're talking to, on their own machine. The flow is the same in Claude Code, Codex or any other coding agent that can run shell commands in this repo.

`hackathon-radar init` holds all the setup logic and `hackathon-radar doctor` checks the result ([ADR 0007](../../../docs/adr/0007-coding-agents-are-the-front-door.md)). You only drive them: you interview the person, turn their answers into `init` flags, run `doctor`, and explain what it finds. Use the words in [`GLOSSARY.md`](../../../GLOSSARY.md) (Profile, Home, Countries, Train limit, Flight budget, Travel-funded rule, Themes, Eligibility, Agent, Alert channel, Alert, Run) when you talk to them. `hackathon-radar init --help` is the source of truth for flags; if it disagrees with this file, trust `--help` and say so.

## Rules

Read these before you start, and keep to them throughout.

1. **Only `init` and `doctor` set things up.** Never create or edit `~/.config/hackathon-radar/profile.toml`, `secrets.toml`, keychain items, or the launchd or systemd files yourself, not even to fix a typo. To change an answer, run `init` again with that answer's flag. Reading the Profile is fine: it holds no secrets.
2. **Secrets never go through the chat.** The secrets are the Photon project ID, the Photon project secret, the SerpApi key and an ntfy access token. Never ask the person to paste or type one into the conversation, and never put one in a command you run, a flag, an environment variable assignment or a file. They go in only through `init`'s hidden prompts in the person's own terminal, or piped into a `--…-stdin` flag from a source they control (a password manager), in a command they run themselves (step 4B). If they paste a secret into the chat anyway, don't repeat it or use it: tell them it is now in the conversation, that they should create a new one in that service's dashboard, and enter the new one through `init`.
3. **Never run `init` without `--yes` yourself.** Without `--yes`, `init` asks questions on the terminal, and you would be the one answering them, secrets included. That form is for the person, in their own terminal.
4. **No permission bypasses.** Never use `bypassPermissions`, Claude Code's `--dangerously-skip-permissions`, a writable sandbox, or ask the person to turn approvals off, for yourself or for the Agent that judges Runs ([ADR 0004](../../../docs/adr/0004-agent-is-read-only.md)).
5. **Ask before installing anything on the system.** Rust, Node.js, Claude Code, Codex, Homebrew or apt packages: say what you'd install and how, and wait for a clear yes. Building this repo's own binary with `cargo install` (step 2) and installing the Photon sidecar's pinned dependencies with `npm ci` (step 5) are part of setup; say you're about to do them.
6. **One test Alert.** Setup sends one test Alert (`init`'s). Never run `hackathon-radar run` without `--dry-run` (it sends the real Welcome Alert), and don't send another test Alert unless the first one didn't arrive, you've fixed the cause, and the person says yes.
7. **Stop and ask when something is ambiguous**: a city `init` doesn't know, unclear airports or region names, whether "no budget" means `none` or a number, an existing Profile with different answers, a `doctor` fix you aren't sure about, anything that changes their system. Don't guess.
8. **Signing in is theirs.** Never sign the person in to Claude Code, Codex, Photon, SerpApi or ntfy. Tell them what to run or open, and wait.

## 1. Check the prerequisites

Run these (they change nothing):

```sh
cargo --version
command -v hackathon-radar
command -v claude
command -v codex
```

Tell the person, in plain words, what is there and what is missing:

- **Install options.** Offer a [release archive](https://github.com/edouard38000/hackathon-radar/releases) first (no Rust needed), `cargo install --locked hackathon-radar` second, or a build from this checkout third. The Cargo options need Rust 1.88 or later. If `cargo` is missing, explain that the release archive avoids installing Rust; they can instead install Rust from https://rustup.rs (ask before you run the installer for them). If no release or crate has been published yet, use the checkout option when Rust is available.
- **An Agent: `claude` or `codex`.** Each Run asks the person's own Agent to judge listings, so one of them must be on their `PATH`. Claude Code is the default and the recommended one. Codex works but is experimental (step 3). Being inside Claude Code or Codex right now doesn't count: Runs call the `claude` or `codex` program.
- **Node.js** is only needed for Photon Alerts; it's checked in step 5 if they choose Photon.

If `hackathon-radar` is already installed and `~/.config/hackathon-radar/profile.toml` exists, this is a re-run: read the Profile, show them their current answers, and ask only what they want to change. `init` keeps every answer it isn't given a flag for, and any section it doesn't ask about.

## 2. Install the binary

Keep an already installed binary unless the person wants an update. Otherwise offer these options in order; say which one you are about to use:

1. **Release archive:** download the archive and `.sha256` for their macOS or Linux architecture from [GitHub Releases](https://github.com/edouard38000/hackathon-radar/releases), verify the checksum, and extract it under `~/.local/opt/`. Keep the resulting `hackathon-radar-<version>-<target>/` directory intact, and symlink its `hackathon-radar` executable into `~/.local/bin/` on PATH. Follow the [setup guide](../../../docs/setup-guide.md#release-archive-no-rust-needed) for the extraction and symlink commands and the macOS quarantine note.
2. **crates.io:** with Rust 1.88 or later:

   ```sh
   cargo install --locked hackathon-radar
   ```

3. **Build from this checkout:** from the repo root:

```sh
cargo install --path . --locked
```

Then check the installed binary:

```sh
hackathon-radar --version
```

Cargo installs go in `~/.cargo/bin`. If `hackathon-radar --version` isn't found, add its install directory to their PATH (rustup normally adds `~/.cargo/bin`; a new terminal picks it up). Use the installed `hackathon-radar` from here on, not `cargo run` or a `target/` build: the schedule runs the binary it was installed from, and on macOS the Keychain trusts that binary.

Photon finds its sidecar next to the binary for a release archive, or in the source tree for a Cargo install; `init` and `doctor` name the directory, so run `npm ci` there.

## 3. Interview for the Profile

Talk it through one topic at a time, in plain words, with an example for each. Then read the answers back as a short summary and get a yes before running anything. Each answer becomes one flag:

| Ask about | Flag | Notes |
| --- | --- | --- |
| Home: the city they travel from | `--home-city <CITY>` | `init` knows the nearby airports and country for many cities. |
| Home's country | `--home-country <COUNTRY>` | Defaults from the city; `none` leaves it out. |
| The airports they fly out of | `--airports <CODES>` | IATA codes, comma-separated, such as `BRU,CRL`. Ask if `init` doesn't know their city. |
| Countries or regions they'll travel to at their own cost | `--countries <LIST>` | Comma-separated English names of countries, US states or regions: Schengen, European Union, EEA, Europe, Benelux, DACH, Nordics, North America, US West Coast, US East Coast, US Northeast. A misspelt name is kept as a country that matches nothing, so check spelling. Defaults to Home's country. |
| The currency they count money in | `--currency <CODE>` | ISO code such as `EUR` or `USD`; defaults to Home country's. |
| Train limit: the longest train journey from Home that still counts as going by train | `--train-limit <HOURS>` | Default 4. Longer trips count as flights. |
| Flight budget: the most they'd pay for a return flight to a hackathon | `--flight-budget <AMOUNT>` | A whole amount in their currency, or `none` for no limit. With a budget, a SerpApi key gives real Google Flights prices (free plan: 250 searches a month, plenty); without a key their Agent estimates Fares and marks them unverified. The key is a secret: step 4B. |
| The Travel-funded rule | `--travel-funded-anywhere <BOOL>` | On (`true`) by default. Explain it like this: "Some organisers pay for part or all of your travel or lodging. With this on, a hackathon like that counts anywhere in the world, even outside your Countries and over your Flight budget. Runs take a little longer because your Agent also looks at listings from other countries." |
| Themes: what they want hackathons about | `--themes <LIST>` | Comma-separated, such as `AI agents, climate`, or `none` for everything. |
| Eligibility: whether they're a student | `--student <BOOL>` | `true`, `false`, or `none` to leave it out. |
| Eligibility: their nationalities | `--nationalities <LIST>` | Or `none`. |
| Eligibility: their age | `--age <AGE>` | Or `none`. Eligibility answers are optional: with `none`, their Agent treats that fact as no obstacle. |
| Their Agent | `--agent <ADAPTER>` | `claude-code` (the default and recommended). See below for `codex` and `command`. |
| Their Alert channel | `--alert-channel <CHANNEL>` | `photon`: an iMessage from their own free Photon line, with a sign-up; the recommended one. `ntfy`: a push notification through the ntfy app, no account. `none`: Alerts are only printed. Details in step 5. |
| Photon: their phone number | `--photon-to <NUMBER>` | E.164 format: `+`, country code and number, no spaces, such as `+32470123456`. |
| ntfy: the topic they subscribed to | `--ntfy-topic <TOPIC>` | Up to 64 letters, digits, `-` or `_`; long and hard to guess (step 5). |
| ntfy: their own server, if they have one | `--ntfy-server <URL>` | Default https://ntfy.sh, which needs no token. Their own server may need an access token: a secret, step 4B. |
| The schedule | `--install-schedule <BOOL>` | Ask: "Shall I schedule a Run every Monday and Thursday at 08:00?" Pass `true` only if they say yes. Other days and times are the `[schedule]` section of the Profile, which they can edit themselves later (see `examples/profile.toml`) before running `hackathon-radar schedule install` again. |
| The test Alert | `--test-alert <BOOL>` | `true` on a first setup or when the Alert channel changes: it is the one test Alert (step 7). `false` on a re-run that leaves the Alert channel as it was, such as adding a key or changing Themes. Pass it every time: `init` sends one by default. |

**The Agent.** Recommend `claude-code`. They must sign `claude` in themselves (run `claude` once, with their subscription or an Anthropic Console account); hackathon-radar never stores their Anthropic key.

- **Codex** only if they ask for it explicitly. First tell them: "The Codex adapter is experimental. Its read-only lockdown comes from OpenAI's documentation and is unverified against a real Codex CLI. If Codex ignored one of its settings, it could keep a shell that reads your whole disk while it reads arbitrary web pages." Only if they then clearly opt in, use `--agent codex --experimental`. Never add `--experimental` on your own. They sign in with `codex login` themselves.
- **Any other coding agent** is `--agent command --agent-command <COMMAND>`, a command template (see "The Agent" in `README.md`). It must be read-only: web search and page reading, nothing else. Stop and talk it through with them before choosing this; it's for people who know their agent's flags.

## 4. Run init

Build one `init` command with a flag for every answer. Lists with spaces go in double quotes. Then:

- **No secret to enter** (step 4A): you run it, with `--yes`.
- **A secret to enter** (step 4B): Photon the first time, a SerpApi key, or an ntfy token. The person runs it, without `--yes`, in their own terminal.

### 4A. You run it, with `--yes`

When their Alert channel is ntfy on ntfy.sh (or none) and they don't want a SerpApi key now, or every secret they need is already stored (a re-run; `doctor` shows where each one is). With `--yes`, `init` asks nothing: it takes the flags, keeps what the Profile already has, and uses each other answer's default. For example:

```sh
hackathon-radar init --yes \
  --home-city Brussels --home-country Belgium --airports BRU,CRL \
  --countries "Belgium, France, Netherlands, Germany" --currency EUR \
  --train-limit 4 --flight-budget 200 --travel-funded-anywhere true \
  --themes "AI agents, climate" --student false --nationalities Belgian --age none \
  --agent claude-code \
  --alert-channel ntfy --ntfy-topic hackathon-radar-k3f9q2m8x1zt \
  --test-alert true --install-schedule false
```

If `init` exits non-zero with `init needs more answers, so nothing was written.`, read the lines under it:

- `Missing:` lists answers it still needs, each as its flag and what it is, such as `--home-city <CITY>: the city you travel from`. Ask the person for them and run the command again with those flags.
- `Not understood:` lists flags whose value didn't make sense, each with the value and why. Ask, fix the value, run again.

Nothing is written until every answer is in, so running it again is safe. If `init` saved the Profile but exits non-zero because the test Alert or the schedule install failed, it says why: go to step 6.

### 4B. The person runs it, in their own terminal

When a secret has to be entered. Give them the same command without `--yes`. Since every other answer is a flag, `init` then only asks for the secrets, at hidden prompts: nothing they type shows on screen, lands in their shell history, or reaches this conversation. `init` stores each one in their operating system's keychain (or an owner-only file where there is no keychain) and never prints it.

Tell them, in these words or close to them:

> Please open a separate terminal window (not this chat, and not Claude Code's `!` prefix, whose output comes into the conversation), paste this command, and type each key when it asks for it. What you type stays hidden and goes to your keychain. When it finishes, tell me what the last lines say (you can paste them: `init` never prints a key) and whether the test Alert reached your phone.

For example, with Photon and a SerpApi key (do step 5's Photon steps first):

```sh
hackathon-radar init \
  --home-city Lyon --home-country France --airports LYS \
  --countries "France, Schengen" --currency EUR \
  --train-limit 5 --flight-budget 150 --travel-funded-anywhere true \
  --themes "AI agents" --student true --nationalities French --age 22 \
  --agent claude-code \
  --alert-channel photon --photon-to +33612345678 \
  --test-alert true --install-schedule true
```

`init` then asks for the SerpApi key (Enter skips it), the Photon project ID and the Photon project secret, and for an ntfy token only when they use their own ntfy server. If it asks anything else, a flag was missing: they can answer it there, or press Ctrl-C (nothing is written) while you fix the command.

If they keep their keys in a password manager with a command-line tool, they can pipe a key into `init` instead, also in their own terminal. The `--…-stdin` flags read one secret per line, in this order: the SerpApi key (`--serpapi-key-stdin`), the ntfy token (`--ntfy-token-stdin`), then the Photon project ID and the Photon project secret (`--photon-credentials-stdin`), only for the flags given. For example, to add a SerpApi key to an existing Profile without another test Alert:

```sh
op read "op://Private/SerpApi/credential" | hackathon-radar init --yes --flight-budget 150 --serpapi-key-stdin --test-alert false
```

Write the pipe for their own tool and item names. Never write a key into the command itself, as an argument, an `echo`, or an environment variable assignment.

## 5. The Alert channel

### Photon (recommended)

Walk them through these before giving them the `init` command, and check each one is done:

1. **Sign up** at https://app.photon.codes, create a project and turn on iMessage (Spectrum). The dashboard shows the project's ID and secret: they stay there until `init` asks for them, not in this chat. Note the iMessage line (phone number) Photon gives them.
2. **Text that Photon line once from their phone**, with anything. A shared (free) line can only message a number that has texted it first; until they do, sends fail with "Photon won't message … yet".
3. **Node.js 20.18.1 or later.** Check with `node --version` and `command -v npm`. If it's missing or older, ask before installing it (from https://nodejs.org or their package manager).
4. **`npm ci` in the resolved sidecar directory.** `init` runs it for them and names that directory (the dependencies are pinned and their install scripts are off). If `init` said npm wasn't on the PATH or `npm ci` failed, run `hackathon-radar doctor --offline` after Photon is selected in the Profile. Its `alert.photon.sidecar` and `alert.photon.dependencies` checks name the resolved directory. Once Node.js is there, replace the example path below with that exact directory and run:

   ```sh
   (cd "/path/reported/by/doctor" && npm ci)
   ```

### ntfy (no account)

1. They install the ntfy app on their phone (iOS or Android) and allow its notifications.
2. Anyone who knows a topic on ntfy.sh can read it, so pick a long, hard-to-guess one. You can make one, and show it to them:

   ```sh
   printf 'hackathon-radar-%s\n' "$(LC_ALL=C tr -dc 'a-z0-9' < /dev/urandom | head -c 12)"
   ```

3. They subscribe to exactly that topic in the app **before** `init` sends the test Alert.

With their own ntfy server, add `--ntfy-server <URL>`; if it needs an access token, that's step 4B.

## 6. Check everything with doctor

```sh
hackathon-radar doctor --json
```

`doctor` exits 1 when a check fails; that's its answer, not a crash, so read the JSON on stdout. If its `version` isn't `1`, stop: this skill is out of date. Run `hackathon-radar doctor` without `--json` and go by its text instead, and tell the person.

The shape is `{version, ok, counts, checks}`. `ok` is false exactly when a check failed. Each check has a stable `id`, a `section`, a `name` for people, a `status` (`pass`, `warn`, `fail` or `skip`), a one-line `reason`, a one-line `fix` (or `null`) and `details` (more lines, such as each problem in the Profile).

- `pass`: nothing to say.
- `skip`: neutral, something that wasn't checked or doesn't apply. Don't present it as a problem.
- `warn`: Runs still work. Explain it in plain words with its `fix`, and decide with the person whether to act.
- `fail`: Runs fail or Alerts don't reach them until it's fixed. Explain each one in plain words with its `fix`, then help fix it.

How to fix, by `id`:

- `profile`: the `details` list each problem. Run `init` again with the corrected flags (4A, or 4B when a secret is involved). Never edit the Profile.
- `secrets.*`: a secret is missing or can't be read. The person enters it again through `init` (4B). On macOS, an item the Keychain wants approval for (after the binary was rebuilt) is fixed by running `init` again in their terminal and choosing Always Allow. For the secrets file's permissions, follow the `fix`. `secrets.xdg-config-home` is a warning: scheduled Runs don't see a custom `XDG_CONFIG_HOME`.
- `agent.program`: their Agent's program isn't on the `PATH`. Offer to install it, with their OK. `agent.sign-in`: they run `claude` once themselves and sign in. `agent.experimental` is a warning they agreed to when they chose Codex.
- `alert.channel`: no Alert channel is set; run `init` again with `--alert-channel`.
- `alert.photon.*`: Node.js, the sidecar and `npm ci`, as in step 5. `alert.photon.line` is a reminder to text the Photon line once.
- `alert.ntfy.*`: the topic, or the server not answering.
- `schedule.*`: `schedule.installed` is a warning when no schedule is installed, which is expected if they said no. Other schedule fails: run `hackathon-radar schedule install` again, only if they want the schedule. `schedule.linger` on Linux needs `loginctl enable-linger`, a system setting: ask first.
- `source.*`: a warning that a Source didn't answer; sites go down for a while. Nothing to do.
- `last-run`: "No Run is recorded yet" is expected on a new setup. It goes away after the first Run.

Run `hackathon-radar doctor --json` again after each fix, until `ok` is true. If the same check still fails after a fix, stop and talk it through with the person rather than trying things.

## 7. Confirm the test Alert arrived

Ask the person whether the test Alert reached their phone. `init` saying it sent one only means Photon or ntfy took it.

- **It arrived**: setup is done.
- **It didn't, and `init` said the test Alert failed**: fix the cause with `doctor` (step 6). Then, with their OK, send one more:

  ```sh
  hackathon-radar doctor --send-test-alert --json
  ```

  Its `alert.test` check says whether it was sent. Ask again whether it arrived.
- **It didn't, though `init` said it was sent**: for Photon, did they text their Photon line first, from the number they gave? For ntfy, are they subscribed to exactly that topic, with notifications allowed? Fix that, then send one more as above, with their OK.

Setup is finished only when they confirm an Alert arrived.

## 8. Wrap up

Tell them:

- Their Profile is at `~/.config/hackathon-radar/profile.toml`. To change anything, ask you to set them up again, or run `hackathon-radar init` in a terminal: it shows their current answers and changes only what they answer differently.
- The schedule is installed only if they said yes. If not, `hackathon-radar schedule install` starts it whenever they're ready, and `hackathon-radar schedule status` shows it. The first Run sends a Welcome Alert listing what already fits them.
- Tracking hackathons in a spreadsheet already? `hackathon-radar import-sheet --dry-run <file.csv>` shows what a CSV export of their Sheet's Hackathons tab would bring in, and `hackathon-radar import-sheet <file.csv>` imports it.
- `hackathon-radar run --dry-run` shows the Alert a Run would send, without sending it (it uses their Agent and takes a few minutes).
- `hackathon-radar doctor` checks their setup at any time.
