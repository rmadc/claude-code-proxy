# Security review of this fork

This is a security-focused fork of [Raine Virta's claude-code-proxy](https://github.com/raine/claude-code-proxy).
The MIT license and upstream copyright remain in `LICENSE`.

Baseline: tag `v0.1.46`, commit `5261067510fd00dd5efcadab3ecf79b8e02b4f80`.
On 2026-10-24, upstream's latest release and default branch both pointed to
this commit. No baseline change was made.

## Findings verified against the baseline

| Observation | Source and practical impact |
| --- | --- |
| Windows access/refresh tokens are plaintext JSON | `src/auth.rs`: Windows uses `FileAuthStore`; `set_mode` only protects Unix permissions. Provider token stores enable Keychain only on macOS. A readable file or backup exposes a reusable credential; no remote exploit was demonstrated. |
| Local API requests have no client authentication | `src/server.rs`: API routes have no authentication middleware. Loopback binding limits default reachability, but other local processes can use the subscription. `/monitor` separately checks the actual loopback peer. |
| Captures and logs can contain sensitive content | `src/traffic.rs`: traffic capture is already opt-in and redacts known keys, not arbitrary prose. `record_failed_response` in `src/server.rs` automatically persists error bodies. `src/logging.rs` rotates logs but does not prune old rotations. Prompts, paths, source code and echoed secrets can survive redaction. |
| Auto-review defaults independently to `gpt-6-luna` | `apply_auto_review_model` in `src/server.rs` provides that Codex default. `CCP_AUTO_REVIEW_MODEL` / `autoReviewModel` already supports an override. This is a policy/model-selection concern, not a proven authorization bypass or proof of a model's quality. |
| Platform names are incorrect | `src/paths.rs` compares `win32` / `darwin` while `std::env::consts::OS` supplies `windows` / `macos`. Windows therefore falls through to XDG paths; a custom macOS XDG path can also differ from intended behavior. A naive fix would strand configuration. |
| Release actions/toolchain are mutable | `.github/workflows/release.yml` uses action version tags and `rust-toolchain@stable`. This weakens build reproducibility and leaves future builds exposed to changes in those references; it does not establish a compromised release. |

## Changes and design

* Windows file-backed provider credentials use user-scoped DPAPI in
  `auth.json.dpapi`, including when `CCP_CONFIG_DIR` is set. Migration encrypts,
  writes and syncs an atomic sidecar, then decrypts and verifies its persisted
  contents before removing the source plaintext. Errors preserve that source.
  First-load migrations are serialized within the process; concurrent requests
  cannot race plaintext retirement. A 12-thread regression test covers this.
  A damaged protected copy fails closed. New writes never fall back to plaintext.
  This reuses the existing provider authentication architecture and Windows APIs;
  it does not import native Codex credentials or change OAuth.
* Native Rust OS names work. Existing Windows XDG/home configuration directories
  and existing custom macOS XDG configuration directories remain in use.
  `CCP_CONFIG_DIR` takes precedence. State moves to the proper Windows local
  application-data location; old logs/captures are left untouched.
* Loopback remains the default. `CCP_CLIENT_TOKEN` optionally authenticates every
  route with Bearer or `x-api-key`, using SHA-256 and `subtle` comparison.
  Configured tokens must contain at least 32 printable ASCII characters without
  spaces; use a randomly generated value. Invalid configuration fails closed.
  Network listeners additionally require `CCP_ALLOW_NETWORK=1`, including
  externally supplied listeners passed to `serve_listener`.
* Error captures require `CCP_ERROR_CAPTURE=1`; routine error/body log fields
  require `CCP_LOG_CONTENT=1`. Known-key redaction includes camel-case token keys.
  Logs rotate at 20 MiB and retain three rotations. Error captures retain 100
  files in sequential operation. Unicode truncation no longer slices inside a
  UTF-8 character. Traffic capture remains opt-in with `CCP_TRAFFIC_LOG=1`.
* Auto-review inherits normal model routing unless explicitly overridden by
  `CCP_AUTO_REVIEW_MODEL` / `autoReviewModel`; `inherit` cancels that override.
  Classifier requests remain stateless and cannot update session affinity even
  when inheriting the main model. Detection remains a system-prompt heuristic.
* Release and CI actions are pinned to resolved commit hashes; Rust is pinned
  to `1.99.0`. CI checks Windows and Linux. Release publishing alone receives
  repository write permission, uses the repository token, and the upstream
  Homebrew update job cannot run from this fork. No release tag is published.

## Dependencies and build inspection

The project has no root `build.rs` or Cargo build hook. The Cargo manifest,
release workflows, `justfile`, check configuration and hook shim were inspected
before compiling. Locked dependency build-script inspection found compiler
feature probes, generated tables/configuration, Windows link setup and `ring`'s
C/assembly build; this is not an exhaustive audit of dependency code.

The initial RustSec audit reported 12 vulnerability advisories. Targeted lockfile
updates patched `h2`, `quinn-proto`, `rustls`, `rustls-webpki`, `webpki-roots`, and
the additional Tokio I/O-safety warning. The follow-up audit reported zero
vulnerability advisories. Advisory database commit:
`21d909e4c8bd4e14c7aea58bf980c881a0053ab1` (2026-10-24).

Remaining warnings are `paste` 1.0.15 being unmaintained
(`RUSTSEC-2024-0436`) and `lru` 0.12.5 unsoundness
(`RUSTSEC-2026-0002`, `RUSTSEC-2026-0253`), through Ratatui 0.29.
Ratatui's layout cache uses `get_or_insert`; inspection found no calls to the
flagged `LruCache::pop` or `IterMut` APIs. This is limited reachability evidence,
not proof of safety. No advisory ignores are configured. A larger Ratatui upgrade
is deferred to avoid mixing a TUI API migration into this hardening work.

## Validation

`python scripts/check-isolated.py` runs Cargo tests with disposable configuration,
home, application-data and state directories, and removes inherited provider
configuration from the child environment. Only synthetic credentials and local
mock upstreams are used. Windows DPAPI tests require a normal Windows user
context: the restricted agent sandbox cannot access DPAPI key material.

Tests cover encrypted migration, persisted round trips and token replacement,
failed migration preserving plaintext, corruption failing closed, legacy paths
and overrides, router authentication and binding guards, review routing and
continuation isolation, redaction and retention. The Windows timeout test uses
Tokio's paused clock instead of depending on 50 ms of wall-clock scheduling.
See `VALIDATION.md` for final results and commands.

## Remaining limitations

* DPAPI does not isolate credentials from malware or another process running as
  the same user. Tokens exist in process memory; memory dumps and privileged
  access remain risks. Protect the Windows account, backups and disk encryption.
* Plaintext deletion is not secure erasure. Historical copies, backups and
  snapshots remain. A failed or interrupted migration can leave plaintext;
  inspect migration errors and do not discard the verified protected copy.
  Upstream binaries cannot read this fork's encrypted format.
* Optional authentication means unauthenticated loopback use is still possible
  when no token is configured. Tokens live in process environments; do not put
  them in source control, shell history or diagnostic captures. Length checks
  cannot establish randomness. There is no per-client policy or rate limit.
* The proxy listener is HTTP, not TLS. Network opt-in is not encryption: use a
  trusted TLS tunnel, network firewall and authentication for remote access.
* Diagnostic opt-ins can record arbitrary sensitive content, including secrets
  outside recognized keys and raw stream/error prose. Redaction is not a
  guarantee. Traffic captures have no automatic aggregate retention and must be
  removed after investigation. Old logs/captures are not retroactively scrubbed;
  concurrent writers can exceed retention counts temporarily.
* Action/compiler pinning does not make builds fully reproducible or signed.
  Hosted runner images and package infrastructure remain external dependencies.
  Documentation/Nix workflows retain their upstream behavior and were not
  comprehensively hardened. RustSec coverage is not an audit of every dependency.
* This unofficial integration cannot guarantee subscription compatibility,
  provider availability, account acceptance, or auto-review decision quality.
  No real sign-in or provider request was used to validate subscription access.

Do not post credentials, raw captures, or exploit details in public issues.
GitHub login performed during setup authorizes fork/push only; provider
credentials were not accessed and no upstream PR was opened.
