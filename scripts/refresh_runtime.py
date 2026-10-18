"""Reuse a verified release's pinned Python/NumPy runtime, refreshing vendored code.

The original artifact and every packaged file must match their published digests.
Use build_runtime.py when changing runtime dependencies.
"""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("package", type=Path)
args = parser.parse_args()
raw = args.package.read_bytes()
assert hashlib.sha256(raw).hexdigest() == args.package.with_name(args.package.name + ".sha256").read_text().split()[0], "Source artifact digest mismatch"
with tarfile.open(fileobj=io.BytesIO(raw)) as package:
    manifest = json.load(package.extractfile("plugin.json"))
    for name, digest in manifest["package"]["files"].items():
        assert hashlib.sha256(package.extractfile(name).read()).hexdigest() == digest, name
    runtime = package.extractfile("runtime/runtime.tar.gz").read()

files = {}
with tarfile.open(fileobj=io.BytesIO(runtime)) as archive:
    for entry in archive:
        assert entry.isfile() and not Path(entry.name).is_absolute() and ".." not in Path(entry.name).parts
        files[entry.name] = (archive.extractfile(entry).read(), entry.mode)
for prefix in ("engine", "vendor/modeltrace"):
    for file in (ROOT / prefix).rglob("*"):
        if file.is_file() and not any(part in {"tests", "__pycache__"} for part in file.parts):
            files[str(file.relative_to(ROOT))] = (file.read_bytes(), 0o644)

destination = ROOT / "runtime/runtime.tar.gz"
destination.parent.mkdir(parents=True, exist_ok=True)
with destination.open("wb") as output, gzip.GzipFile(fileobj=output, mode="wb", mtime=0, compresslevel=9) as compressed, tarfile.open(fileobj=compressed, mode="w|") as archive:
    for name, (data, mode) in sorted(files.items()):
        entry = tarfile.TarInfo(name)
        entry.size, entry.mode = len(data), mode
        archive.addfile(entry, io.BytesIO(data))
print(json.dumps({"sourceVersion": manifest["version"], "files": len(files), "expandedBytes": sum(len(data) for data, _ in files.values()), "sha256": hashlib.sha256(destination.read_bytes()).hexdigest()}))
