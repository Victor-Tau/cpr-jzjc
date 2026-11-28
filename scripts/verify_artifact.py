"""Verify archive digests, isolated-page resources, executable architecture, and packaged scoring."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
from html.parser import HTMLParser
from protocol_smoke import Host

ELF_MACHINE={"x86_64-unknown-linux-gnu":62,"aarch64-unknown-linux-gnu":183}
MACOS_ARM64 = "aarch64-apple-darwin"
TARGETS = {
    **{target: {"os": "linux", "architecture": target.split("-")[0]} for target in ELF_MACHINE},
    MACOS_ARM64: {"os": "macos", "architecture": "aarch64"},
}


def validate_executable(header, target):
    if target == MACOS_ARM64:
        if len(header) < 8 or header[:4] != b"\xcf\xfa\xed\xfe" or int.from_bytes(header[4:8], "little") != 0x0100000C:
            raise ValueError("Expected a macOS ARM64 Mach-O executable")
        return
    if len(header) < 20 or header[:6] != b"\x7fELF\x02\x01":
        raise ValueError("Expected a 64-bit little-endian ELF file")
    if int.from_bytes(header[18:20], "little") != ELF_MACHINE[target]:
        raise ValueError(f"ELF architecture does not match {target}")

package = Path(sys.argv[1]).resolve()
source_manifest = json.loads((Path(__file__).resolve().parents[1]/'plugin.json').read_text())
target = sys.argv[2] if len(sys.argv) > 2 else os.environ.get("CPR_TARGET", "x86_64-unknown-linux-gnu")
if target not in TARGETS:
    raise SystemExit(f"Unsupported target: {target}")
digest = hashlib.sha256(package.read_bytes()).hexdigest()
assert package.with_name(package.name+'.sha256').read_text().split()[0] == digest
assert package.stat().st_size <= 32*1024*1024
assert package.stat().st_size <= 24*1024*1024, 'package unexpectedly large'
with tempfile.TemporaryDirectory(prefix='modeltrace-package-check-') as directory:
    root=Path(directory)
    with tarfile.open(package) as archive:
        for e in archive:
            assert not Path(e.name).is_absolute() and '..' not in Path(e.name).parts
            assert e.isfile() or e.isdir()
        archive.extractall(root,filter='data')
    manifest=json.loads((root/'plugin.json').read_text())
    for field in ('publisher', 'name', 'version', 'engines', 'main', 'runtime', 'state'):
        assert manifest[field] == source_manifest[field], f'Manifest field differs from source: {field}'
    assert manifest['package']['protocolVersion'] == 7
    assert manifest['package']['target'] == TARGETS[target]
    assert set(manifest['package']['files']) == {manifest['main'], *manifest['resources']}
    for path, expected in manifest['package']['files'].items():
        assert hashlib.sha256((root/path).read_bytes()).hexdigest() == expected, path
    with (root / manifest['main']).open('rb') as binary:
        validate_executable(binary.read(20), target)
    class Page(HTMLParser):
        def handle_starttag(self,tag,attrs):
            a=dict(attrs)
            if tag not in ('script','link'):return
            ref=a.get('src' if tag=='script' else 'href','')
            assert ref and not ref.startswith(('/','http:','https:')) and '..' not in Path(ref).parts
            assert 'web/'+ref.removeprefix('./') in manifest['resources']
            assert a.get('type') != 'module'
    Page().feed((root/'web/index.html').read_text())
    assert not (root/'runtime').exists(), 'Python runtime must no longer be packaged'
    # Score through the packaged binary: the detector is embedded and decoded in-process.
    host=Host(binary=root/'bin/modeltrace-health')
    try:
        b=host.request('api/bootstrap');host_models=b['banks']['active']['models'];config=b['config'];config.update(keyId='key-test',lowConfidencePercent=0,degradedPercent=100,retryDelaySeconds=0)
        host.request('api/config',{'config':config,'revision':b['configRevision']})
        job=host.request('api/submit',{'accountIds':['acct-test'],'requestId':'artifact-runtime-check'})['submitted'][0]
        host.tick(2)
        report=host.request('api/report?id='+job)
        assert report['status']=='completed',report['message']
        assert len(report['rounds'])==1 and len(host.executions)==1,report
    finally:host.close()
    print(json.dumps({'artifact':package.name,'sha256':digest,'bytes':package.stat().st_size,'inProcessScoring':True,'models':len(host_models)},indent=2))
