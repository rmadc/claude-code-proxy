---
title: Files and storage
description: Canonical claude-code-proxy configuration, credential, device ID, log, error, traffic-capture, and service-log paths on macOS, Linux, and Windows.
---

claude-code-proxy separates configuration and credentials from runtime state.

## Directory roots

| Platform | Configuration root | State root |
| --- | --- | --- |
| macOS | `~/.config/claude-code-proxy` | `${XDG_STATE_HOME:-~/.local/state}/claude-code-proxy` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/claude-code-proxy` | `${XDG_STATE_HOME:-~/.local/state}/claude-code-proxy` |
| Windows | `%APPDATA%/claude-code-proxy` | `%LOCALAPPDATA%/claude-code-proxy` |

Windows falls back to `%USERPROFILE%/AppData/Roaming` and `%USERPROFILE%/AppData/Local` when the corresponding environment variable is absent.

On Windows, an existing configuration directory at
`${XDG_CONFIG_HOME:-~/.config}/claude-code-proxy` continues to be used so
configuration and credentials remain accessible. macOS likewise preserves an
existing custom XDG configuration directory from older versions. No directories
are moved or merged automatically; `CCP_CONFIG_DIR` always takes precedence.
Old state/log directories remain on disk and are not automatically relocated.

`CCP_CONFIG_DIR` replaces the configuration root for the current process. It does not change the state root.

## Configuration

`config.json` lives directly under the configuration root. See [Configuration](/reference/configuration/) for its schema and precedence.

## Provider credentials

Windows file stores write `<provider>/auth.json.dpapi`, encrypted with
user-scoped Windows DPAPI. On first load, an existing plaintext `auth.json` is
parsed, encrypted, durably written, decrypted and verified from disk before
that source plaintext file is removed. Failure preserves the plaintext source;
an unreadable protected copy causes an error instead of a plaintext fallback.
The same protection applies with `CCP_CONFIG_DIR`. Protect backups too:
deletion cannot erase old backups, filesystem snapshots, or previous disk blocks.
DPAPI protects storage, not tokens in a running process or against another
process running as the same Windows user. This encrypted format is not readable
by upstream releases; rollback requires keeping this fork or signing in again.

On macOS, Codex and Cursor use Keychain services:

- `claude-code-proxy.codex`
- `claude-code-proxy.cursor`

On Unix, Kimi and Grok use `<configuration-root>/<provider>/auth.json`, and Codex and Cursor use that file layout on Linux. On Windows, all four use the DPAPI sidecar described above. File-backed credentials are written with restrictive permissions where supported.

When `CCP_CONFIG_DIR` is set, file-backed provider credentials use
`<CCP_CONFIG_DIR>/<provider>/auth.json` (with `.dpapi` appended on Windows), including Codex and Cursor on macOS.
`CCP_CURSOR_AUTH_TOKEN` bypasses Cursor's local credential store for that
process.

OpenCode Go is the exception: it reads its API key from
`CCP_OPENCODE_API_KEY`, `OPENCODE_API_KEY`, or `opencode.apiKey` in
`config.json` and does not create a provider auth store.

The proxy owns these credentials independently of native Codex, Grok, and Cursor Agent stores.

## Kimi device ID

Kimi stores a persistent UUID at `<configuration-root>/kimi/device_id` for file-backed setups. It is bound into the Kimi token and must remain paired with that login.

## Structured log

`proxy.log` lives under the state root. It uses JSON Lines and rotates at 20 MiB, retaining three rotated files. Known credential keys, including authorization, access tokens, refresh tokens, ID tokens, and account headers, are redacted before writing. Routine error prose and body fields are omitted unless `CCP_LOG_CONTENT=1`. Arbitrary secrets embedded in other metadata are not guaranteed to be detected.

A Homebrew service also writes `service.log` under the state root.

## Failed responses

Error capture is disabled by default. `CCP_ERROR_CAPTURE=1` writes failed response JSON under `errors/` in the state root, retaining up to 100 files in ordinary sequential use. A `request_failed` log event includes `errorFile`. Concurrent writers may briefly exceed the limit.

Error payloads can contain credentials echoed in prose, prompts, and paths despite known-key redaction. Treat them as sensitive and delete them after investigation. Existing captures are not retroactively scrubbed.

## Traffic captures

Set `CCP_TRAFFIC_LOG=1` to create captures under `traffic/` in the state root. Requests are grouped by Claude Code session and request sequence. A request directory can include:

- inbound Anthropic request and metadata
- translated upstream request
- upstream URL and redacted headers
- raw or decoded upstream events
- translated downstream events and response
- transport or reducer error details

Event filenames use monotonic sequence numbers so lexical order preserves emission order.

<div class="security-callout">
<strong>Traffic captures preserve content.</strong> Header and token redaction does not remove prompts, source code, tool definitions, tool inputs, tool results, images, or provider output. Treat the entire directory as sensitive user data.
</div>

Enable capture only for a focused reproduction, keep the directory local, and delete it after the investigation.
