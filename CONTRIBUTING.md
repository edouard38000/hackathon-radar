# Contributing to hackathon-radar

Thanks for helping. Start with [`docs/background.md`](docs/background.md) for what the project is and why, [`GLOSSARY.md`](GLOSSARY.md) for the words it uses (Profile, Run, Candidate, Alert, Announced…), and [`docs/adr/`](docs/adr/) for the decisions behind it. Please use the glossary's terms in issues, PRs, code and docs.

## Build and test

You need a stable Rust toolchain (1.88 or later). The Photon sidecar also needs Node.js 20.18.1 or later, but only if you touch `sidecar/photon/`.

```sh
cargo build
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
(cd sidecar/photon && npm ci && npm test)   # only for changes to the Photon sidecar
```

CI runs exactly these on macOS and Linux, and a PR must pass them. Tests never touch the network, a real Agent, Photon or your keychain: Sources read saved fixtures from `tests/fixtures/`, and secrets go to a fake keychain. Keep it that way; a test that needs the network belongs in the live test.

The live end-to-end test (`tests/live.rs`) is the one exception. It runs the real binary against real Sources, a real `claude` and ntfy.sh, so it is ignored by default and the maintainer runs it before a release. See the README's [Development](README.md#development) section.

## Rust first

hackathon-radar is Rust (cargo, clippy, rustfmt). Use another language only where Rust isn't possible or sensible, such as the Photon sidecar, which exists because Photon only ships a TypeScript SDK ([ADR 0005](docs/adr/0005-photon-node-sidecar.md)), and say why in the PR.

## Issues

Issues live on GitHub. Use the bug report or feature request template. The maintainer triages them with these labels (see [`docs/agents/triage-labels.md`](docs/agents/triage-labels.md)):

| Label | Meaning |
| --- | --- |
| `needs-triage` | The maintainer needs to evaluate it |
| `needs-info` | Waiting on the reporter for more information |
| `ready-for-agent` | Fully specified, ready for a coding agent to build |
| `ready-for-human` | Needs a person to build it |
| `wontfix` | Won't be done |

Larger work is written up as a spec issue and split into tickets, each a sub-issue of its spec with its blockers linked.

## Pull requests

- **One PR per issue.** Keep it to that issue's acceptance criteria, and put `Closes #<issue>` in the description.
- Fill in the pull request template: what changed, the acceptance criteria, how you tested it, and anything you couldn't do.
- Don't edit existing database migrations or weaken existing tests; add a new migration or a new test instead.
- Never commit secrets (API keys, tokens, Photon credentials, your phone number). Profiles hold no secrets by design; keys go to the keychain.
- An Agent may only search and read the web ([ADR 0004](docs/adr/0004-agent-is-read-only.md)). A change that gives it any other tool, or loosens its flags, needs a new ADR first.

## Coding agents

Coding agents (Claude Code, Codex and others) working in this repo follow [`AGENTS.md`](AGENTS.md): the Rust-first rule, the issue tracker and labels, and the domain docs. If you use one to write your PR, it should read `AGENTS.md` first.

## Licence

By contributing, you agree that your contributions are licensed under the project's dual MIT or Apache-2.0 licence, as described in the [README](README.md#license).
