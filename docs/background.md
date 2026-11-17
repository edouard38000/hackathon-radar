# Background: where this comes from

This project generalises Edouard's personal hackathon tracker, which has been running since 2026-11-02.

## The personal version (the reference implementation)

- **Agent:** a Claude scheduled task (Mon and Thu 08:00) whose prompt does all the work. Each run:
  - collects candidates from hackathon.be (ICS), Luma calendars (ICS), hackathonhub.eu (`sitemap-events.xml` plus filter pages), Devpost (JSON), MLH, ETHGlobal, Cassini and dev.events, plus web search;
  - filters them with scope rules: coding or building events only; a country allowlist (BE, FR, NL, LU, DE, CH, UK); events with travel or lodging covered allowed anywhere in the world; otherwise a return flight of €200 or less to a notable event;
  - applies a 4-hour train rule from Brussels-Midi (anything longer counts as a flight);
  - requires a flight-fare lookup from BRU or CRL for every flight trip (Google Flights, then Skyscanner or Kayak, then web search);
  - re-checks deadlines, application status and travel funding for known events.
- **Storage:** a Google Sheet, with tabs Hackathons (23 columns: dates, deadline, days left, status, travel covered, accommodation, getting there, fare, fits budget, prize, theme, eligibility, links, notes), Changelog and Sources.
- **Reminders:** an all-day "⏰ Apply: <name>" event in Google Calendar on each deadline, with a popup 3 days before, for open events that fit.
- **Alerts:** at most one iMessage per run, for new events that fit and for any event whose travel or lodging becomes covered. It's sent by a small script that POSTs to Edouard's Photon iMessage bridge. iMessages sent from your own Apple ID to yourself don't trigger a notification, which is why a separate line is needed.

## Decisions already made for the public version (2026-11-03)

1. **Self-hosted, open source.** Edouard never hosts it for anyone, and it must cost him nothing.
2. **Each user runs their own AI agent**, like the personal version.
3. **Each user sets up their own Photon iMessage line.**
4. **The README states personal use only**, to respect sources' terms of use.
5. **Flight fares:** researching price sources (APIs or scraping) is fine.
6. **Users set their own preferences:** home city or airport, countries, budget, travel tolerance, themes, eligibility (for example student status).
7. **All data lives on the user's machine.**
8. **Rust first** wherever possible (cargo, clippy, rustfmt).

## What was decided

The open questions from 2026-11-03 were settled in the grilling that followed, and each decision is recorded as an ADR in [`adr/`](adr/):

- **Self-hosted only**, with no hosted version: [ADR 0001](adr/0001-self-hosted-only.md).
- **What the Rust CLI owns and how the Agent runs:** the CLI fetches, filters, stores and alerts; a launchd or systemd schedule starts the CLI, which runs the person's own Agent headless (`claude -p`, `codex exec` or a command template) only for what needs judgement: [ADR 0002](adr/0002-cli-filters-agent-judges.md). It may only search and read the web: [ADR 0004](adr/0004-agent-is-read-only.md). Its instructions ship in the binary: [ADR 0006](adr/0006-agent-instructions-ship-in-the-binary.md).
- **Results storage:** a local SQLite database is the source of truth; reports and a Google Sheet CSV are only exports: [ADR 0003](adr/0003-sqlite-source-of-truth.md).
- **Alerts:** Photon iMessage through a small Node sidecar, with ntfy as the no-signup fallback channel: [ADR 0005](adr/0005-photon-node-sidecar.md).
- **Setup** goes through the person's own coding agent, driving `hackathon-radar init`: [ADR 0007](adr/0007-coding-agents-are-the-front-door.md).
- **Duplicate listings** are matched by rules in SQLite, with the Agent settling the rest: [ADR 0008](adr/0008-duplicates-matched-without-vectors.md).
- **Worldwide** from v1, with regional Source packs: [ADR 0009](adr/0009-worldwide-with-source-packs.md).
- **Licence:** MIT or Apache-2.0, at the user's option (see the README).

The words these use are defined in [`GLOSSARY.md`](../GLOSSARY.md).
