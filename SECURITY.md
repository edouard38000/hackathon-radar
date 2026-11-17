# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately, through GitHub's private vulnerability reporting: open the repository's **Security** tab and choose **Report a vulnerability**. Don't open a public issue, pull request or discussion for a security problem.

Include what you found, how to reproduce it, the version (`hackathon-radar --version`) and platform, and what an attacker could do with it. You'll get an answer as soon as the maintainer can look at it. hackathon-radar is maintained by one person in their spare time, so there is no fixed response time, and no bug bounty.

Only the latest release is supported; fixes go into the next release.

## What's in scope

hackathon-radar runs unattended on people's own machines and reads arbitrary web pages, so these are the parts that matter most:

- **Secrets handling.** The Photon credentials, the SerpApi key and the ntfy token live in the operating system's keychain (service `hackathon-radar`), or in an owner-only `secrets.toml` when there is no keychain, never in the Profile. hackathon-radar must never print a secret, write it into the Profile, a log, a report or an Alert, put it on a command line, or read a secrets file other people can read. A way to get a secret out, or into the wrong place, is in scope.
- **The Agent's read-only lockdown** ([ADR 0004](docs/adr/0004-agent-is-read-only.md)). The Agent may only search and read the web, and its answer is validated against a schema before anything is stored. A listing or web page that gets the Agent to run a command, write a file, use another tool, load the person's own settings, hooks or MCP servers, or makes the CLI act on something other than validated JSON, is in scope. So are flag changes in a Claude Code or Codex release that quietly widen what the Agent can do.
- **The Photon sidecar** ([ADR 0005](docs/adr/0005-photon-node-sidecar.md)) in `sidecar/photon/`: how the CLI finds and starts it, how the credentials reach it (in its environment, never on its command line), its pinned dependencies with install scripts turned off, and what it prints back.
- **The schedule** (launchd or systemd): what it runs, with which `PATH`, and the files it writes.
- **Everything the CLI writes:** the Profile, the SQLite database, Run logs, reports, exports and Alerts, including text from a listing that could inject markup into an HTML report or a calendar export.

## What's out of scope

- Vulnerabilities in the services and tools hackathon-radar uses (Claude Code, Codex, Photon, ntfy, SerpApi, the Sources): report those to their owners. How hackathon-radar uses them is in scope.
- That anyone who knows an ntfy topic on ntfy.sh can read it. That's how public ntfy topics work; the setup asks for a long, hard-to-guess topic, or you can use your own server with a token.
- Attacks that need control of the person's own account or machine already.
- Listings that are wrong, spammy or misleading but don't make hackathon-radar do anything beyond showing them.
