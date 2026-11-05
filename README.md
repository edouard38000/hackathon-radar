# hackathon-radar

A self-hosted hackathon tracker. An AI agent running on your own machine finds hackathons that fit where you live and what you care about, tracks application deadlines, and texts you when something new or travel-funded appears.

> **Status:** early development. Only a dry run works so far.

## Principles

- **Self-hosted only.** Everything runs and stays on your machine. There is no hosted service.
- **Bring your own agent and messaging.** You run your own AI agent and your own iMessage line (Photon).
- **Personal use only.** It reads public hackathon listings for your own tracking. Respect each source's terms of use.
- **Rust first** wherever the platform allows.

## Try it

`hackathon-radar run --dry-run` reads the [hackathon.be](https://hackathon.be) calendar, keeps the Hackathons that haven't ended and are in your Profile's Countries, stores them in a local SQLite database, and prints the Alert it would send. Nothing is sent: Alert channels come later, so even without `--dry-run` the Alert is only printed.

```sh
cargo run -- run --dry-run --profile examples/profile.toml
```

Your Profile is a small TOML file with your Home and Countries; see [`examples/profile.toml`](examples/profile.toml). By default it is read from `~/.config/hackathon-radar/profile.toml` and the database lives at `~/.local/share/hackathon-radar/hackathon-radar.sqlite` (both honour `XDG_CONFIG_HOME` and `XDG_DATA_HOME`). Pass `--profile` and `--db` to use other paths.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Tests never touch the network: Sources read saved fixtures from `tests/fixtures/`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
