# hackathon-radar

A self-hosted hackathon tracker. An AI agent running on your own machine finds hackathons that fit where you live and what you care about, tracks application deadlines, and texts you when something new or travel-funded appears.

> **Status:** early development. Runs read one Source and send Alerts through ntfy.

## Principles

- **Self-hosted only.** Everything runs and stays on your machine. There is no hosted service.
- **Bring your own agent and messaging.** You run your own AI agent and your own iMessage line (Photon).
- **Personal use only.** It reads public hackathon listings for your own tracking. Respect each source's terms of use.
- **Rust first** wherever the platform allows.

## Try it

`hackathon-radar run --dry-run` reads the [hackathon.be](https://hackathon.be) calendar, keeps the Hackathons that haven't ended and are in your Profile's Countries, stores them in a local SQLite database, and prints the Alert it would send. A dry run sends nothing and marks nothing as Announced.

```sh
cargo run -- run --dry-run --profile examples/profile.toml
```

Without `--dry-run`, the Alert goes to the Alert channel in your Profile. For now that is [ntfy](https://ntfy.sh): install the ntfy app, subscribe to a hard-to-guess topic, and uncomment the `[alert_channel]` section of the example Profile with that topic. Your first Alert is a Welcome Alert listing up to 6 Hackathons that already Fit; later Runs only tell you about Hackathons that newly Fit, each one once, and send nothing when nothing is new. If your ntfy server needs an access token, set it in `HACKATHON_RADAR_NTFY_TOKEN`; it never goes in the Profile. With no Alert channel set, the Alert is printed instead.

Your Profile is a small TOML file with your Home, Countries and Alert channel; see [`examples/profile.toml`](examples/profile.toml). By default it is read from `~/.config/hackathon-radar/profile.toml` and the database lives at `~/.local/share/hackathon-radar/hackathon-radar.sqlite` (both honour `XDG_CONFIG_HOME` and `XDG_DATA_HOME`). Pass `--profile` and `--db` to use other paths.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Tests never touch the network: Sources read saved fixtures from `tests/fixtures/`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
