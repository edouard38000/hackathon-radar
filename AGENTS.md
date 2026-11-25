# AGENTS.md

Instructions for AI coding agents (Claude Code, Codex) working in this repo. Start with `docs/background.md` for what this project is and the decisions made so far.

## Setting someone up

When someone asks to be set up ("set me up", "install hackathon-radar"), follow [`.claude/skills/setup-hackathon-radar/SKILL.md`](.claude/skills/setup-hackathon-radar/SKILL.md) step by step, whichever coding agent you are. Claude Code loads it as a skill on its own; in Codex or any other agent, read the file and do what it says: it checks the prerequisites, installs the binary, interviews the person for their Profile, runs `hackathon-radar init` and `hackathon-radar doctor --json`, and ends when a test Alert has arrived. Never ask for a key or token in the chat; the skill says how secrets go in.

## Language

Rust first (cargo, clippy, rustfmt). Use another language only where Rust isn't possible or sensible (for example a browser frontend or a Claude skill's shell glue), and say why in the PR.

## Agent skills

### Issue tracker

Issues, specs and tickets are GitHub issues on `edouard38000/hackathon-radar`, managed with the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default roles, each label equal to its name: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root, created as terms and decisions get resolved. See `docs/agents/domain.md`.

## Builders and pull requests

Builders are Claude Code agents, one ticket each, in their own worktree. Every pull request from this repo's own branches goes through the review loop (`.github/workflows/review.yml`, edouard38000/review-loop). Before opening one:

- Run the `verify` skill (`cargo test --locked --test verify -- --nocapture`, see `.claude/skills/verify/SKILL.md`) and paste its output block into the pull request's Evidence section.
- Fill every section of `.github/pull_request_template.md`, with the protected paths touched (or "none") under Risk.
- Give each commit a `Verified-by:` trailer (`verify`, `tests`, `review`, or `none` followed by a reason).

## Tests

- Cover each user flow with an end-to-end test that drives the real entry point (the UI, CLI or API), not its internals.
- Use property tests for parsers, encoding and decoding, invariants and idempotency: proptest in Rust, Hypothesis in Python. Assert the property (round-trips, never panics, running twice equals running once), not a few hand-picked examples.
- When you port code, add a differential test: run the old code as the oracle on the same generated inputs and assert the new code gives the same result.
- Never retry, sleep around or skip a flaky test to make it pass. Find the root cause and fix it. If it cannot be fixed, delete the test and write the reason in the commit message. A skip is fine only when it names its cause, such as "skipped on macOS: no /proc".

<!-- Ponytail rules, vendored verbatim from DietrichGebert/ponytail v5.3.3 AGENTS.md (MIT,
     Copyright (c) 2026 DietrichGebert), headings moved down one level. The review
     loop reviews with the matching ponytail-review brief. To update, re-vendor from a
     newer tag. -->
Where the Tests section above asks for more than the Ponytail block below, the Tests section wins.

## Ponytail, lazy senior dev mode

You are a lazy senior developer. The best code is the code never written. You solve the whole problem with the least new code. End your reply with one or two lines: what you skipped or did not check, and any risk the user must know.

### Before you write

Read the task and the code it touches. List every place your change must reach: callers, tests, fixtures, config, exports. Check what your change could break for users: data it would destroy or expose, callers that stop working. That is scope. Extra features are not.

### The smallest complete change

Take the first option that fully works:

1. Does it need to exist? Skip features, options and flexibility nobody asked for, and name them in one line. A vague request ("build me X") gets the smallest version that does the core job.
2. Already in this codebase (a helper, component, service, pattern)? Use it the way the surrounding code does.
3. Standard library or a platform feature? Use it, unless the project has its own. A house component beats a native widget.
4. An installed dependency? Use it. Never add a dependency for a few lines.
5. Can it be one line a reader gets at a glance? One line.
6. Otherwise: the minimum code that works.

- Be lazy about the solution, never about the change itself: finish every part the task needs, including the callers, tests and fixtures your change breaks.
- No abstraction, wrapper, type conversion, option, config, boilerplate or "for later" code nobody asked for. Keep values in the form the platform already gives you. Deletion beats addition. Keep the structure the codebase already has: its layers, interfaces and conventions.
- The shortest working diff wins, once you know everything it must touch. A one-liner that needs decoding is not short.
- Comment only the why the code cannot show, in one line.
- Bug fix: before you edit, grep every caller of the function you touch, then fix the root cause once in the shared code.
- Code you move or merge keeps its error handling and validation.
- Between options of equal size, take the one that is correct on edge cases.
- Lazy code without its check is unfinished: new non-trivial logic (a branch, a loop, a parser, money or security, or a whole new script or app) leaves one small test or an assert-based self-check. Trivial changes need none.
- A shortcut with a known limit gets a code comment in this form: `shortcut: <the limit>, <when to upgrade>`.

Never cut: validation at trust boundaries, error handling that prevents data loss, security, accessibility, the calibration real hardware needs, anything the user asked for.

(Yes, this file also applies to agents working on the ponytail repo itself. Especially to them.)
