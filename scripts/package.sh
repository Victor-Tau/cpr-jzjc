#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$task_root"
target="${CPR_TARGET:-x86_64-unknown-linux-gnu}"
case "$target" in
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu) ;;
  *) echo "Unsupported CPR_TARGET: $target" >&2; exit 2 ;;
esac
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
cargo fmt --manifest-path backend/Cargo.toml --check
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
python3 -m unittest discover -s engine/tests -v
python3 -m unittest discover -s scripts/tests -v
node scripts/api_smoke.mjs
python3 scripts/build_runtime.py --target "$target" --cache-dir ".cache/runtime/$target" "$@"
python3 scripts/collect_licenses.py
if command -v cargo-zigbuild >/dev/null 2>&1; then
  cargo zigbuild --manifest-path backend/Cargo.toml --locked --release --target "${target}.2.36"
elif [[ "${CPR_NATIVE_BUILD:-0}" == "1" ]]; then
  [[ "$(rustc -vV | sed -n 's/^host: //p')" == "$target" ]] || { echo "Native build requires a matching host target" >&2; exit 2; }
  python3 - <<'PY'
import platform
libc, version = platform.libc_ver()
assert libc == 'glibc' and tuple(map(int, version.split('.'))) <= (2, 36), 'Use glibc <=2.36 or cargo-zigbuild for portable packages'
PY
  cargo build --manifest-path backend/Cargo.toml --locked --release --target "$target"
else
  echo "cargo-zigbuild is required, or set CPR_NATIVE_BUILD=1 on a native target host" >&2
  exit 2
fi
cpr-plugin package --manifest plugin.json --binary "backend/target/$target/release/modeltrace-health" --target "$target" --resource-map web=frontend/dist --output-dir dist
task_version="$(python3 -c 'import json; print(json.load(open("plugin.json"))["version"])')"
python3 scripts/verify_artifact.py "dist/personal.modeltrace-health-${task_version}-${target}.tar.gz" "$target"
