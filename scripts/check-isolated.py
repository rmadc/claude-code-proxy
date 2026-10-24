"""Run Cargo tests with synthetic configuration and a disposable user home.

No provider credentials are copied. Cargo/Rustup caches retain their normal paths.
Usage: python scripts/check-isolated.py [cargo test arguments]
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo.exe")
    env = os.environ.copy()
    # Preserve only build-tool cache locations before isolating the child home.
    env.setdefault("CARGO_HOME", str(Path.home() / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(Path.home() / ".rustup"))
    for name in list(env):
        if name.startswith(("CCP_", "ANTHROPIC_", "OPENAI_", "CURSOR_", "KIMI_", "GROK_")):
            del env[name]
    with tempfile.TemporaryDirectory(prefix="ccp-security-tests-") as root:
        for name, subdir in {
            "HOME": "home", "USERPROFILE": "home", "APPDATA": "roaming",
            "LOCALAPPDATA": "local", "XDG_CONFIG_HOME": "config",
            "XDG_STATE_HOME": "state", "CCP_CONFIG_DIR": "ccp-config",
            "CODEX_HOME": "codex",
        }.items():
            location = Path(root) / subdir
            location.mkdir(exist_ok=True)
            env[name] = str(location)
        return subprocess.call([cargo, "test", "--locked", *sys.argv[1:]], env=env)


if __name__ == "__main__":
    sys.exit(main())
