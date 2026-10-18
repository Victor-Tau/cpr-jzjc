#!/usr/bin/env bash
# Linux-only: private mount namespace; never mounts in the host namespace.
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
task_binary="$(realpath "${1:?Pass an extracted release binary}")"
mkdir -p "$task_root/.cache"
task_mount="$(mktemp -d "$task_root/.cache/runtime-mount-XXXXXX")"
trap 'rmdir "$task_mount"' EXIT
task_namespace=(--mount --propagation private --fork)
if (( EUID != 0 )); then task_namespace=(--user --map-root-user "${task_namespace[@]}"); fi
unshare "${task_namespace[@]}" \
  bash -c '
    set -euo pipefail
    cd "$1"
    mount -t tmpfs -o size=64m,noexec,nodev,nosuid tmpfs "$3"
    "$1/.venv/bin/python" scripts/protocol_smoke.py --binary "$2" --runtime-tmpdir "$3" --deny-timestamps -v
    # This time allow execution: extraction must encounter ENOSPC, clean up,
    # and fall back to another directory instead of failing the detection.
    mount -o remount,exec "$3"
    "$1/.venv/bin/python" scripts/protocol_smoke.py --binary "$2" --runtime-tmpdir "$3" Integration.test_binding_idempotency_and_completed_scoring -v
    test -z "$(ls -A "$3")"
  ' -- "$task_root" "$task_binary" "$task_mount"
