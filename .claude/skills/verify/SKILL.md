---
name: verify
description: Verify a hackathon-radar change before opening a pull request. Boots the built binary and drives a whole radar flow offline (init, test Alert, dry run, Run, report, calendar and Sheet export, Sheet import) in a throwaway directory, then prints a block to paste as the pull request's Evidence. Use it whenever you are about to open or update a pull request on hackathon-radar, or are asked to verify or try a change here.
---

# verify (hackathon-radar)

Run this before you open a pull request, and again after each fix round, from the repo root of your worktree:

```bash
cargo test --locked --test verify -- --nocapture
```

It takes about half a minute after the build. Commit first, so the block names the commit you push.

## What it runs

The harness is `tests/verify.rs`. It builds the binary and runs it with a clean environment and a temporary `HOME`:

| Step | What must happen |
| --- | --- |
| `--version` | the binary boots |
| `init --yes` | it writes a Profile |
| `test-alert` | the test Alert reaches the ntfy server |
| `run --dry-run` | the calendar's Candidates are filtered and judged, and nothing is sent |
| `run` | one Alert arrives, naming only the Hackathon that Fits |
| `run` again | nothing new is sent |
| `report`, `export ics` | they list that Hackathon |
| `export sheet`, `import-sheet` | the exported Sheet imports into a new database |

The binary does real HTTP and starts a real Agent process. Both stay on your machine: a local server holds the calendar and takes the ntfy Alerts, and a shell script stands in for the Agent through the `command` adapter. The server is also the binary's proxy and refuses every other request, so the built-in Sources are asked and answered with a refusal. Secrets come from the environment, so your keychain is never read, and your Profile and database are never touched.

## What it does not cover

A real Agent (`claude`, `codex`), the real Sources, Fares, Photon Alerts, `doctor` and `schedule`. If your change touches one, also run its tests and say in Evidence what you tried. The live test (`cargo test --locked --test live -- --ignored`) needs the network and a signed-in `claude`, sends real ntfy messages and installs a schedule; only the maintainer runs it.

## Evidence

The run ends with a fenced block under `---- paste into the pull request's Evidence ----`. Paste that block, unchanged, into the pull request's Evidence section. If it fails, fix the change and run it again; do not open the pull request on a failure.

Use `Verified-by: verify` as your commits' trailer when this passed on the commit you push.

## Changing this skill

This file is a protected path (`.claude/**`), and so is `tests/verify.rs` (review-loop's protected paths for this repo): a pull request that changes either needs Edouard. When a change adds or alters a user flow, add its step to the harness in the same pull request.
