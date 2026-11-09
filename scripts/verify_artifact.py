"""Verify archive digests, isolated-page resources, and packaged offline scoring."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
from html.parser import HTMLParser
from protocol_smoke import Host
from runtime_target import TARGETS, validate_elf

package = Path(sys.argv[1]).resolve()
source_manifest = json.loads((Path(__file__).resolve().parents[1]/'plugin.json').read_text())
target = sys.argv[2] if len(sys.argv) > 2 else os.environ.get("CPR_TARGET", "x86_64-unknown-linux-gnu")
if target not in TARGETS:
    raise SystemExit(f"Unsupported target: {target}")
spec = TARGETS[target]
libdir, loader = spec["libdir"], spec["loader"]
digest = hashlib.sha256(package.read_bytes()).hexdigest()
assert package.with_name(package.name+'.sha256').read_text().split()[0] == digest
assert package.stat().st_size <= 32*1024*1024
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
    assert manifest['package']['target'] == {'os': 'linux', 'architecture': target.split('-')[0]}
    assert set(manifest['package']['files']) == {manifest['main'], *manifest['resources']}
    for path, expected in manifest['package']['files'].items():
        assert hashlib.sha256((root/path).read_bytes()).hexdigest() == expected, path
    validate_elf((root/manifest['main']).read_bytes()[:20], target)
    class Page(HTMLParser):
        def handle_starttag(self,tag,attrs):
            a=dict(attrs)
            if tag not in ('script','link'):return
            ref=a.get('src' if tag=='script' else 'href','')
            assert ref and not ref.startswith(('/','http:','https:')) and '..' not in Path(ref).parts
            assert 'web/'+ref.removeprefix('./') in manifest['resources']
            assert a.get('type') != 'module'
    Page().feed((root/'web/index.html').read_text())
    runtime=root/'runtime-expanded';runtime.mkdir()
    with tarfile.open(root/'runtime/runtime.tar.gz') as archive:
        for entry in archive:
            assert entry.isfile() and not Path(entry.name).is_absolute() and '..' not in Path(entry.name).parts
            header = archive.extractfile(entry).read(20)
            if header.startswith(b'\x7fELF'): validate_elf(header, target)
        archive.extractall(runtime,filter='data')
    command=[str(runtime/f'usr/lib/{libdir}/{loader}'),'--library-path',str(runtime/'usr/local/lib')+':'+str(runtime/f'usr/lib/{libdir}'),str(runtime/'usr/local/bin/python3.11'),'-B',str(runtime/'engine/worker.py')]
    result=subprocess.run(command,input=json.dumps({'op':'validate','path':str(runtime/'vendor/modeltrace/data/unified_bank.json')})+'\n',text=True,capture_output=True,env={'PYTHONHOME':str(runtime/'usr/local'),'OPENBLAS_NUM_THREADS':'1'},timeout=15)
    assert result.returncode==0,result.stderr
    data=json.loads(result.stdout);assert data['ok'],data
    # Exercise Rust's runtime extraction too; Python extraction alone cannot
    # catch failures caused by tar metadata restoration in the plugin process.
    host=Host(binary=root/'bin/modeltrace-health')
    try:
        b=host.request('api/bootstrap');config=b['config'];config.update(keyId='key-test',lowConfidencePercent=0,degradedPercent=100,retryDelaySeconds=0)
        host.request('api/config',{'config':config,'revision':b['configRevision']})
        job=host.request('api/submit',{'accountIds':['acct-test'],'requestId':'artifact-runtime-check'})['submitted'][0]
        host.tick(2)
        report=host.request('api/report?id='+job)
        assert report['status']=='completed',report['message']
        assert len(report['rounds'])==1 and len(host.executions)==1,report
    finally:host.close()
    print(json.dumps({'artifact':package.name,'sha256':digest,'bytes':package.stat().st_size,'runtimeValidated':True,'pluginExtractionValidated':True,'models':len(data['data']['models'])},indent=2))
