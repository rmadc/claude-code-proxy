# Validation record

Environment: Windows 11, PowerShell, Rust 1.99.0 MSVC, Python 3.13.
Validation date: 2026-10-24. Baseline: upstream `v0.1.46` at
`5261067510fd00dd5efcadab3ecf79b8e02b4f80`.

## Local results

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --locked --offline --all-targets -- -D warnings` | Pass |
| `cargo build --release --locked --offline` | Pass; built from inspected source, not executed against an account |
| Library unit tests | 943 passed |
| CLI unit tests | 6 passed |
| Integration tests across 15 test binaries | 180 passed |
| Documentation tests | 0 present; command passes |
| `cargo audit --no-fetch --json` | Zero vulnerability advisories; three documented warnings, no ignores |
| `git diff --check` | Pass |

The RustSec database was refreshed before the audit. Its commit was
`21d909e4c8bd4e14c7aea58bf980c881a0053ab1`.
Warnings and their limited reachability assessment are in [SECURITY.md](SECURITY.md).

Tests ran through `scripts/check-isolated.py`, with disposable home,
configuration, application-data and state directories and synthetic credentials.
Successful DPAPI tests ran outside the restricted agent sandbox, which cannot
access Windows DPAPI key material. The test wrapper isolates provider credentials
in both contexts. Only GitHub CLI was authorized for fork/push; no provider
sign-in, native credential import, real-provider API request, or downloaded proxy
release binary was used.

## Commands and scope

The final passing runs were split so a failure could be corrected and its
affected suite rerun without repeatedly executing already-passing suites:

```text
python scripts/check-isolated.py --lib --bin claude-code-proxy -- --test-threads=1
python scripts/check-isolated.py --test '*' -- --test-threads=1
python scripts/check-isolated.py --test server --test smoke_cutover --test security_diagnostics -- --test-threads=1
python scripts/check-isolated.py --test smoke_cutover -- --test-threads=1
python scripts/check-isolated.py --doc
```

The all-integration run passed through path/authentication tests, then stopped at
an upstream test expecting unauthenticated wildcard binding. Its expectation was
updated to the intentional rejection. The server suite and new diagnostic suite
then passed. Five smoke capture fixtures still selected the old Unix state
variable on Windows; after correcting them to `LOCALAPPDATA`, all 55 smoke tests
passed. All earlier integration binaries passed in the all-integration run.

Earlier validation also found and corrected a concurrent first-load DPAPI
migration race and preserved auto-review continuation isolation when no model
override is selected. The migration has a 12-thread regression test. An upstream
50 ms WebSocket timing test now uses Tokio's paused clock; its assertion remains.

To rerun everything in one invocation:

```powershell
python scripts/check-isolated.py -- --test-threads=1
```

Linux/Windows GitHub CI is configured in `.github/workflows/ci.yml`. Local results
above establish Windows coverage; hosted CI results are reported separately.
macOS, ARM release targets, documentation-site dependencies, live subscription
compatibility and the release-publishing workflow were not exercised locally.
