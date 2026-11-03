# AGENTS.md

Instructions for AI coding agents (Claude Code, Codex) working in this repo. Start with `docs/background.md` for what this project is and the decisions made so far.

## Language

Rust first (cargo, clippy, rustfmt). Use another language only where Rust isn't possible or sensible (for example a browser frontend or a Claude skill's shell glue), and say why in the PR.

## Agent skills

### Issue tracker

Issues, specs and tickets are GitHub issues on `edouard38000/hackathon-radar`, managed with the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default roles, each label equal to its name: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root, created as terms and decisions get resolved. See `docs/agents/domain.md`.
