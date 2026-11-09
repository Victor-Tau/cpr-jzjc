"""Build a private Linux CPython + NumPy runtime from pinned public inputs.

Use --base-tar with a crane-exported python image to reuse a local cache.
"""
from __future__ import annotations
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import posixpath
import shutil
import subprocess
import sys
import tarfile
import zipfile
from elftools.elf.elffile import ELFFile
from runtime_target import TARGETS, validate_elf

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--base-tar", type=Path)
parser.add_argument("--target", choices=tuple(TARGETS), default=os.environ.get("CPR_TARGET", "x86_64-unknown-linux-gnu"))
parser.add_argument("--cache-dir", type=Path)
args = parser.parse_args()
spec = TARGETS[args.target]
cache = (args.cache_dir or ROOT / ".cache/runtime" / args.target).resolve(); cache.mkdir(parents=True, exist_ok=True)
base = args.base_tar or cache / "python-base.tar"
if not base.exists():
    crane = shutil.which("crane")
    if not crane: parser.error("Install crane or supply --base-tar from the documented pinned image")
    subprocess.run([crane,"pull","--platform",spec["docker_platform"],spec["image"],str(base)],check=True)

files = {}
with tarfile.open(base) as image:
    manifest = json.load(image.extractfile("manifest.json"))[0]
    raw_config = image.extractfile(manifest["Config"]).read()
    if "sha256:" + hashlib.sha256(raw_config).hexdigest() != spec["config"]: raise ValueError("Unexpected runtime base configuration")
    config = json.loads(raw_config)
    assert config["architecture"] == spec["architecture"] and config["os"] == "linux"
    for layer_path, digest in zip(manifest["Layers"],config["rootfs"]["diff_ids"],strict=True):
        raw = image.extractfile(layer_path).read()
        if raw[:2] == b"\x1f\x8b": raw = gzip.decompress(raw)
        assert "sha256:"+hashlib.sha256(raw).hexdigest() == digest
        with tarfile.open(fileobj=io.BytesIO(raw)) as layer:
            for e in layer:
                p = e.name.removeprefix("./").rstrip("/")
                if not p.startswith(("usr/local/","usr/lib/","lib/","lib64/","usr/share/doc/")) or e.isdir(): continue
                if e.isfile(): files[p] = (layer.extractfile(e).read(),e.mode)
                elif e.issym() or e.islnk():
                    link = e.linkname if e.linkname.startswith("/") or e.islnk() else posixpath.join(posixpath.dirname(p),e.linkname)
                    files[p] = (posixpath.normpath(link).lstrip("/"),e.mode)

def resolve(p, seen=None):
    seen = set() if seen is None else seen
    if p not in files and p.startswith("lib/"): p="usr/"+p
    if p not in files or p in seen: raise ValueError("Unresolved runtime dependency: "+p)
    seen.add(p); content,mode=files[p]
    return resolve(content,seen) if isinstance(content,str) else (content,mode)

output = {}
for p in files:
    if p != "usr/local/bin/python3.11" and not p.startswith("usr/local/lib/"): continue
    if any(part in {"test","tests","__pycache__","site-packages","ensurepip","idlelib","tkinter","turtledemo","pkgconfig"} for part in p.split("/")): continue
    if p.endswith((".a",".pyc")) or p == "usr/local/lib/libpython3.11.so": continue
    if "/lib-dynload/" in p and p.rsplit("/",1)[-1].startswith(("_tkinter","_test","_xx","_ctypes_test")): continue
    output[p] = resolve(p)
loader = f"usr/lib/{spec['libdir']}/{spec['loader']}"
output[loader] = resolve(next(p for p in files if p.endswith("/" + spec["loader"])))

wheels = cache / "wheels"; wheels.mkdir(exist_ok=True)
download = [sys.executable,"-m","pip","download","--dest",str(wheels),"--only-binary=:all:","--no-deps"]
for platform in spec["wheel_platforms"]: download += ["--platform", platform]
download += ["--python-version","3.11","--implementation","cp","--abi","cp311","numpy==2.4.3"]
subprocess.run(download,check=True)
wheel = next(wheels.glob("numpy-2.4.3-cp311-*.whl"))
wheel_hash = hashlib.sha256(wheel.read_bytes()).hexdigest()
pin_file = ROOT / spec["pin_file"]
if pin_file.exists() and pin_file.read_text().strip() != wheel_hash: raise ValueError("NumPy wheel SHA-256 mismatch")
if not pin_file.exists(): raise ValueError("Missing pinned NumPy wheel hash")
with zipfile.ZipFile(wheel) as z:
    for e in z.infolist():
        p = Path(e.filename)
        if p.is_absolute() or ".." in p.parts: raise ValueError("Unsafe wheel path")
        if e.is_dir() or any(s in {"tests","__pycache__"} for s in p.parts) or p.suffix == ".pyc": continue
        output["usr/local/lib/python3.11/site-packages/"+e.filename] = (z.read(e),0o755 if ".so" in p.name else 0o644)

for prefix in ("engine","vendor/modeltrace"):
    for p in (ROOT/prefix).rglob("*"):
        if p.is_file() and not any(s in {"tests","__pycache__"} for s in p.parts): output[str(p.relative_to(ROOT))]=(p.read_bytes(),0o644)

pending = list(output.values()); seen = set(); used = set()
while pending:
    raw,mode=pending.pop()
    if not raw.startswith(b"\x7fELF"): continue
    validate_elf(raw, args.target)
    elf=ELFFile(io.BytesIO(raw)); dynamic=elf.get_section_by_name(".dynamic")
    if dynamic is None: continue
    for t in dynamic.iter_tags():
        if t.entry.d_tag != "DT_NEEDED" or t.needed in seen: continue
        seen.add(t.needed)
        if any(p.endswith('/'+t.needed) for p in output): continue
        p=next((p for p in [f"usr/lib/{spec['libdir']}/"+t.needed,f"lib/{spec['libdir']}/"+t.needed,"usr/local/lib/"+t.needed] if p in files),None)
        if p is None: raise ValueError("Missing ELF library: "+t.needed)
        resolved=resolve(p); output[f"usr/lib/{spec['libdir']}/"+t.needed]=resolved; pending.append(resolved);used.add(t.needed)

notices = ["Runtime base: "+spec["image"], "NumPy 2.4.3 wheel SHA256: "+wheel_hash]
for p in sorted(files):
    if p.startswith("usr/share/doc/") and p.endswith("/copyright"):
        try: notices.append("\n--- "+p+" ---\n"+resolve(p)[0].decode(errors="replace"))
        except ValueError: pass
with zipfile.ZipFile(wheel) as z:
    for name in z.namelist():
        if "dist-info/" in name and "LICENSE" in name.upper() and not name.endswith("/"): notices.append("\n--- "+name+" ---\n"+z.read(name).decode(errors="replace"))
(ROOT/"legal/THIRD_PARTY.txt").write_text('\n'.join(notices))
destination=ROOT/"runtime/runtime.tar.gz"
with destination.open("wb") as raw, gzip.GzipFile(fileobj=raw,mode="wb",mtime=0,compresslevel=9) as gz, tarfile.open(fileobj=gz,mode="w|") as archive:
    for p,(data,mode) in sorted(output.items()):
        info=tarfile.TarInfo(p);info.size=len(data);info.mode=mode;archive.addfile(info,io.BytesIO(data))
print(json.dumps({"files":len(output),"expandedBytes":sum(len(r) for r,m in output.values()),"compressedBytes":destination.stat().st_size,"sha256":hashlib.sha256(destination.read_bytes()).hexdigest()},indent=2))
