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

- **Agent:** running hackathon-radar costs nothing extra when you use the Claude or Codex subscription you already pay for, within that subscription's usage limits. Runs consume your plan's allowance. If you choose an API key instead, you pay the provider's per-token API prices: [Anthropic API pricing](https://platform.claude.com/docs/en/about-claude/pricing) and [OpenAI API pricing](https://developers.openai.com/api/docs/pricing). hackathon-radar does not sell a subscription or store these Agent API keys; sign in through your Agent's own CLI. Scheduled Runs do not inherit API keys from your interactive shell, so arrange authentication through your Agent for unattended use.
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

## Scheduling and doctor

Install first, then schedule the installed binary from its permanent location:

```sh
hackathon-radar schedule install
hackathon-radar schedule status
hackathon-radar doctor --json
```

The default is Monday and Thursday at 08:00 local time. To change it, edit the Profile's `[schedule]` section using [the example Profile](https://github.com/edouard38000/hackathon-radar/blob/main/examples/profile.toml), then run `schedule install` again. macOS uses launchd; Linux uses a systemd user timer. On Linux, enable lingering with `loginctl enable-linger` if you want the timer to run while logged out.

`doctor` exits 1 when a check fails, and its JSON names a reason and fix for each check. It checks Sources over the network but never asks an Agent to judge anything. `--offline` skips network checks. It sends no Alert unless you explicitly pass `--send-test-alert`; only retry a test after fixing a delivery problem. `hackathon-radar run --dry-run` previews the Alert without sending or marking Hackathons as Announced, but does use your Agent and Sources.

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
2. From that commit, optionally run the Release workflow with `workflow_dispatch`. It builds and uploads Actions artifacts for all four targets, without creating or uploading a GitHub Release.
3. Tag and push the version (for example, after bumping to 0.1.0):

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. Check the workflow and [GitHub Release](https://github.com/edouard38000/hackathon-radar/releases). The tag must equal `v` plus the Cargo version or validation fails. Each archive must contain one `hackathon-radar-<version>-<target>/` directory holding the executable, `sidecar/photon` (with `.npmrc`, without `node_modules`), README, both licences and this guide, plus a separate SHA-256 checksum for the archive. Smoke-test the extracted binaries on their platforms, especially the cross-compiled Intel macOS build.
5. Publish to crates.io **manually**, from the clean, reviewed tagged checkout with your own crates.io credentials:

   ```sh
   cargo publish --dry-run --locked
   cargo publish --locked
   ```

   These are the `cargo publish --dry-run` and `cargo publish` steps with lockfile enforcement. CI never publishes the crate. Confirm public-release prerequisites in issue #1, including the switch-over comparison, before publishing.
