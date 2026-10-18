"""Exercise the real plugin process against a deterministic mock RS host. No live accounts."""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import select
import struct
import subprocess
import sys
import time
import threading
import unittest

ROOT=Path(__file__).resolve().parents[1]
BINARY=ROOT/"backend/target/debug/modeltrace-health"
DENY_TIMESTAMPS=False
RUNTIME_TMPDIR=None
def raw(v): return json.dumps(v,ensure_ascii=False,separators=(",",":"),allow_nan=False).encode()

class Host:
    def __init__(self, binary=BINARY, state=None):
        self.send_lock=threading.Lock();self.timers=[]
        self.state={} if state is None else state
        self.enabled=True;self.streams={};self.executions=[];self.closes=0;self.fail_model=False;self.fail_download=False
        self.identity="original";self.bank=(ROOT/"vendor/modeltrace/data/unified_bank.json").read_bytes();self.sha="b"*40
        self.disable_calls=0;self.fail_disable=False;self.sequence=1
        self.fail_put=None;self.model_read_delay=0
        self.additional_accounts=[]
        env=os.environ.copy();env['MODELTRACE_DEV_PYTHON']=str(ROOT/'.venv/bin/python')
        if RUNTIME_TMPDIR is not None:env['TMPDIR']=str(RUNTIME_TMPDIR)
        command=[str(binary)]
        if DENY_TIMESTAMPS:
            command=['strace','-f','-o',os.devnull,'-e','trace=utimensat','-e','inject=utimensat:error=EPERM',*command]
        self.process=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
        contributes={name:{'id':'personal.modeltrace-health.'+name,'version':version,'stages':[name]} for name,version in [('management',2),('maintenance',1)]}
        self.send({'type':'hello','handshake':{'protocol_version':4,'artifact_sha256':'a'*64,'plugin_id':'personal.modeltrace-health','instance_id':'test','generation':1,'incarnation':'test-run','contributes':contributes}}, {})
        ready,_,_=self.receive();assert ready['type']=='ready',ready

    def send(self,meta,data=None,payload=b''):
        meta=copy.deepcopy(meta);encoded=b'' if data is None else raw(data)
        if encoded:meta['data_bytes']=len(encoded)
        head=raw(meta)
        with self.send_lock:
            self.process.stdin.write(struct.pack('>IQ',len(head),len(encoded)+len(payload))+head+encoded+payload);self.process.stdin.flush()

    def receive(self):
        def read(n):
            out=b''
            while len(out)<n:
                if not select.select([self.process.stdout],[],[],35)[0]:raise AssertionError('Plugin RPC timed out')
                chunk=os.read(self.process.stdout.fileno(),n-len(out))
                if not chunk:raise AssertionError('Plugin stopped: '+self.process.stderr.read().decode()[:1000])
                out+=chunk
            return out
        a,b=struct.unpack('>IQ',read(12));meta=json.loads(read(a));body=read(b);d=meta.get('data_bytes',0);r=meta.get('request_context_bytes',0)
        return meta,json.loads(body[r:r+d]) if d else None,body[r+d:]

    def call(self,method,params=None,payload=b'',stage='management'):
        i=self.sequence;self.sequence+=2
        context={'call_id':i,'instance_id':'test','generation':1,'incarnation':'test-run','stage':stage,'timeout_ms':30000,'resource_stream':False,'resource_scope_id':f'scope-{i}','request_id':None,'attempt_id':None,'account_id':None,'credential_revision':None}
        self.send({'type':'call','id':i,'method':method,'context':context},params or {},payload)
        while True:
            meta,data,body=self.receive()
            if meta['type']=='callback':
                delay=self.model_read_delay if meta['method']=='host.model.stream_read' else 0
                self.model_read_delay=0 if delay else self.model_read_delay
                try:result,out=self.callback(meta['method'],data,body)
                except SimulatedFault as e:self.send({'type':'error','id':meta['id'],'error':{'code':'rejected','message':str(e),'send_state':'not_sent'}})
                else:
                    if delay:
                        timer=threading.Timer(delay,self.send,args=({'type':'result','id':meta['id']},result,out));timer.daemon=True;self.timers.append(timer);timer.start()
                    else:self.send({'type':'result','id':meta['id']},result,out)
                finally:
                    if delay:self.model_read_delay=delay
            elif meta.get('id')==i:
                if meta['type']=='error':raise AssertionError(meta['error'])
                return data,body
            else:raise AssertionError(meta)

    def request(self,path,body=None,ok=True):
        path,_,query=path.partition('?');data,payload=self.call('management.handle',{'method':'GET' if body is None else 'POST','path':path,'query':query,'content_type':None if body is None else 'application/json','headers':[]},b'' if body is None else raw(body))
        result=json.loads(payload)
        if ok:assert data['status']==200,result
        else:assert data['status']!=200,result
        return result

    def tick(self,n=1):
        for _ in range(n):self.call('plugin.reconcile',stage='maintenance')

    def stream(self,body):
        s='stream-'+str(len(self.streams));self.streams[s]=[body];return s

    def callback(self,method,p,body):
        if method=='host.state.get':return {'record':copy.deepcopy(self.state.get((p['namespace'],p['key'])))},b''
        if method=='host.state.put':
            key=(p['namespace'],p['key']);previous=self.state.get(key)
            if self.fail_put and self.fail_put(key,p['value']):raise SimulatedFault('state unavailable')
            assert p['expected_version']==(None if previous is None else previous['version']), 'CAS mismatch'
            version=1 if previous is None else previous['version']+1
            assert len(raw(p['value']))<=262144
            self.state[key]={'value':copy.deepcopy(p['value']),'version':version,'schema_version':1};return {'version':version},b''
        if method=='host.state.delete':
            key=(p['namespace'],p['key']);assert self.state[key]['version']==p['expected_version'];del self.state[key];return {'deleted':True},b''
        if method=='host.keys.list':return {'keys':[{'id':'key-test','name':'体检计量','enabled':True}],'next_cursor':None},b''
        if method=='host.models.list':return {'models':['gpt-6.1-sol','gpt-5.6-luna']},b''
        if method=='host.auth.get_runtime':
            account_id=json.loads(body)['account_id']
            assert account_id in ['acct-test',*[a['id'] for a in self.additional_accounts]]
            return {},raw({'account_id':account_id,'provider_id':'openai','credential_revision':1,'name':'account','email':'user@example.com','upstream_user_id':'user','upstream_account_id':self.identity,'plan_type':'plus','authentication_kind':'oauth','enabled':self.enabled,'credential_state':'ready','has_refresh_token':True,'access_token_expires_at_ms':None,'next_refresh_at_ms':None})
        if method=='host.http.dispatch':
            uri=p['uri']
            if '/accounts/batch-update' in uri:
                if self.fail_disable:raise SimulatedFault('permission denied')
                self.enabled=False;self.disable_calls+=1;data={}
            elif uri.startswith('/api/admin/accounts?'):
                data={'items':[{'id':'acct-test','name':'test-account','email':'user@example.com','provider':'openai','enabled':self.enabled,'status':'normal','planType':'plus','groups':[{'id':'group-test','name':'测试组'}]}],'total':1}
                data['items'].extend(self.additional_accounts);data['total']=len(data['items'])
            else:raise AssertionError(uri)
            return {'status':200,'version':'HTTP/1.1','headers':[],'body':{'kind':'handle','handle':self.stream(raw({'code':200,'data':data}))},'response':None,'session':False},b''
        if method=='host.http.body_read':
            chunks=self.streams[p['handle']]
            return ({'eof':False,'trailers':None},chunks.pop(0)) if chunks else ({'eof':True,'trailers':None},b'')
        if method=='host.http.body_close':return {},b''
        if method=='host.model.execute_stream':
            self.executions.append((p,json.loads(body)))
            assert p['account_id'] in ['acct-test',*[a['id'] for a in self.additional_accounts]] and p['client_key_id']=='key-test'
            assert p['model']==json.loads(body)['model']
            if self.fail_model:raise SimulatedFault('401 authentication failure secret-do-not-display')
            return {'request_id':f'request-{len(self.executions)}','stream':'model-stream'},b''
        if method=='host.model.stream_read':
            if self.model_read_delay:time.sleep(self.model_read_delay)
            numbers=','.join(str((i*i+i*97)%355+1) for i in range(330))
            value={'type':'response.completed','response':{'output':[{'content':[{'type':'output_text','text':numbers}]}]}}
            event=raw({'wire':{'protocol':'openai','payload':{'kind':'json','event':None,'data':value}}})
            event=b'GPE2'+struct.pack('>III',len(event),0,0)+event
            return {'events':1,'end':True},b'HME1'+struct.pack('>II',1,len(event))+event
        if method=='host.model.stream_close':self.closes+=1;return {},b''
        if method=='host.http.do_stream':
            url=p['url']
            if self.fail_download:raise SimulatedFault('network unavailable')
            if url.endswith('/xqy2006/ModelTrace'):data=raw({'default_branch':'main'})
            elif '/commits/' in url:data=raw({'sha':self.sha})
            elif '/'+self.sha+'/data/unified_bank.json' in url:data=self.bank
            else:raise AssertionError(url)
            return {'status':200,'headers':[],'stream':self.stream(data)},b''
        if method=='host.http.stream_read':
            chunks=self.streams[p['stream']]
            if chunks:
                chunk=chunks.pop(0);limit=p['maximum_bytes'];part=chunk[:limit]
                if len(chunk)>limit:chunks.insert(0,chunk[limit:])
                return {'eof':False},part
            return {'eof':True},b''
        if method=='host.http.stream_close':return {},b''
        raise AssertionError('Unhandled host callback: '+method+' '+str(p)[:200])

    def close(self):
        for timer in self.timers:timer.cancel()
        if self.process.poll() is None:
            self.send({'type':'shutdown'})
            try:self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
        self.process.stdin.close();self.process.stdout.close();self.process.stderr.close()

class SimulatedFault(Exception):pass

class Integration(unittest.TestCase):
    def setUp(self):self.host=Host();self.addCleanup(self.host.close)
    def config(self,**overrides):
        b=self.host.request('api/bootstrap');c=b['config'];c.update({'keyId':'key-test','retryDelaySeconds':0});c.update(overrides)
        self.host.request('api/config',{'config':c,'revision':b['configRevision']});return c
    def submit(self):return self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'submit-one'})['submitted'][0]
    def test_registration_and_queries(self):
        _,payload=self.host.call('management.register',stage='registration');registration=json.loads(payload);self.assertIn('api/report',[r['path'] for r in registration['routes']])
        self.assertEqual(self.host.request('api/accounts')['accounts'][0]['groups'][0]['id'],'group-test')
        self.assertIn('gpt-6.1-sol',self.host.request('api/models?keyId=key-test')['models'])
    def test_binding_idempotency_and_completed_scoring(self):
        self.config(degradedModels=['gpt-5.6-luna'],lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();self.assertEqual(identity,self.submit());self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(len(report['rounds']),1);self.assertEqual(len(self.host.executions),1);self.assertFalse(report['config']['autoDisable']);self.assertEqual(self.host.disable_calls,0)
        self.assertEqual(report['bankHash'],hashlib.sha256((ROOT/'vendor/modeltrace/data/unified_bank.json').read_bytes()).hexdigest())
    def test_batch_retests_keep_each_account_route(self):
        self.host.additional_accounts=[{'id':'acct-proxy-two','name':'second-account','provider':'openai','enabled':True,'status':'normal','planType':'plus','groups':[],'outboundProxyId':'proxy-two'}]
        self.config(lowConfidencePercent=100,degradedPercent=100,lowConfidenceRetries=1)
        ids=self.host.request('api/submit',{'accountIds':['acct-test','acct-proxy-two'],'requestId':'batch-routes'})['submitted']
        self.host.tick(20)
        for job_id,account_id in zip(ids,['acct-test','acct-proxy-two']):
            report=self.host.request('api/report?id='+job_id)
            self.assertEqual(report['status'],'completed');self.assertEqual(len(report['rounds']),2)
            executions=[params for params,_ in self.host.executions if params['account_id']==account_id]
            self.assertEqual(len(executions),2)
            for params in executions:
                self.assertEqual(params['provider'],'openai')
                self.assertEqual(set(params),{'account_id','client_key_id','model','protocol','operation','provider'})
    def test_default_timeout_allows_response_after_twenty_seconds(self):
        c=self.config(lowConfidencePercent=0,degradedPercent=100);self.assertEqual(c['timeoutSeconds'],120)
        self.host.model_read_delay=21;identity=self.submit();self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(len(report['attempts']),1);self.assertIsNone(report['attempts'][0]['error'])
        self.assertEqual(report['status'],'completed');self.assertEqual(len(report['rounds']),1);self.assertEqual(self.host.closes,0)
    def test_unlimited_and_large_timeout_complete_without_overflow(self):
        for seconds in [0,3600,2**40,2**64-1]:
            with self.subTest(seconds=seconds):
                self.config(timeoutSeconds=seconds,lowConfidencePercent=0,degradedPercent=100)
                identity=self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':f'timeout-{seconds}'})['submitted'][0]
                self.host.tick(6);report=self.host.request('api/report?id='+identity)
                self.assertEqual(report['config']['timeoutSeconds'],seconds)
                self.assertEqual(report['status'],'completed');self.assertEqual(len(report['rounds']),1)
        self.host.close();self.host=Host(state=self.host.state);self.addCleanup(self.host.close)
        self.assertEqual(self.host.request('api/bootstrap')['config']['timeoutSeconds'],2**64-1)
    def test_custom_short_timeout_closes_stream(self):
        self.config(timeoutSeconds=1);self.host.model_read_delay=2
        identity=self.submit();self.host.tick();report=self.host.request('api/report?id='+identity)
        self.assertIn('超时',report['attempts'][0]['error']);self.assertEqual(self.host.closes,1)
        self.assertEqual(self.host.disable_calls,0)
    def test_low_confidence_budget(self):
        self.config(lowConfidencePercent=100,degradedPercent=100,lowConfidenceRetries=2);identity=self.submit();self.host.tick(16)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(len(report['rounds']),3);self.assertEqual(report['verdict'],'inconclusive');self.assertEqual(len(self.host.executions),3);self.assertEqual(self.host.disable_calls,0)
    def test_auth_error_does_not_disable_or_leak(self):
        self.config(autoDisable=True);self.host.fail_model=True;identity=self.submit();self.host.tick(3)
        report=self.host.request('api/report?id='+identity);self.assertEqual(report['status'],'failed');self.assertEqual(len(self.host.executions),1);self.assertNotIn('secret',json.dumps(report));self.assertEqual(self.host.disable_calls,0)
    def test_cancel_and_restart(self):
        self.config(lowConfidencePercent=100,degradedPercent=100);identity=self.submit();self.host.tick();self.host.request('api/cancel',{'id':identity});self.host.tick(2);self.assertEqual(len(self.host.executions),1)
        self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'restart-two'});state=self.host.state;self.host.close();self.host=Host(state=state);self.addCleanup(self.host.close)
        self.assertEqual(self.host.request('api/jobs')['jobs'][0]['status'],'interrupted')
    def test_update_atomicity_rollback_and_running_snapshot(self):
        self.config();identity=self.submit();self.host.tick()
        old=self.host.request('api/bootstrap')['banks']['active'];new=json.loads(self.host.bank);new['built_at']='2026-10-18T00:00:00Z';self.host.bank=raw(new)
        checked=self.host.request('api/banks/check',{});self.assertIsNotNone(checked['update']['available'])
        applied=self.host.request('api/banks/apply',{});self.assertNotEqual(old['hash'],applied['active']['hash'])
        self.assertEqual(self.host.request('api/report?id='+identity)['bankHash'],old['hash'])
        self.assertEqual(self.host.request('api/banks/rollback',{})['active']['hash'],old['hash'])
        self.host.fail_download=True;failed=self.host.request('api/banks/check',{});self.assertIsNotNone(failed['update']['error']);self.assertEqual(failed['active']['hash'],old['hash'])
    def test_rejects_incompatible_bank_and_rule_conflict(self):
        self.config();old=self.host.request('api/bootstrap')['banks']['active']['hash'];b=json.loads(self.host.bank);b['schema']='unknown';self.host.bank=raw(b)
        failed=self.host.request('api/banks/check',{});self.assertIsNotNone(failed['update']['error']);self.assertEqual(failed['active']['hash'],old)
        b=json.loads((ROOT/'vendor/modeltrace/data/unified_bank.json').read_bytes());i=next(i for i,m in enumerate(b['models']) if m['id']=='gpt-5.6-luna')
        b['models'].pop(i);b['robust']['model_order'].pop(i)
        for kind in ['hellinger','ordered_blocks']:b['robust'][kind]['centroids'].pop(i)
        for environment in b['robust']['ordered_blocks']['environment_centroids']:environment.pop(i)
        self.host.bank=raw(b);self.host.request('api/banks/check',{})
        conflict=self.host.request('api/banks/apply',{},ok=False);self.assertIn('gpt-5.6-luna',conflict['error']['message']);self.assertEqual(self.host.request('api/bootstrap')['banks']['active']['hash'],old)
    def test_identity_change_stops_samples(self):
        self.config(autoDisable=True,lowConfidencePercent=100,degradedPercent=100);identity=self.submit();self.host.tick();self.host.identity='changed';self.host.tick()
        self.assertEqual(self.host.request('api/report?id='+identity)['status'],'failed');self.assertEqual(len(self.host.executions),1);self.assertEqual(self.host.disable_calls,0)
    def test_three_round_confirmation_and_disable_retry(self):
        models=[m['id'] for m in json.loads(self.host.bank)['models']]
        self.config(autoDisable=True,degradedModels=models,lowConfidencePercent=0,degradedPercent=0)
        identity=self.submit();self.host.tick(3)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['verdict'],'degraded');self.assertEqual(len(report['rounds']),3);self.assertEqual(self.host.disable_calls,0)
        self.host.fail_disable=True;self.host.tick();report=self.host.request('api/report?id='+identity);self.assertEqual(report['disableState'],'failed')
        self.host.fail_disable=False;self.host.request('api/retry-disable',{'id':identity});self.assertEqual(self.host.disable_calls,1);self.assertEqual(len(self.host.executions),3)
    def test_schedule_dynamic_scope_and_deduplication(self):
        c=self.config();c['schedule'].update({'enabled':True,'filter':{'plan':'plus','status':'normal','group':'group-test','keyword':'USER@'}})
        b=self.host.request('api/bootstrap');self.host.request('api/config',{'config':c,'revision':b['configRevision']})
        self.host.close();state=self.host.state;state[('control','main')]['value']['nextDue']=1
        self.host=Host(state=state);self.addCleanup(self.host.close);self.host.tick(2)
        jobs=self.host.request('api/jobs')['jobs'];self.assertEqual(len(jobs),1);self.assertEqual(jobs[0]['source'],'schedule')
    def test_partial_update_keeps_active_and_cleans_on_restart(self):
        self.config();old=self.host.request('api/bootstrap')['banks']['active']['hash'];b=json.loads(self.host.bank);b['built_at']='new-build';self.host.bank=raw(b)
        self.host.request('api/banks/check',{});self.host.fail_put=lambda key,value:key[0]=='banks'
        self.host.request('api/banks/apply',{},ok=False);self.assertEqual(self.host.request('api/bootstrap')['banks']['active']['hash'],old)
    def test_update_survives_restart_and_is_used_by_new_tasks(self):
        self.config();b=json.loads(self.host.bank);b['built_at']='new-build';self.host.bank=raw(b);self.host.request('api/banks/check',{})
        updated=self.host.request('api/banks/apply',{})['active']['hash'];state=self.host.state;self.host.close()
        self.host=Host(state=state);self.addCleanup(self.host.close);self.assertEqual(self.host.request('api/bootstrap')['banks']['active']['hash'],updated)
        identity=self.submit();self.host.tick();self.assertEqual(self.host.request('api/report?id='+identity)['bankHash'],updated)
    def test_uncertain_probe_storage_failure_never_replays(self):
        self.config();identity=self.submit()
        self.host.fail_put=lambda key,value:key[0]=='jobs' and value.get('status')=='running' and not value.get('pendingProbe') and bool(value.get('attempts'))
        with self.assertRaises(AssertionError):self.host.tick()
        self.host.fail_put=None;self.host.tick()
        report=self.host.request('api/report?id='+identity);self.assertEqual(report['status'],'interrupted');self.assertEqual(len(self.host.executions),1)

    def test_progress_and_cancel_do_not_wait_for_probe_lock(self):
        self.config();identity=self.submit();original=self.host.callback;observed=[]
        def callback(method,p,body):
            if method=='host.model.stream_read':
                started=time.monotonic()
                progress=self.host.request('api/progress')
                report=self.host.request('api/report?id='+identity)
                observed.append(time.monotonic()-started)
                self.assertEqual(progress['jobs'][0]['progress']['stage'],'waiting_response')
                self.assertEqual(report['status'],'running')
                self.host.request('api/cancel',{'id':identity})
            return original(method,p,body)
        self.host.callback=callback;self.host.tick()
        self.assertTrue(observed);self.assertLess(observed[0],1)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'cancelled');self.assertFalse(report['pendingProbe'])
        self.assertEqual(len(self.host.executions),1);self.assertEqual(self.host.closes,1)

    def test_progress_revision_and_stream_updates(self):
        self.config(lowConfidencePercent=0,degradedPercent=100);identity=self.submit()
        first=self.host.request('api/progress');same=self.host.request('api/progress?since='+str(first['revision']))
        self.assertFalse(same['changed']);self.assertIsNone(same['jobs']);self.assertNotIn('banks',first)
        original=self.host.callback;chunks=[]
        def callback(method,p,body):
            if method=='host.model.stream_read' and not chunks:
                chunks.append(True)
                value={'type':'response.output_text.delta','delta':'12,34,'}
                event=raw({'wire':{'protocol':'openai','payload':{'kind':'json','event':None,'data':value}}})
                event=b'GPE2'+struct.pack('>III',len(event),0,0)+event
                return {'events':1,'end':False},b'HME1'+struct.pack('>II',1,len(event))+event
            if method=='host.model.stream_read':
                progress=self.host.request('api/progress')
                self.assertEqual(progress['jobs'][0]['progress']['stage'],'receiving')
                self.assertGreater(progress['jobs'][0]['progress']['detail']['receivedBytes'],0)
            return original(method,p,body)
        self.host.callback=callback;self.host.tick()
        after=self.host.request('api/progress?since='+str(first['revision']))
        self.assertTrue(after['changed']);self.assertEqual(after['jobs'][0]['status'],'completed')

    def test_waiting_retest_does_not_block_other_accounts(self):
        self.host.additional_accounts=[{'id':'acct-two','name':'second','provider':'openai','enabled':True,'status':'normal','groups':[]}]
        self.config(lowConfidencePercent=100,degradedPercent=100,retryDelaySeconds=60)
        ids=self.host.request('api/submit',{'accountIds':['acct-test','acct-two'],'requestId':'fair-queue'})['submitted']
        self.host.tick(2)
        self.assertEqual([p['account_id'] for p,_ in self.host.executions],['acct-test','acct-two'])
        for identity in ids:self.assertEqual(len(self.host.request('api/report?id='+identity)['rounds']),1)

    def test_parent_budget_times_out_cleanly_without_interrupting_job(self):
        self.config();identity=self.submit();self.host.model_read_delay=31
        self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'running');self.assertFalse(report['pendingProbe'])
        self.assertEqual(len(report['attempts']),1);self.assertIn('超时',report['attempts'][0]['error'])
        self.assertEqual(self.host.closes,1)
        self.host.model_read_delay=0;self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertNotEqual(report['status'],'interrupted');self.assertEqual(len(self.host.executions),2)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=BINARY);parser.add_argument('--deny-timestamps',action='store_true',help='Use strace to deny utimensat and exercise restricted runtime extraction');parser.add_argument('--runtime-tmpdir',type=Path,help='Set TMPDIR for the plugin process to exercise runtime directory fallback');args,remaining=parser.parse_known_args();BINARY=args.binary;DENY_TIMESTAMPS=args.deny_timestamps;RUNTIME_TMPDIR=args.runtime_tmpdir
    # Host defaults are evaluated at definition time.
    Host.__init__.__defaults__=(BINARY,None)
    unittest.main(argv=[sys.argv[0],*remaining])
