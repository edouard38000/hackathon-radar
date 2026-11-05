# The agent can only search and read; the CLI does all writing

The person's Agent runs headless with web search and page reading only: no shell, no file writes, nothing else. It returns its judgements as JSON matching a schema the CLI provides, and the CLI validates and stores them. Each Run is capped by a number of agent steps rather than dollars, because most people use their subscription, where a dollar cap means nothing. The personal version let the agent write the Google Sheet and calendar directly; we don't, because strangers will run this unattended every week on their own machines, and an agent that reads arbitrary web pages must not be able to act on anything it reads there.

How each Agent is held to this:

- **Claude Code**: `claude -p --permission-mode dontAsk --tools WebSearch,WebFetch --allowedTools WebSearch,WebFetch --strict-mcp-config --setting-sources "" --max-turns <n> --output-format json --json-schema <schema>`. Without the extra flags, the person's own user settings (allow rules such as `Bash(...)`), hooks and MCP servers would still apply: `--tools` makes web search and page reading the only tools that exist, `--strict-mcp-config` with no `--mcp-config` loads no MCP servers, and `--setting-sources ""` loads no user, project or local settings, so no allow rules or hooks (the person's sign-in still works, which is why `--bare` is not used).
- **Codex**: `codex exec --sandbox read-only` with live search and `--output-schema <schema>`.
- **Any other coding agent**: a command template in the Profile. The setup guide states the read-only requirement, and the CLI rejects output that doesn't match the schema.

Never use `bypassPermissions`, `--dangerously-skip-permissions` or a writable sandbox.
