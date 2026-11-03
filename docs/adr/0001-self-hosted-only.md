# Self-hosted only, no hosted service

hackathon-radar is open source and runs entirely on each person's own machine: their own AI agent, their own Alert channel, and data that never leaves their computer. We deliberately do not offer a hosted version, because it must cost the maintainer nothing to run, and a hosted service would mean paying for agent runs, messaging and storage on behalf of strangers, plus holding their phone numbers.

## Consequences

- Each person brings their own coding agent (Claude Code, Codex or another), normally on the subscription they already pay for, with an API key only as a fallback, plus their own messaging line (Photon). So there's no per-Run cost beyond what they already have, but setup has to be guided.
- Setup is done by the person's own coding agent: they open the repo in it and ask to be set up (see ADR 0007).
- The README states the tool is for personal use only, because it reads public hackathon listings whose terms differ by source.

## Considered options

- **Hosted service:** rejected. It means paying for strangers' agent runs, messages and fare lookups, and holding their phone numbers.
- **Hybrid, left for later:** a shared public Hackathon list. The deterministic collection step would run once (for example in a scheduled GitHub Action on the public repo) and publish a static file. Each install would read that file instead of scraping every Source itself, while judging, Profiles and Alerts stay local. That cuts duplicated scraping across installs. We kept v1 fully self-hosted; revisit this once there are enough users for the duplicated scraping to matter, checking GitHub's Actions terms and each Source's terms first.
