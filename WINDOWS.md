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
