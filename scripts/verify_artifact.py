"""Verify archive digests, isolated-page resources, and packaged offline scoring."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
from html.parser import HTMLParser
from protocol_smoke import Host

package = Path(sys.argv[1]).resolve()
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
    assert manifest['publisher']== 'personal' and manifest['name']== 'modeltrace-health'
    assert manifest['package']['protocolVersion'] == 6
    assert manifest['engines']['codex-proxy-rs'] == '>=3.32.0, <3.33.0'
    assert set(manifest['package']['files']) == {manifest['main'], *manifest['resources']}
    for path, expected in manifest['package']['files'].items():
        assert hashlib.sha256((root/path).read_bytes()).hexdigest() == expected, path
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
    with tarfile.open(root/'runtime/runtime.tar.gz') as archive:archive.extractall(runtime,filter='data')
    command=[str(runtime/'usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2'),'--library-path',str(runtime/'usr/local/lib')+':'+str(runtime/'usr/lib/x86_64-linux-gnu'),str(runtime/'usr/local/bin/python3.11'),'-B',str(runtime/'engine/worker.py')]
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
