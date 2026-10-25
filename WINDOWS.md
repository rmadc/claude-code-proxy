# Windows setup for this fork

The source is checked out on `security/windows-hardening`. `origin` is your fork;
`upstream` is Raine Virta's repository. The working baseline remains `v0.1.46`.
Read [SECURITY.md](SECURITY.md) before enabling diagnostic captures or remote access.

## Build and test

Use PowerShell from this directory. Rust 1.99.0, MSVC C++ Build Tools and a
Windows SDK are required; `rust-toolchain.toml` selects the compiler.

```powershell
cargo build --release --locked
python scripts/check-isolated.py -- --test-threads=1
```

The test wrapper requires Python 3. It isolates provider configuration and home
directories and uses synthetic credentials. DPAPI tests need an ordinary Windows
user context; the restricted agent sandbox cannot perform successful encryption.

## Authorize the provider yourself

No provider sign-in was performed during this work and no existing provider
credentials were inspected or imported. When ready, follow the local
[Codex provider instructions](docs/src/content/docs/providers/codex.md) to
authorize the proxy's own store with the account you intend to use. Provider
eligibility and model availability remain controlled by the provider.

On first access, this fork migrates any existing proxy plaintext credentials to
user-scoped DPAPI. Existing Windows XDG/home configuration remains accessible.
Do not delete the source yourself to force migration. If migration reports an
error, keep all copies until a protected record has been verified successfully.

## Run with client authentication

### Automatic picker, effort and context discovery

The `claude` launcher handles both the proxy and Claude Code. After authorizing
your profile, run the executable from its location:

```powershell
.\claude-code-proxy.exe claude
```

The default Windows profile is `%LOCALAPPDATA%\claude-code-proxy`; no environment
override is required. Set `CCP_CONFIG_DIR` only when deliberately using a separate
profile.

On first launch without `--model`, the launcher fetches compatible models and
prompts you to choose one, saving it as `claudeDefaultModel` in the proxy's
`config.json` (under `CCP_CONFIG_DIR` when set). Later launches use that saved
model; `--model` overrides it for one launch. If the saved model is no longer
available, an interactive launch prompts again. An interactive terminal is
required for first-run selection.

It queries the authenticated Codex catalog, supplies a `modelPicker` lineup to
Claude, and starts an authenticated loopback proxy on a free port. Inside Claude,
use `/model` to select a listed model and `/effort` to change reasoning effort.
An explicit initial effort can be supplied with `--effort high`; otherwise the
launcher uses the backend-advertised default. Proxy model/effort overrides must
be unset so they cannot silently override the picker.

The client token is passed through the child environment, never printed or
written to the temporary settings file. The launcher does not rewrite global
Claude settings. Claude's own model/effort persistence still applies. Exiting
Claude stops its proxy. Normal Claude permission controls remain enabled.

**Context switching limitation:** Claude's context override applies to the whole
process. The multi-model mode therefore uses the smallest default context among
the listed models, with compaction at no more than 95% of that window. Picker
descriptions show each model's own context and reasoning capabilities. For the
selected model's full default window, restrict the process to one model:

```powershell
.\claude-code-proxy.exe claude --model gpt-6.1-sol --only-model
```

For a catalog advertising 500,000 tokens, this declares a 500,000 context and
475,000 compaction threshold. The advertised maximum override, such as 872,000,
is informational and is not automatically enabled. Seamless per-model context
resizing within a running Claude process is not supported by this launcher.

Metadata inspection without launching Claude:

```powershell
.\claude-code-proxy.exe catalog
.\claude-code-proxy.exe claude --model gpt-6-sol --print-config
```

Additional Claude arguments follow `--`; `--claude-bin` selects its executable
when it is not on PATH. No native Codex credentials are imported. The picker is
the intersection of the authenticated catalog and models the proxy translates;
catalog advertisement does not guarantee inference access on every plan.
Unsupported effort modes such as `ultra` are omitted. Claude exposes five named
capability-declaration slots: the initial model is always declared and remaining
slots are populated from the catalog. Native effort behavior for other rows
depends on Claude's gateway handling; explicit model/effort combinations were
verified against Claude Code 2.1.310. Use a fresh launch for the selected model's
full capability declaration when needed.

### Manual environment setup

After provider authorization, the following example generates a new local
client token without printing it, starts the locally built proxy, and configures
Claude Code in the same PowerShell session. Choose a registered model available
to your account; `gpt-6-sol` below is an example from the source catalog.

```powershell
$clientTokenBytes = New-Object byte[] 32
$random = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$random.GetBytes($clientTokenBytes)
$random.Dispose()
$env:CCP_CLIENT_TOKEN = [Convert]::ToBase64String($clientTokenBytes)
$env:ANTHROPIC_AUTH_TOKEN = $env:CCP_CLIENT_TOKEN
$env:ANTHROPIC_BASE_URL = 'http://127.0.0.1:18765'
$env:ANTHROPIC_MODEL = 'gpt-6-sol'
$env:CCP_AUTO_REVIEW_MODEL = 'inherit'

$proxyPath = Join-Path $PWD 'target\release\claude-code-proxy.exe'
$proxyProcess = Start-Process -FilePath $proxyPath -ArgumentList @('serve', '--no-monitor') -WindowStyle Hidden -PassThru
claude
```

Do not save the token value in source control or paste it into chat. A different
terminal needs the same client token; environment changes in one terminal do not
propagate to an already-open terminal. This example leaves loopback binding and
diagnostic defaults intact and does not enable `CCP_ALLOW_NETWORK`.

To stop the process started above when finished:

```powershell
$proxyProcess | Stop-Process
Remove-Item Env:CCP_CLIENT_TOKEN, Env:ANTHROPIC_AUTH_TOKEN
```

Diagnostic opt-ins are `CCP_TRAFFIC_LOG=1`, `CCP_ERROR_CAPTURE=1`, and
`CCP_LOG_CONTENT=1`. They can record sensitive content. Traffic captures have no
automatic aggregate retention; remove them after a focused investigation.

## ClaudeGPT desktop shortcut

Windows release ZIPs include `ClaudeGPT.ico` and `Create-ClaudeGPT-Shortcut.ps1`.
Extract the ZIP into a permanent folder, then run:

```powershell
& .\Create-ClaudeGPT-Shortcut.ps1
```

This creates or updates one desktop shortcut called **ClaudeGPT**. It launches
Claude Code and its authenticated loopback proxy together, using the saved model
choice. Claude Code must be installed and available on PATH. Keep the extracted
folder in place; rerun the installer after moving it. The icon is a multi-size
Windows ICO, supplied alongside the executable without changing upstream branding.

The default profile is `%LOCALAPPDATA%\claude-code-proxy`. To use another existing
profile, pass `-ConfigDirectory 'C:\path\to\profile'`. The shortcut stores only
paths, never credentials. `-WorkingDirectory` selects Claude's starting project;
it defaults to your user folder. The source checkout installer lives at
`scripts/create-claudegpt-shortcut.ps1`; supply `-ExecutablePath` and `-IconPath`
when running it from source.
