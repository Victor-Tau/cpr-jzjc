"""Install the official packager from the same pinned commit as the plugin SDK."""
from pathlib import Path
import re
import subprocess
import tomllib

root = Path(__file__).resolve().parents[1]
sdk = tomllib.loads((root / "backend/Cargo.toml").read_text())["dependencies"]["gateway-plugin-sdk"]
assert re.fullmatch(r"[0-9a-f]{40}", sdk["rev"]), "Pin the SDK to a full commit SHA"
subprocess.run([
    "cargo", "install", "--locked", "--force", "--git", sdk["git"], "--rev", sdk["rev"],
    "--root", str(root / ".cache/tools"), "codex-proxy-plugin-cli",
], check=True)
