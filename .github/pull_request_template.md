Closes #

## What changed

<!-- What this PR does, in the GLOSSARY.md terms (Profile, Run, Candidate, Alert…). One PR per issue. -->

## Acceptance criteria

<!-- Copy the issue's acceptance criteria and tick each one you met. Say why for any you didn't. -->

- [ ]

## Tests

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets --locked -- -D warnings`
- [ ] `cargo test --locked`
- [ ] `(cd sidecar/photon && npm ci && npm test)`, if the Photon sidecar changed

<!-- New or changed tests, and how you checked the change by hand. Tests never touch the network; only the ignored live test (tests/live.rs) does. -->

## Checklist

- [ ] Rust first: any non-Rust code says why it can't be Rust.
- [ ] No secrets (keys, tokens, Photon credentials, phone numbers) in the code, tests, fixtures or this description.
- [ ] No existing migration edited and no existing test weakened.
- [ ] Docs updated (README, setup guide, `GLOSSARY.md`, an ADR) where behaviour or a decision changed.

## Gaps

<!-- Anything uncertain, not verified, or left for later. -->
