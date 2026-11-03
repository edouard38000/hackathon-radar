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

## Open questions for the grilling

- **Results storage:** a Google Sheet like today, or a local store (file or SQLite) with an optional Sheet export.
- **Agent runtime:** a Claude app scheduled task (needs the app open), or headless `claude -p` started by cron or launchd from a Rust CLI.
- **What the Rust CLI owns:** setup wizard, preferences file, deterministic feed fetching (to cut agent cost), notifications.
- **Fallback alerts** for users without Photon (ntfy, email), or Photon only.
- **License:** MIT or Apache-2.0.
