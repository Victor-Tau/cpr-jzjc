"""Append SDK, Rust and bundled frontend dependency notices to runtime notices."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[1]
destination=ROOT/'legal/THIRD_PARTY.txt'
marker='\n\n===== PLUGIN DEPENDENCIES =====\n'
base=destination.read_text().split(marker)[0]
chunks=[base,marker];seen=set()

def add(name,root):
    entries=[]
    for file in sorted(root.iterdir()):
        if file.is_file() and file.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE','COPYRIGHT')):
            data=file.read_text(errors='replace');digest=hashlib.sha256(data.encode()).hexdigest()
            if digest in seen:continue
            seen.add(digest);entries.append(f'--- {name}/{file.name} ---\n{data}\n')
    chunks.extend(entries)

metadata=json.loads(subprocess.check_output(['cargo','metadata','--manifest-path',str(ROOT/'backend/Cargo.toml'),'--locked','--format-version','1'],text=True))
for package in metadata['packages']:
    add(f"{package['name']} {package['version']} ({package.get('license')})",Path(package['manifest_path']).parent)
    # Workspace SDK's Apache license is inherited from the repository root.
    if package['name']=='gateway-plugin-sdk':
        directory=Path(package['manifest_path']).parent
        while directory!=directory.parent:
            if (directory/'LICENSE').exists():add('Codex Proxy RS',directory);break
            directory=directory.parent

for node_root in (ROOT/'frontend/node_modules/.pnpm').iterdir():
    modules=node_root/'node_modules'
    if not modules.is_dir():continue
    for package in modules.iterdir():
        if package.name.startswith('@'):
            candidates=list(package.iterdir()) if package.is_dir() else []
        else:candidates=[package]
        for candidate in candidates:
            if not candidate.is_dir() or not (candidate/'package.json').exists():continue
            p=json.loads((candidate/'package.json').read_text());add(f"{p.get('name')} {p.get('version')}",candidate)
destination.write_text('\n'.join(chunks))
print('Third-party notices:',destination.stat().st_size,'bytes')
