# claude-code-proxy

This fork adds Windows credential protection and local-proxy safeguards to
[Raine Virta's upstream project](https://github.com/raine/claude-code-proxy),
starting from `v0.1.46`. See [SECURITY.md](SECURITY.md) for the verified findings,
design, migration behavior and limitations. Upstream's MIT license is preserved.
For PowerShell build, validation and authenticated startup, see [WINDOWS.md](WINDOWS.md).

Run `claude-code-proxy claude` to launch Claude with a picker populated from the
authenticated Codex catalog and explicit context/effort configuration. On first
launch without `--model`, choose an available model to save as the proxy's default.
Later launches reuse it; `--model` overrides it for one launch. Use
`--only-model` for the selected model's full default context window.

Claude Code, powered by **OpenAI Codex**, **Kimi**, **Grok**, **OpenCode Go**,
or **Cursor Agent**.

Docs: <https://claude-code-proxy.raine.dev>

LLM docs: <https://claude-code-proxy.raine.dev/llms.txt>

<img src="meta/claude-code-screenshot-2026-07.webp" alt="Claude Code running through claude-code-proxy" />

<a href="https://aventasks.dev/">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://aventasks.dev/readme/aven-banner-dark.webp">
    <img src="https://aventasks.dev/readme/aven-banner-light.webp" alt="aven: one task queue for you and your coding agents" width="100%">
  </picture>
</a>

<sub>Also from the author of claude-code-proxy: <a href="https://aventasks.dev/"><b>aven</b></a>, one
queue for every project, with a keyboard-driven TUI for you and a CLI for your
agents.</sub>

## Why?

Claude Code remains an excellent coding harness, with strong tools, skills,
hooks, subagents, and editor integrations. claude-code-proxy keeps that client
experience while translating its Anthropic API traffic for subscription-backed
provider services.

One local process handles provider authentication, model-based routing,
protocol translation, streaming responses, and diagnostics. The built-in
monitor shows sessions, active and recent requests, errors, token usage, and
throughput.

## Quick start with Codex

In this fork, run `claude-code-proxy` (or `claude-code-proxy claude`) to launch
Claude Code and its local proxy together. On first interactive launch, it opens
the existing browser sign-in flow when no authentication is stored, then fetches
models and asks you to save a default. Existing authentication is reused and
expired tokens use normal refresh. Storage, refresh and network errors do not
silently trigger a new sign-in. Noninteractive launches and `--print-config`
require authentication to be configured beforehand. Standalone proxy mode remains
available explicitly through `claude-code-proxy serve`.

Install on macOS or Linux:

```sh
brew install raine/claude-code-proxy/claude-code-proxy
```

Or use the release installer:

```sh
curl -fsSL https://raw.githubusercontent.com/raine/claude-code-proxy/main/scripts/install.sh | bash
```

Windows and other prebuilt artifacts are available from
[GitHub Releases](https://github.com/raine/claude-code-proxy/releases).

Sign in with a **ChatGPT Plus or Pro account**, not an OpenAI API account:

```sh
claude-code-proxy codex auth login
```

Start the proxy in one terminal:

```sh
claude-code-proxy serve
```

For a background service, use `claude-code-proxy serve --no-monitor` and attach
from another terminal with `claude-code-proxy monitor`. Closing an attached
dashboard leaves the proxy running.

Start Claude Code in another:

```sh
ANTHROPIC_BASE_URL=http://127.0.0.1:18765 \
ANTHROPIC_AUTH_TOKEN=unused \
ANTHROPIC_MODEL=gpt-6-sol[1m] \
ANTHROPIC_SMALL_FAST_MODEL=gpt-6-luna[1m] \
CLAUDE_CODE_AUTO_COMPACT_WINDOW=272000 \
CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK=1 \
  claude
```

See [Getting started](https://claude-code-proxy.raine.dev/getting-started/)
for the complete first session.

Optional Codex image generation and editing can reuse the same ChatGPT login:

```sh
CCP_CODEX_IMAGES_API=1 claude-code-proxy serve
curl http://127.0.0.1:18765/v1/images/generations \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"A paper-cut fox","model":"gpt-image-2"}'
```

The opt-in Images API returns base64 image data and consumes the signed-in account's image quota. Image prompts and payloads are excluded from traffic captures. See the [HTTP API](https://claude-code-proxy.raine.dev/reference/http-api/) for generation and edit schemas.

## Providers

| Provider     | Account                        | Model selection                                 |
| ------------ | ------------------------------ | ----------------------------------------------- |
| Codex        | ChatGPT Plus or Pro            | Registered `gpt-*` models, `-fast`/`-ultrafast` |
| Kimi         | kimi.com with Kimi Code access | `kimi-for-coding` and aliases                   |
| Grok         | grok.com                       | Registered Grok models                          |
| OpenCode Go  | OpenCode Go subscription       | Non-conflicting IDs and `opencode-go/<model-id>` |
| Cursor Agent | Cursor account                 | Cursor aliases and `cursor:<model-id>` prefixes |

Run `claude-code-proxy models` for the current catalog or
`claude-code-proxy models --full` for every dynamic Cursor alias.

> [!WARNING]
> The proxy binds to `127.0.0.1` by default and accepts local requests without
> client authentication unless `CCP_CLIENT_TOKEN` is set. Non-loopback binding
> requires both `CCP_CLIENT_TOKEN` and `CCP_ALLOW_NETWORK=1`; use a trusted TLS
> tunnel for remote access. Provider subscriptions, model access, terms,
> and account enforcement remain under each provider's control. Unofficial
> clients may carry account risk.

To enable client authentication, set `CCP_CLIENT_TOKEN` to a secret of at least
32 printable ASCII characters without spaces in the proxy environment. Set
`ANTHROPIC_AUTH_TOKEN` to the same value in Claude Code. All routes, including
health and monitor endpoints, require `Authorization: Bearer <token>` or
`x-api-key: <token>`. Run the attached monitor with the same `CCP_CLIENT_TOKEN`.

On Windows, the file credential store uses user-scoped DPAPI protection and
migrates existing plaintext credentials after verifying the protected copy.
Routine logs omit error prose and request/response bodies. `CCP_LOG_CONTENT=1`
enables those fields, and `CCP_ERROR_CAPTURE=1` enables error capture files;
these opt-ins can write sensitive content. Rotated logs retain three files,
and error captures retain up to 100 files.

## Documentation

- [What is claude-code-proxy?](https://claude-code-proxy.raine.dev/)
- [Choosing a provider](https://claude-code-proxy.raine.dev/providers/choosing-a-provider/)
- [Configure Claude Code](https://claude-code-proxy.raine.dev/using/configure-claude-code/)
- [Models and routing](https://claude-code-proxy.raine.dev/using/models-and-routing/)
- [Monitor TUI](https://claude-code-proxy.raine.dev/using/monitor-tui/)
- [Troubleshooting](https://claude-code-proxy.raine.dev/using/troubleshooting/)
- [Command reference](https://claude-code-proxy.raine.dev/reference/command-reference/)
- [Configuration](https://claude-code-proxy.raine.dev/reference/configuration/)
- [HTTP API](https://claude-code-proxy.raine.dev/reference/http-api/)
- [Compatibility and limitations](https://claude-code-proxy.raine.dev/reference/compatibility-and-limitations/)
- [For coding agents](https://claude-code-proxy.raine.dev/using/for-coding-agents/)

## Related projects

- [aven](https://github.com/raine/aven): local-first task management for power
  users and agents
- [claude-history](https://github.com/raine/claude-history): search Claude Code
  conversation history from the terminal
- [git-surgeon](https://github.com/raine/git-surgeon): non-interactive
  hunk-level git staging for coding agents
- [workmux](https://github.com/raine/workmux): parallel coding tasks in git
  worktrees and tmux
- [consult-llm](https://github.com/raine/consult-llm): consult other AI models
  from an agent workflow

## License

[MIT](LICENSE)
