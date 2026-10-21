#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$task_root"
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
cargo fmt --manifest-path backend/Cargo.toml --check
cargo test --manifest-path backend/Cargo.toml --locked
python3 -m unittest discover -s engine/tests -v
python3 scripts/build_runtime.py "$@"
python3 scripts/collect_licenses.py
cargo zigbuild --manifest-path backend/Cargo.toml --locked --release --target x86_64-unknown-linux-gnu.2.36
cpr-plugin package --manifest plugin.json --binary backend/target/x86_64-unknown-linux-gnu/release/modeltrace-health --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
python3 scripts/verify_artifact.py dist/personal.modeltrace-health-1.5.0-x86_64-unknown-linux-gnu.tar.gz
