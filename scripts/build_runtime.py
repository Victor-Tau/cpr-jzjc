"""Build a private Linux x86_64 CPython + NumPy runtime from pinned public inputs.

Use --base-tar with a crane-exported python image to reuse a local cache.
"""
from __future__ import annotations
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import posixpath
import shutil
import subprocess
import sys
import tarfile
import zipfile
from elftools.elf.elffile import ELFFile

ROOT = Path(__file__).resolve().parents[1]
IMAGE = "index.docker.io/library/python@sha256:8a00ecb83ed9d0864dfd102a1bdb2067fda907ceb30bb86d09da9ee558b19590"
CONFIG = "sha256:141fe9b385f3bb6d618c07ace1836c0b3c72bf525566ac3eb13cd1f405069877"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--base-tar", type=Path)
parser.add_argument("--cache-dir", type=Path, default=ROOT / ".cache/runtime")
args = parser.parse_args()
cache = args.cache_dir.resolve(); cache.mkdir(parents=True, exist_ok=True)
base = args.base_tar or cache / "python-base.tar"
if not base.exists():
    crane = shutil.which("crane")
    if not crane: parser.error("Install crane or supply --base-tar from the documented pinned image")
    subprocess.run([crane,"pull","--platform","linux/amd64",IMAGE,str(base)],check=True)

files = {}
with tarfile.open(base) as image:
    manifest = json.load(image.extractfile("manifest.json"))[0]
    raw_config = image.extractfile(manifest["Config"]).read()
    if "sha256:" + hashlib.sha256(raw_config).hexdigest() != CONFIG: raise ValueError("Unexpected runtime base configuration")
    config = json.loads(raw_config)
    assert config["architecture"] == "amd64" and config["os"] == "linux"
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
loader = "usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2"
output[loader] = resolve(next(p for p in files if p.endswith("/ld-linux-x86-64.so.2")))

wheels = cache / "wheels"; wheels.mkdir(exist_ok=True)
subprocess.run([sys.executable,"-m","pip","download","--dest",str(wheels),"--only-binary=:all:","--no-deps","--platform","manylinux_2_27_x86_64","--platform","manylinux_2_28_x86_64","--python-version","3.11","--implementation","cp","--abi","cp311","numpy==2.4.3"],check=True)
wheel = next(wheels.glob("numpy-2.4.3-cp311-*.whl"))
wheel_hash = hashlib.sha256(wheel.read_bytes()).hexdigest()
pin_file = ROOT / "scripts/numpy-wheel.sha256"
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
    elf=ELFFile(io.BytesIO(raw)); dynamic=elf.get_section_by_name(".dynamic")
    if dynamic is None: continue
    for t in dynamic.iter_tags():
        if t.entry.d_tag != "DT_NEEDED" or t.needed in seen: continue
        seen.add(t.needed)
        if any(p.endswith('/'+t.needed) for p in output): continue
        p=next((p for p in ["usr/lib/x86_64-linux-gnu/"+t.needed,"lib/x86_64-linux-gnu/"+t.needed,"usr/local/lib/"+t.needed] if p in files),None)
        if p is None: raise ValueError("Missing ELF library: "+t.needed)
        resolved=resolve(p); output["usr/lib/x86_64-linux-gnu/"+t.needed]=resolved; pending.append(resolved);used.add(t.needed)

notices = ["Runtime base: "+IMAGE, "NumPy 2.4.3 wheel SHA256: "+wheel_hash]
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
