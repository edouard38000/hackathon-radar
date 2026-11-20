# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

## Commands

CI runs exactly these on Linux for every PR (Rust 1.88+, edition 2024):

```sh
cargo build
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
(cd sidecar/photon && npm ci && npm test)   # only when touching sidecar/photon/ (Node 20.18.1+)
```

- One integration test file: `cargo test --locked --test run` (files live in `tests/*.rs`).
- One test by name: `cargo test --locked --test run -- some_test_name`.
- Live end-to-end test (real network, real `claude`, ntfy.sh; ~10 min, ignored by default, maintainer runs it before a release): `cargo test --locked --test live -- --ignored`.

## Testing rules

- Tests never touch the network, a real Agent, Photon or the real keychain. Sources read saved fixtures from `tests/fixtures/`; secrets go to a fake keychain; shared fakes are in `tests/support/`. Anything needing the network belongs in `tests/live.rs`.
- `tests/setup_skill.rs` keeps `.claude/skills/setup-hackathon-radar/SKILL.md` in step with the CLI: every flag it mentions must exist, every `init` question must have a flag, no secret may appear on a command line, and its example `init` commands are executed. Changing `init` flags means updating the skill too.
- The Photon Rust tests and the sidecar's own tests both check the recorded outputs in `tests/fixtures/photon/`.
- Don't weaken existing tests; add new ones.

## Architecture

A Rust CLI (`src/main.rs`, clap subcommands: `init`, `run`, `test-alert`, `schedule`, `report`, `export`, `import-sheet`, `doctor`) over a library crate (`src/lib.rs`). Core split (ADR 0002): **the CLI fetches, filters, stores and alerts; the person's own coding Agent is run headless only for what needs judgement.**

A Run (`src/run.rs`) is the central pipeline. All dependencies come in through `RunDeps` as trait objects so tests drive a whole Run with fakes:

1. `sources::Source` impls (`src/sources/`, one file per site, grouped into regional packs in `sources/packs.rs`, ADR 0009) collect `Candidate`s. `sources/polite.rs` wraps HTTP politely. A failing Source doesn't fail the Run.
2. `matching` merges Duplicate listings by SQL rules, asking the Agent only for ambiguous pairs (ADR 0008).
3. `rules` applies Profile checks that need data only (countries, dates, budget…).
4. `judging` sends the rest to the `agent::Agent` (adapters: `claude_code`, `codex`, `command` template). Prompts (`*.md`) and JSON schemas (`*.schema.json`) live next to the code in `src/agent/` and `src/matching/` and are compiled into the binary (ADR 0006); the engine, not the Agent, validates output against the schema. Themes are judged in the same request (ADR 0010).
5. `travel` / `fares` work out how to get there and flight cost (`fares::FareLookup` via SerpApi when a key exists, else Agent-estimated "Unverified" fares). `deadlines` keeps deadlines/status current.
6. Results go to the SQLite `store` (source of truth, ADR 0003; reports/CSV/ICS are exports only), then at most one Alert goes out through an `alert::AlertChannel` (ntfy, or Photon iMessage via the Node sidecar in `sidecar/photon/`, ADR 0005).

Other seams with traits for faking: `Clock`, `HttpGet`/`HttpPost`, `ProcessRunner`, `Keychain`, `scheduler::CommandRunner`, `init::Terminal`. `src/wiring.rs` builds the real Alert channel and Fare lookup from the Profile plus secrets. `src/scheduler/` installs launchd (macOS) or systemd (Linux) units. `src/doctor/` checks everything a Run depends on (`doctor --json` is consumed by the setup skill).

## Hard constraints

- **Database migrations**: `MIGRATIONS` in `src/store.rs` is append-only. Never edit an existing migration; add a new one.
- **Agent is read-only** (ADR 0004): it may only search and read the web. Giving it another tool or loosening its CLI flags needs a new ADR first.
- **No secrets in the Profile or on command lines.** Keys live in the OS keychain via `src/secrets.rs`, so scheduled Runs (which don't see the shell environment) still find them.
- Use the domain terms from `GLOSSARY.md` (Profile, Run, Candidate, Hackathon, Fit, Alert, Announced, Travel-funded, Source…) in code, docs, issues and PRs, capitalised as there. Decisions are in `docs/adr/`; add an ADR for new architectural decisions.
- PRs: one per issue, `Closes #<issue>`, fill in the PR template.
