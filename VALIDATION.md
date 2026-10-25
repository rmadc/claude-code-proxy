# Validation record

## Automatic first-run sign-in

The launcher now reuses the existing browser login only when credentials are
absent and the session is interactive. A bare executable launch selects the
Claude launcher; standalone server mode remains explicit through `serve`.
Synthetic tests cover login persistence, existing authentication, storage errors,
cancelled login and noninteractive operation. The new CLI regression and compiled
release checks confirm that bare launch, `claude`, and `claude --print-config`
fail without starting browser authentication when run noninteractively with an
empty disposable profile. Formatting, strict Clippy and the locked release build
passed. The full isolated suite passed (953 library, 7 binary and 181 integration
tests), followed by the additional CLI regression test. No real sign-in was
performed to validate this change.

## Windows launcher packaging follow-up

Validation on 2026-10-25:

* Correction after direct-launch testing: Windows configuration defaults to
  `%APPDATA%` (Roaming), not `%LOCALAPPDATA%`. The local encrypted profile was
  moved to the verified Roaming default without reading credential contents.
  Shortcuts now specify Claude's resolved native executable explicitly, avoiding
  reliance on Explorer inheriting a shell's PATH. Synthetic shortcut checks passed.
* The optimized Windows executable was rebuilt from the locked dependencies.
* Formatting and strict Clippy across all targets passed.
* The full isolated suite passed: 951 library tests, 7 CLI unit tests and
  181 integration tests, using disposable configuration and synthetic credentials.
* Synthetic shortcut checks passed, including literal paths with spaces,
  apostrophes and shell metacharacters, repeat installation without duplicates,
  direct executable targeting, and a loadable multi-size Windows ICO.
* A locally assembled release ZIP was extracted and its installer verified.
  Only the locally compiled executable's `--version` command was executed;
  no provider requests were needed for packaging checks.
* The user-authorized local profile directory was renamed from the temporary
  supervised name to the normal Windows default, preserving encrypted files
  without reading credentials. The desktop shortcut now uses that default.

## Model-catalog and Claude-launcher follow-up

The follow-up adds authenticated model metadata, a generated Claude `/model`
picker, native effort selection and context configuration. Validation on
2026-10-24:

* Full isolated suite: 947 library tests, 6 CLI unit tests and 180 pre-existing
  integration tests passed. The new catalog integration test subsequently passed,
  for **1,134 tests** across the final feature. Authentication coverage for both
  new metadata endpoints was rerun successfully.
* Formatting, strict Clippy across all targets and the optimized Windows build
  passed with the unchanged locked dependencies.
* A local mock with disposable Claude configuration verified that installed
  Claude Code 2.1.310 accepts the generated picker settings and emits
  `gpt-6-sol` / `high` and `gpt-6.1-sol` / `max` in actual outgoing requests.
* A live metadata query through the newly authorized supervised profile returned
  seven compatible models. The generated multi-model profile selected a 272,000
  shared context with compaction at 258,400. Raw catalog instructions and auth
  data were not printed or persisted.
* One short, user-authorized live inference with tools and session persistence
  disabled returned `OK`; the proxy logged `model=gpt-6-sol`,
  `reasoningEffort=high`, HTTP 200, approximately 1.95 seconds. Prompt/body/error
  captures were disabled. This supersedes the original no-live-inference scope
  below; only the new supervised profile was used by the credential manager.

The native CLI print-mode check validates settings parsing and routing, not a
screenshot of the interactive picker. Interactive `/model` and `/effort` remain
subject to Claude's version and managed settings. Per-model seamless context
resizing is not claimed: multi-model mode uses the smallest advertised default,
and `--only-model` uses the selected model's full advertised default.

## Original hardening validation

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

Final review also added an empty-DPAPI-payload assertion to the existing failure
test and guarded zero-length output buffers. The three Windows credential tests
and strict Clippy were rerun successfully after this small follow-up.

To rerun everything in one invocation:

```powershell
python scripts/check-isolated.py -- --test-threads=1
```

Linux/Windows GitHub CI is configured in `.github/workflows/ci.yml`. Local results
above establish Windows coverage; hosted CI results are reported separately.
macOS, ARM release targets, documentation-site dependencies, live subscription
compatibility and the release-publishing workflow were not exercised locally.
