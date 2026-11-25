<!-- What changed, Evidence, Risk and How to revert come from edouard38000/review-loop templates/pull_request_template.md. The reviewer grades a missing or empty section Should fix. -->

Closes #

## What changed

<!-- What this PR does, in the GLOSSARY.md terms (Profile, Run, Candidate, Alert…), and the ticket it closes. One PR per issue. -->

## Acceptance criteria

<!-- Copy the issue's acceptance criteria and tick each one you met. Say why for any you didn't. -->

- [ ]

## Evidence

<!-- What you ran to try the change for real, and what you saw: a screenshot, command output or test log. "Tested locally" alone counts as empty. Paste the block the verify skill prints (`cargo test --locked --test verify -- --nocapture`), and list new or changed tests. Tests never touch the network; only the ignored live test (tests/live.rs) does. -->

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets --locked -- -D warnings`
- [ ] `cargo test --locked`
- [ ] `(cd sidecar/photon && npm ci && npm test)`, if the Photon sidecar changed

## Risk

<!-- The protected paths this touches (workflows, CLAUDE.md, AGENTS.md, .claude/, this template, the repo's high-risk code), or "none". -->

## How to revert

<!-- How to undo it: "revert this commit", plus any data, migration or deploy step a revert misses. -->

## Checklist

- [ ] Rust first: any non-Rust code says why it can't be Rust.
- [ ] No secrets (keys, tokens, Photon credentials, phone numbers) in the code, tests, fixtures or this description.
- [ ] No existing migration edited and no existing test weakened.
- [ ] Docs updated (README, setup guide, `GLOSSARY.md`, an ADR) where behaviour or a decision changed.

## Gaps

<!-- Anything uncertain, not verified, or left for later. -->
