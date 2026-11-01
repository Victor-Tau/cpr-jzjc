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
from urllib.parse import parse_qsl, urlencode

ROOT=Path(__file__).resolve().parents[1]
BINARY=ROOT/"backend/target/debug/modeltrace-health"
DENY_TIMESTAMPS=False
RUNTIME_TMPDIR=None
def raw(v): return json.dumps(v,ensure_ascii=False,separators=(",",":"),allow_nan=False).encode()

def model_events(*values, end=False, wire_kind='json', parsed=True):
    events=[]
    for value in values:
        message=raw(value) if wire_kind=='web_socket' else b''
        payload={'kind':wire_kind,'event':None,'data':value if parsed else None}
        if wire_kind=='web_socket':payload['message']=[]
        event=raw({'wire':{'protocol':'openai','payload':payload}})
        events.append(b'GPE2'+struct.pack('>III',len(event),len(message),0)+event+message)
    return {'events':len(events),'end':end},b'HME1'+struct.pack('>I',len(events))+b''.join(struct.pack('>I',len(e)) for e in events)+b''.join(events)

class Host:
    def __init__(self, binary=BINARY, state=None, protocol_version=7):
        self.protocol_version=protocol_version
        self.send_lock=threading.Lock();self.timers=[]
        self.scheduling=[];self.pending_callbacks={};self.schedule_epochs={};self.suspended=set()
        self.state={} if state is None else state
        self.enabled=True;self.streams={};self.executions=[];self.closes=0;self.fail_model=False;self.fail_download=False
        self.identity="original";self.bank=(ROOT/"vendor/modeltrace/data/unified_bank.json").read_bytes();self.sha="b"*40
        self.disable_calls=0;self.fail_disable=False;self.sequence=1
        self.fail_put=None;self.model_read_delay=0;self.model_response_delay=0;self.callback_response_delay=0
        self.additional_accounts=[]
        self.groups=[{'id':'group-test','name':'测试组'},{'id':'group-degraded','name':'降智隔离组'}]
        self.memberships=['group-test'];self.move_calls=0;self.fail_move=False;self.ignore_move=False;self.updates=[]
        env=os.environ.copy();env['MODELTRACE_DEV_PYTHON']=str(ROOT/'.venv/bin/python')
        if RUNTIME_TMPDIR is not None:env['TMPDIR']=str(RUNTIME_TMPDIR)
        command=[str(binary)]
        if DENY_TIMESTAMPS:
            command=['strace','-f','-o',os.devnull,'-e','trace=utimensat','-e','inject=utimensat:error=EPERM',*command]
        self.process=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
        contributes={name:{'id':'personal.modeltrace-health.'+name,'version':version,'stages':[name]} for name,version in [('management',2),('maintenance',1)]}
        self.send({'type':'hello','handshake':{'protocol_version':protocol_version,'artifact_sha256':'a'*64,'plugin_id':'personal.modeltrace-health','instance_id':'test','generation':1,'incarnation':'test-run','contributes':contributes}}, {})
        ready,_,_=self.receive();assert ready['type']=='ready',ready
        self.call_budgets=ready.get('call_budgets',{})

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
                if not select.select([self.process.stdout],[],[],max(35,getattr(self,'receive_timeout',35)))[0]:raise AssertionError('Plugin RPC timed out')
                chunk=os.read(self.process.stdout.fileno(),n-len(out))
                if not chunk:raise AssertionError('Plugin stopped: '+self.process.stderr.read().decode()[:1000])
                out+=chunk
            return out
        while True:
            a,b=struct.unpack('>IQ',read(12));meta=json.loads(read(a));body=read(b);d=meta.get('data_bytes',0);r=meta.get('request_context_bytes',0)
            kind=meta['type']
            if kind in ('suspend','resume','callback_consumed'):
                assert self.protocol_version >= 7, meta
                self.scheduling.append(meta)
                if kind=='suspend':
                    identity=meta['id'];epoch=meta['epoch']
                    assert identity not in self.suspended, meta
                    assert epoch==self.schedule_epochs.get(identity,0)+1, meta
                    self.schedule_epochs[identity]=epoch;self.suspended.add(identity)
                elif kind=='resume':
                    identity=meta['id']
                    assert identity in self.suspended and meta['epoch']==self.schedule_epochs[identity], meta
                    self.suspended.remove(identity)
                    self.send({'type':'resumed','id':identity,'epoch':meta['epoch']})
                else:
                    assert self.pending_callbacks.pop(meta['id'])==meta['parent_id'], meta
                continue
            if kind=='callback' and self.protocol_version >= 7:
                assert meta['id'] not in self.pending_callbacks, meta
                self.pending_callbacks[meta['id']]=meta['parent_id']
            return meta,json.loads(body[r:r+d]) if d else None,body[r+d:]

    def call(self,method,params=None,payload=b'',stage='management',timeout_ms=30000):
        i=self.sequence;self.sequence+=2
        self.receive_timeout=timeout_ms/1000+5
        context={'call_id':i,'instance_id':'test','generation':1,'incarnation':'test-run','stage':stage,'timeout_ms':timeout_ms,'resource_stream':False,'resource_scope_id':f'scope-{i}','request_id':None,'attempt_id':None,'account_id':None,'credential_revision':None}
        self.send({'type':'call','id':i,'method':method,'context':context},params or {},payload)
        while True:
            meta,data,body=self.receive()
            if meta['type']=='callback':
                is_stream=meta['method']=='host.model.stream_read'
                persistent_delay=self.model_response_delay if is_stream else 0
                delay=persistent_delay or (self.model_read_delay if is_stream else 0)
                if is_stream and not persistent_delay:self.model_read_delay=0 if delay else self.model_read_delay
                try:result,out=self.callback(meta['method'],data,body)
                except SimulatedFault as e:self.send({'type':'error','id':meta['id'],'error':{'code':'rejected','message':str(e),'send_state':'not_sent'}})
                else:
                    delay=delay or self.callback_response_delay;self.callback_response_delay=0
                    if delay:
                        timer=threading.Timer(delay,self.send,args=({'type':'result','id':meta['id']},result,out));timer.daemon=True;self.timers.append(timer);timer.start()
                    else:self.send({'type':'result','id':meta['id']},result,out)
                finally:
                    if delay and is_stream and not persistent_delay:self.model_read_delay=delay
            elif meta.get('id')==i:
                if meta['type']=='error':raise AssertionError(meta['error'])
                if method=='management.handle':
                    assert 200 <= data['status'] <= 599, data
                    assert data['content_type']=='application/json', data
                    assert len(body)<=1024*1024, f'CPR rejects management body: {len(body)} bytes > 1 MiB'
                return data,body
            else:raise AssertionError(meta)

    def request(self,path,body=None,ok=True):
        path,_,query=path.partition('?');params=dict(parse_qsl(query));merged=None;previous=0
        while True:
            data,payload=self.call('management.handle',{'method':'GET' if body is None else 'POST','path':path,'query':urlencode(params),'content_type':None if body is None else 'application/json','headers':[]},b'' if body is None else raw(body))
            result=json.loads(payload)
            if ok:assert data['status']==200,result
            else:assert data['status']!=200,result
            page=result.get('pagination')
            if page is None:return result
            if merged is None:merged=result
            else:
                assert merged['pagination']['snapshot']==page['snapshot']
                merged[page['field']].extend(result[page['field']])
            offset=page['nextOffset']
            if offset is None:
                del merged['pagination'];return merged
            assert offset>previous
            previous=offset;params.update(pageOffset=offset,pageSnapshot=page['snapshot'])

    def tick(self,n=1,budget_ms=30000):
        # 旧回归有意保留 30 秒短预算；新预算用例使用 ready 中协商的值。
        for _ in range(n):self.call('plugin.reconcile',stage='maintenance',timeout_ms=budget_ms)

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
            if uri.startswith('/api/admin/usage/'):
                # Protocol-v4 binaries are used only by the upgrade test. The
                # current plugin must never query platform usage records.
                assert self.protocol_version == 4, 'removed usage lookup was called'
                raise SimulatedFault('usage records unavailable')
            elif '/accounts/batch-update' in uri:
                update=json.loads(body);assert update['accountIds']==['acct-test'];self.updates.append(update)
                if 'enabled' in update:
                    assert set(update)=={'accountIds','enabled'} and update['enabled'] is False
                    if self.fail_disable:raise SimulatedFault('permission denied')
                    self.enabled=False;self.disable_calls+=1
                elif 'groupIds' in update:
                    assert set(update)=={'accountIds','groupIds'}
                    assert all(any(g['id']==i for g in self.groups) for i in update['groupIds'])
                    if self.fail_move:raise SimulatedFault('permission denied')
                    self.move_calls+=1
                    if not self.ignore_move:self.memberships=update['groupIds']
                else:raise AssertionError(update)
                data={}
            elif uri.startswith('/api/admin/account-groups/options?'):
                from urllib.parse import urlparse, parse_qs
                page=int(parse_qs(urlparse(uri).query)['page'][0]);items=self.groups[(page-1)*200:page*200]
                data={'items':items,'page':{'page':page,'totalPages':(len(self.groups)+199)//200}}
            elif uri.startswith('/api/admin/accounts/detail?'):
                from urllib.parse import urlparse, parse_qs
                account_id=parse_qs(urlparse(uri).query)['accountId'][0];assert account_id in ['acct-test',*[a['id'] for a in self.additional_accounts]]
                data={'account':{'id':account_id,'groups':[{'id':i,'name':i} for i in self.memberships]}}
            elif uri.startswith('/api/admin/accounts?'):
                data={'items':[{'id':'acct-test','name':'test-account','email':'user@example.com','provider':'openai','enabled':self.enabled,'status':'normal','planType':'plus','groups':[{'id':i,'name':i} for i in self.memberships]}],'total':1}
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
            if self.protocol_version >= 5:
                assert all(isinstance(v,list) and all(isinstance(b,int) and 0<=b<=255 for b in v) for _,v in p['headers'])
            if self.fail_download:raise SimulatedFault('network unavailable')
            if url.endswith('/xqy2006/ModelTrace'):data=raw({'default_branch':'main'})
            elif '/commits/' in url:data=raw({'sha':self.sha})
            elif '/'+self.sha+'/data/unified_bank.json' in url:data=self.bank
            else:raise AssertionError(url)
            response={'status':200,'headers':[],'stream':self.stream(data)}
            if self.protocol_version >= 5:response.update(http_version='HTTP/1.1',trailers=[])
            return response,b''
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
    def setUp(self):
        self.host=Host();self.addCleanup(self.host.close)
        self.host.request('api/banks/builtin',{}) # Most cases isolate probing from cold startup; tested separately below.
    def config(self,**overrides):
        b=self.host.request('api/bootstrap');c=b['config'];c.update({'keyId':'key-test','retryDelaySeconds':0});c.update(overrides)
        self.host.request('api/config',{'config':c,'revision':b['configRevision']});return c
    def submit(self):return self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'submit-one'})['submitted'][0]
    def confirmed_config(self, **overrides):
        self.host.request('api/banks/builtin',{})
        cfg=dict(degradedModels=[m['id'] for m in json.loads(self.host.bank)['models']],lowConfidencePercent=0,degradedPercent=0)
        cfg.update(overrides);return self.config(**cfg)

    def test_defaults_and_saved_v13_config_survive_upgrade(self):
        fresh=self.host.request('api/bootstrap')['config']
        self.assertEqual([fresh[k] for k in ['lowConfidencePercent','degradedPercent','confirmationRounds','lowConfidenceRetries','retryDelaySeconds','retentionDays']],[20,90,1,1,10,3])
        saved=self.config(lowConfidencePercent=60,degradedPercent=80,confirmationRounds=3,lowConfidenceRetries=2,retryDelaySeconds=30,timeoutSeconds=999)
        self.host.close();state=copy.deepcopy(self.host.state)
        for key in ['autoMoveGroup','targetGroupId']:state[('control','main')]['value']['config'].pop(key)
        self.host=Host(state=state);self.addCleanup(self.host.close)
        loaded=self.host.request('api/bootstrap')['config']
        for key in saved:
            if key not in ['autoMoveGroup','targetGroupId']:self.assertEqual(loaded[key],saved[key])
        self.assertFalse(loaded['autoMoveGroup']);self.assertEqual(loaded['targetGroupId'],'')

    def test_config_import_preview_validates_without_applying(self):
        before=self.host.request('api/bootstrap');old=copy.deepcopy(before['config'])
        old.update(lowConfidencePercent=60,confirmationRounds=3)
        for key in ['autoMoveGroup','targetGroupId']:old.pop(key)
        preview=self.host.request('api/config/preview',{'config':old})['config']
        self.assertEqual(preview['confirmationRounds'],3);self.assertFalse(preview['autoMoveGroup'])
        after=self.host.request('api/bootstrap');self.assertEqual(before['config'],after['config']);self.assertEqual(before['configRevision'],after['configRevision'])
        for bad in [{'degradedPercent':101},{'unknownField':True},{'confirmationRounds':0},{'autoMoveGroup':True}]:
            cfg=copy.deepcopy(before['config']);cfg.update(bad)
            self.host.request('api/config/preview',{'config':cfg},ok=False)
        self.assertEqual(self.host.request('api/bootstrap')['config'],before['config'])

    def test_empty_groups_and_group_pagination(self):
        self.host.groups=[{'id':f'group-{i}','name':f'组{i}'} for i in range(201)]
        groups=self.host.request('api/bootstrap')['groups'];self.assertEqual(len(groups),201)
        self.config(autoMoveGroup=True,targetGroupId='group-200')
        cfg=self.host.request('api/bootstrap');cfg['config']['targetGroupId']='deleted'
        self.host.request('api/config',{'config':cfg['config'],'revision':cfg['configRevision']},ok=False)

    def test_disable_and_move_independent_combinations(self):
        for disable,move in [(False,False),(True,False),(False,True),(True,True)]:
            with self.subTest(disable=disable,move=move):
                if self.host.executions:self.host.close();self.host=Host();self.addCleanup(self.host.close)
                self.confirmed_config(autoDisable=disable,autoMoveGroup=move,targetGroupId='group-degraded')
                identity=self.submit();self.host.tick()
                before=self.host.request('api/report?id='+identity)
                self.assertEqual(before['verdict'],'degraded');self.assertEqual(len(before['rounds']),1)
                report=self.host.request('api/report?id='+identity)
                self.assertEqual(report['disableState'],'disabled' if disable else 'none')
                self.assertEqual(report['groupState'],'moved' if move else 'none')
                self.assertEqual(self.host.enabled,not disable);self.assertEqual(self.host.move_calls,int(move));self.assertEqual(len(self.host.executions),1)
                if move:self.assertEqual(self.host.memberships,['group-degraded'])

    def test_already_in_target_group_skips_redundant_move(self):
        self.host.memberships=['group-degraded']
        self.confirmed_config(autoMoveGroup=True,targetGroupId='group-degraded')
        identity=self.submit();self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['verdict'],'degraded')
        self.assertEqual(report['groupState'],'moved')
        self.assertNotIn('移组失败',report['message'])
        self.assertEqual(self.host.move_calls,0)
        self.assertEqual(self.host.memberships,['group-degraded'])

    def test_action_failure_does_not_block_other_action_or_reprobe(self):
        for failure in ['disable','move']:
            with self.subTest(failure=failure):
                if self.host.executions:self.host.close();self.host=Host();self.addCleanup(self.host.close)
                self.confirmed_config(autoDisable=True,autoMoveGroup=True,targetGroupId='group-degraded')
                identity=self.submit();setattr(self.host,'fail_'+failure,True);self.host.tick(3)
                report=self.host.request('api/report?id='+identity)
                self.assertEqual(report['disableState'],'failed' if failure=='disable' else 'disabled')
                self.assertEqual(report['groupState'],'failed' if failure=='move' else 'moved')
                setattr(self.host,'fail_'+failure,False)
                self.host.request('api/retry-'+('disable' if failure=='disable' else 'group'),{'id':identity})
                report=self.host.request('api/report?id='+identity)
                self.assertEqual(report['disableState'],'disabled');self.assertEqual(report['groupState'],'moved');self.assertEqual(len(self.host.executions),1)

    def test_move_verification_retry_is_idempotent_and_uses_snapshot(self):
        self.confirmed_config(autoMoveGroup=True,targetGroupId='group-degraded');identity=self.submit()
        self.config(autoMoveGroup=False,targetGroupId='group-test')
        self.host.ignore_move=True;self.host.tick()
        self.assertEqual(self.host.request('api/report?id='+identity)['groupState'],'failed')
        self.host.ignore_move=False;self.host.memberships=['group-degraded']
        self.host.request('api/retry-group',{'id':identity})
        self.assertEqual(self.host.request('api/report?id='+identity)['groupState'],'moved');self.assertEqual(self.host.move_calls,1);self.assertTrue(self.host.enabled)

    def test_deleted_group_and_identity_change_prevent_mutations(self):
        for changed in ['group','identity']:
            with self.subTest(changed=changed):
                if self.host.executions:self.host.close();self.host=Host();self.addCleanup(self.host.close)
                self.confirmed_config(autoDisable=False,autoMoveGroup=True,targetGroupId='group-degraded')
                identity=self.submit();original=self.host.callback
                def callback(method,p,body):
                    if method=='host.state.put' and p['namespace']=='jobs' and p['value'].get('groupState')=='pending':
                        if changed=='group':self.host.groups=self.host.groups[:1]
                        else:self.host.identity='replaced'
                    return original(method,p,body)
                self.host.callback=callback
                self.host.tick();self.assertEqual(self.host.request('api/report?id='+identity)['groupState'],'failed');self.assertEqual(self.host.updates,[])

    def test_interrupted_move_requires_retry_without_duplicate_update(self):
        self.confirmed_config(autoMoveGroup=True,targetGroupId='group-degraded');identity=self.submit()
        self.host.fail_put=lambda key,value:key[0]=='jobs' and value.get('groupState')=='moved'
        with self.assertRaises(AssertionError):self.host.tick()
        self.host.fail_put=None;self.host.tick()
        self.assertEqual(self.host.request('api/report?id='+identity)['groupState'],'failed')
        self.host.request('api/retry-group',{'id':identity})
        self.assertEqual(self.host.request('api/report?id='+identity)['groupState'],'moved');self.assertEqual(self.host.move_calls,1);self.assertEqual(len(self.host.executions),1)

    def test_no_actions_for_non_degraded_verdict(self):
        self.config(autoDisable=True,autoMoveGroup=True,targetGroupId='group-degraded',lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();self.host.tick(3)
        report=self.host.request('api/report?id='+identity)
        self.assertNotEqual(report['verdict'],'degraded');self.assertEqual(self.host.updates,[])

    def test_cold_preparation_does_not_shorten_25_second_probe(self):
        self.host.close();self.host=Host();self.addCleanup(self.host.close)
        self.config(lowConfidencePercent=0,degradedPercent=100);identity=self.submit()
        original=self.host.callback;delayed=[]
        def callback(method,p,body):
            if method=='host.state.put' and p['namespace']=='jobs' and p['value'].get('challenges') and not p['value'].get('pendingProbe') and not delayed:
                delayed.append(True);time.sleep(3)
            return original(method,p,body)
        self.host.callback=callback;self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['progress']['stage'],'waiting_request');self.assertEqual(report['roundAttempts'],0);self.assertEqual(self.host.executions,[])
        self.host.model_read_delay=25;self.host.tick();self.host.model_read_delay=0
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(len(report['attempts']),1);self.assertIsNone(report['attempts'][0]['error']);self.assertFalse(report['pendingProbe'])
        if report['status']=='running':self.host.tick()
        self.assertEqual(self.host.request('api/report?id='+identity)['status'],'completed');self.assertEqual(len(self.host.executions),1)

    def test_repeated_host_budget_timeouts_stop_with_diagnostics(self):
        self.config();identity=self.submit();original=self.host.callback;seen=set()
        def callback(method,p,body):
            if method=='host.model.stream_read' and len(self.host.executions) not in seen:
                seen.add(len(self.host.executions));self.host.model_read_delay=20
                value={'type':'response.output_text.delta','delta':'1,2,3,'}
                event=raw({'wire':{'protocol':'openai','payload':{'kind':'json','event':None,'data':value}}})
                event=b'GPE2'+struct.pack('>III',len(event),0,0)+event
                return {'events':1,'end':False},b'HME1'+struct.pack('>II',1,len(event))+event
            return original(method,p,body)
        self.host.callback=callback
        for _ in range(6):
            self.host.model_read_delay=0;self.host.tick(budget_ms=10000)
            if self.host.request('api/report?id='+identity)['status']=='failed':break
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'failed');self.assertEqual(len(report['attempts']),2);self.assertIn('连续两次',report['message'])
        for attempt in report['attempts']:
            self.assertEqual(attempt['diagnostics']['kind'],'host_budget_timeout');self.assertGreater(attempt['diagnostics']['receivedBytes'],0)
            self.assertGreater(attempt['diagnostics']['budgetMs'],8800);self.assertLessEqual(attempt['diagnostics']['budgetMs'],9400)
        self.host.tick();self.assertEqual(len(self.host.executions),2)

    def test_configured_timeout_keeps_five_attempt_limit(self):
        self.config(timeoutSeconds=1);identity=self.submit();self.host.model_read_delay=2;self.host.tick(5)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'failed');self.assertEqual(len(report['attempts']),5)
        self.assertTrue(all(a['diagnostics']['kind']=='request_timeout' for a in report['attempts']))

    def test_timeout_accepts_valid_partial_output_and_separates_reasoning(self):
        self.config(timeoutSeconds=1,lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();original=self.host.callback;sent=[]
        text=','.join(str((i*i+i*97)%355+1) for i in range(240))+',35'
        def callback(method,p,body):
            if method=='host.model.stream_read' and not sent:
                sent.append(True);self.host.model_read_delay=2
                return model_events({'type':'response.reasoning_text.delta','delta':'推理内容'},
                                    {'type':'response.output_text.delta','delta':text})
            return original(method,p,body)
        self.host.callback=callback;self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertIsNone(report['attempts'][0]['error'])
        d=report['attempts'][0]['diagnostics']
        self.assertTrue(d['partial']);self.assertEqual(d['kind'],'request_timeout')
        self.assertEqual(d['outputBytes'],len(text.encode()));self.assertEqual(d['reasoningBytes'],12)
        self.assertGreater(d['receivedBytes'],d['outputBytes']+d['reasoningBytes'])
        self.assertEqual(report['status'],'completed');self.assertEqual(self.host.closes,1)
        self.assertEqual(len(self.host.executions),1)

    def test_host_timeout_persists_partial_sample_and_scores_without_reprobe(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();original=self.host.callback;sent=[]
        def callback(method,p,body):
            if method=='host.model.stream_read' and not sent:
                sent.append(True);self.host.model_read_delay=20
                return model_events({'type':'response.output_text.delta','delta':'12,34,56,'*80})
            return original(method,p,body)
        self.host.callback=callback;self.host.tick(budget_ms=10000)
        report=self.host.request('api/report?id='+identity)
        if report['status']=='running':
            # A durable receipt or validated sample can wait for the next callback.
            saved=self.host.state[('jobs',identity)]['value']
            self.assertTrue(saved.get('pendingOutcome') or saved['samples'])
            self.assertFalse(report['pendingProbe']);self.assertIn(report['progress']['stage'],('waiting_validation','waiting_score'))
            self.host.tick();report=self.host.request('api/report?id='+identity)
        self.assertTrue(report['attempts'][0]['diagnostics']['partial']);self.assertIsNone(report['attempts'][0]['error'])
        self.assertEqual(report['status'],'completed')
        self.assertEqual(len(report['rounds']),1)
        self.host.model_read_delay=0;self.host.tick()
        self.assertEqual(self.host.request('api/report?id='+identity)['status'],'completed')
        self.assertEqual(len(self.host.executions),1)

    def test_reasoning_only_timeout_never_becomes_sample(self):
        self.config(timeoutSeconds=1,autoDisable=True,autoMoveGroup=True,targetGroupId='group-degraded')
        identity=self.submit();original=self.host.callback;sent=[]
        def callback(method,p,body):
            if method=='host.model.stream_read' and not sent:
                sent.append(True);self.host.model_read_delay=2
                return model_events({'type':'response.reasoning_summary_text.delta','delta':'12,34,56,'*100})
            return original(method,p,body)
        self.host.callback=callback;self.host.tick()
        report=self.host.request('api/report?id='+identity);d=report['attempts'][0]['diagnostics']
        self.assertEqual(d['outputBytes'],0);self.assertGreater(d['reasoningBytes'],0)
        self.assertIsNotNone(report['attempts'][0]['error']);self.assertEqual(self.host.state[('jobs',identity)]['value']['samples'],[])
        self.assertEqual(self.host.disable_calls+self.host.move_calls,0)

    def test_partial_eof_and_output_limit_use_sample_validation_but_failure_does_not(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        original=self.host.callback
        for index,termination in enumerate(['eof','limit','short','failed','filtered','malformed']):
            with self.subTest(termination=termination):
                identity=self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':f'partial-{index}'})['submitted'][0]
                def callback(method,p,body):
                    if method=='host.model.stream_read':
                        text='12,34,' if termination=='short' else '12,34,56,'*80
                        delta={'type':'response.output_text.delta','delta':text}
                        if termination=='limit':return model_events(delta,{'type':'response.incomplete','response':{'incomplete_details':{'reason':'max_output_tokens'}}},end=True)
                        if termination=='failed':return model_events(delta,{'type':'response.failed'},end=True)
                        if termination=='filtered':return model_events(delta,{'type':'response.incomplete','response':{'incomplete_details':{'reason':'content_filter'}}},end=True)
                        if termination=='malformed':return {'events':2,'end':True},model_events(delta,end=True)[1]
                        return model_events(delta,end=True)
                    return original(method,p,body)
                self.host.callback=callback
                # Cold preparation can yield to preserve the full request
                # window, especially while artifact checks run concurrently.
                for _ in range(3):
                    self.host.tick();report=self.host.request('api/report?id='+identity)
                    if report['attempts']:break
                if termination in ['eof','limit']:
                    if report['status']=='running' and report['attempts'] and report['attempts'][0]['error'] is None:
                        self.host.tick();report=self.host.request('api/report?id='+identity)
                    self.assertEqual(report['status'],'completed');self.assertTrue(report['attempts'][0]['diagnostics']['partial'])
                else:
                    self.host.request('api/cancel',{'id':identity})
                    self.assertIsNotNone(report['attempts'][0]['error']);self.assertEqual(report['rounds'],[])

    def test_websocket_metadata_then_completion_with_or_without_parsed_data(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        original=self.host.callback
        for parsed in [True,False]:
            with self.subTest(parsed=parsed):
                identity=self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':f'ws-{parsed}'})['submitted'][0]
                reads=[]
                text='12,34,56,'*80
                def callback(method,p,body):
                    if method=='host.model.stream_read':
                        reads.append(True)
                        if len(reads)==1:
                            return model_events({'type':'codex.response.metadata','headers':{'x-models-etag':'test'}},wire_kind='web_socket',parsed=parsed)
                        if len(reads)>2:
                            self.assertEqual(len(reads),3)
                            return model_events(end=True)
                        return model_events({'type':'response.reasoning_summary_text.delta','delta':'推理'},
                                            {'type':'response.output_text.delta','delta':text},
                                            {'type':'response.completed','response':{'output':[{'content':[{'type':'output_text','text':text}]}]}},
                                            wire_kind='web_socket',parsed=parsed)
                    return original(method,p,body)
                self.host.callback=callback;self.host.tick()
                report=self.host.request('api/report?id='+identity)
                self.assertEqual(report['status'],'completed');self.assertEqual(len(reads),3)
                self.assertEqual(self.host.closes,0)
                self.assertEqual(len(report['attempts']),1);self.assertIsNone(report['attempts'][0]['error'])
                d=report['attempts'][0]['diagnostics']
                self.assertEqual(d['kind'],'completed');self.assertEqual(d['outputBytes'],len(text.encode()))
                self.assertEqual(d['reasoningBytes'],len('推理'.encode()));self.assertGreater(d['receivedBytes'],d['outputBytes'])
                self.assertEqual(len(report['rounds']),1)

    def test_websocket_failure_is_rejected_and_output_limit_can_be_scored(self):
        self.config(lowConfidencePercent=0,degradedPercent=100,autoDisable=True,autoMoveGroup=True,targetGroupId='group-degraded')
        original=self.host.callback
        for terminal in ['response.failed','response.incomplete']:
            with self.subTest(terminal=terminal):
                identity=self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':terminal})['submitted'][0]
                def callback(method,p,body):
                    if method=='host.model.stream_read':
                        return model_events({'type':'response.output_text.delta','delta':'12,34,56,'*80},
                                            {'type':terminal,'response':{'incomplete_details':{'reason':'max_output_tokens'}}},
                                            wire_kind='web_socket',end=True)
                    return original(method,p,body)
                self.host.callback=callback;self.host.tick()
                report=self.host.request('api/report?id='+identity)
                if terminal=='response.failed':
                    self.assertIsNotNone(report['attempts'][0]['error']);self.assertEqual(report['rounds'],[])
                    self.assertGreater(report['attempts'][0]['diagnostics']['outputBytes'],0)
                    self.assertGreater(report['attempts'][0]['diagnostics']['receivedBytes'],0)
                    self.assertEqual(self.host.state[('jobs',identity)]['value']['samples'],[])
                    self.assertEqual(self.host.disable_calls+self.host.move_calls,0)
                    self.host.request('api/cancel',{'id':identity})
                else:
                    self.assertEqual(report['status'],'completed');self.assertIsNone(report['attempts'][0]['error'])
                    self.assertEqual(report['attempts'][0]['diagnostics']['kind'],'output_limit')
                    self.assertTrue(report['attempts'][0]['diagnostics']['partial'])

    def test_completed_result_survives_eof_drain_timeout(self):
        self.config(lowConfidencePercent=0,degradedPercent=100,timeoutSeconds=1)
        identity=self.submit();original=self.host.callback;reads=[]
        def callback(method,p,body):
            result,payload=original(method,p,body)
            if method=='host.model.stream_read':
                reads.append(True)
                if len(reads)>1:self.host.callback_response_delay=3
                result['end']=False
            return result,payload
        self.host.callback=callback;self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'completed');self.assertFalse(report['attempts'][0]['diagnostics']['partial'])
        self.assertEqual(len(reads),2);self.assertEqual(self.host.closes,1)
        self.assertEqual(report['attempts'][0]['diagnostics']['kind'],'completed')

    def test_batch_continues_in_same_callback_after_confirmed_move(self):
        self.host.additional_accounts=[{'id':'acct-two','name':'second','provider':'openai','enabled':True,'status':'normal','planType':'plus','groups':[]}]
        self.confirmed_config(autoMoveGroup=True,targetGroupId='group-degraded')
        ids=self.host.request('api/submit',{'accountIds':['acct-test','acct-two'],'requestId':'continuous-batch'})['submitted']
        original=self.host.callback;order=[]
        def callback(method,p,body):
            # Parallel waves start peer probes before the first verdict's
            # account action finishes; both actions still complete inside
            # this callback before the reports are read.
            if method=='host.model.execute_stream':order.append(p['account_id'])
            return original(method,p,body)
        self.host.callback=callback;self.host.tick()
        self.assertEqual(len(self.host.executions),2)
        for identity in ids:
            report=self.host.request('api/report?id='+identity)
            self.assertEqual(report['verdict'],'degraded');self.assertEqual(report['groupState'],'moved')
        self.assertEqual(sorted(order),['acct-test','acct-two'])
        self.assertEqual(self.host.move_calls,1) # Mock memberships are shared, so the second move is already satisfied.

    def test_cancelled_maintenance_recovers_pending_probes(self):
        self.host.additional_accounts=[{'id':'acct-two','name':'second','provider':'openai','enabled':True,'status':'normal','groups':[]}]
        self.config(lowConfidencePercent=0,degradedPercent=100,maxConcurrency=2)
        ids=self.host.request('api/submit',{'accountIds':['acct-test','acct-two'],'requestId':'cancel-recovery'})['submitted']
        original=self.host.callback;delayed=False
        def callback(method,p,body):
            nonlocal delayed
            result=original(method,p,body)
            if (not delayed and method=='host.state.put' and p['namespace']=='jobs'
                    and p['value'].get('pendingProbe') and p['value'].get('accountId')=='acct-two'):
                delayed=True;self.host.callback_response_delay=8.5
            return result
        self.host.callback=callback
        with self.assertRaises(AssertionError) as cancelled:
            self.host.tick(budget_ms=8000)
        self.assertIn('timed out',str(cancelled.exception))
        before=len(self.host.executions)
        self.host.tick()
        for identity in ids:
            report=self.host.request('api/report?id='+identity)
            self.assertEqual(report['status'],'interrupted')
            self.assertFalse(report['pendingProbe'])
            self.assertIn('中断',report['message'])
        # An uncertain in-flight probe must not be silently resent.
        self.assertEqual(len(self.host.executions),before)

    def test_slow_move_defers_to_next_callback_without_duplicate_update(self):
        self.confirmed_config(autoMoveGroup=True,targetGroupId='group-degraded')
        identity=self.submit();original=self.host.callback
        def callback(method,p,body):
            result=original(method,p,body)
            if method=='host.http.dispatch' and '/accounts/batch-update' in p['uri']:
                self.host.callback_response_delay=20
            return result
        self.host.callback=callback;started=time.monotonic();self.host.tick(budget_ms=10000)
        report=self.host.request('api/report?id='+identity)
        self.assertLess(time.monotonic()-started,10)
        # The idempotent update completed host-side, but its response missed
        # this callback. Keep the action pending instead of reporting failure.
        self.assertEqual(report['groupState'],'pending');self.assertIsNone(report['groupError'])
        self.assertEqual(self.host.move_calls,1)
        self.host.tick()
        self.assertEqual(self.host.request('api/report?id='+identity)['groupState'],'moved')
        self.assertEqual(self.host.move_calls,1);self.assertEqual(len(self.host.executions),1)

    def test_registration_and_queries(self):
        _,payload=self.host.call('management.register',stage='registration',timeout_ms=5000);registration=json.loads(payload);self.assertIn('api/report',[r['path'] for r in registration['routes']])
        self.assertEqual(self.host.request('api/accounts')['accounts'][0]['groups'][0]['id'],'group-test')
        self.assertIn('gpt-6.1-sol',self.host.request('api/models?keyId=key-test')['models'])
    def test_batch_delete_history_skips_active_and_keeps_legacy_id(self):
        self.host.additional_accounts=[{'id':'acct-two','name':'second-account','provider':'openai','enabled':True,'status':'normal','planType':'plus','groups':[]}]
        self.config(lowConfidencePercent=0,degradedPercent=100)
        first=self.submit()
        second=self.host.request('api/submit',{'accountIds':['acct-two'],'requestId':'submit-two'})['submitted'][0]
        self.host.tick(2)
        for identity in [first,second]:
            self.assertEqual(self.host.request('api/report?id='+identity)['status'],'completed')
        active=self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'submit-active'})['submitted'][0]
        result=self.host.request('api/delete',{'ids':[first,second,active,first]})
        self.assertEqual(result,{'deleted':2,'skipped':1})
        self.assertEqual([j['id'] for j in self.host.request('api/jobs')['jobs']],[active])
        self.assertNotIn(('jobs',first),self.host.state);self.assertNotIn(('jobs',second),self.host.state)
        self.host.request('api/report?id='+first,ok=False)
        self.host.request('api/delete',{'id':active},ok=False)
        self.host.request('api/cancel',{'id':active})
        self.assertEqual(self.host.request('api/delete',{'id':active}),{'ok':True,'deleted':1,'skipped':0})
        self.assertNotIn(('jobs',active),self.host.state)
        self.assertNotIn(active,self.host.request('api/bootstrap')['jobs'])
    def test_history_keeps_three_days_without_count_limit(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();self.host.tick()
        self.assertEqual(self.host.request('api/report?id='+identity)['status'],'completed')
        self.host.close();state=copy.deepcopy(self.host.state)
        template=copy.deepcopy(state[('jobs',identity)]['value'])
        for key in ['samples','challenges','attempts','rounds']:template[key]=[]
        now_ms=int(time.time()*1000);ids=[]
        for i in range(500):
            job_id=f'j-history-{i:04d}';job=copy.deepcopy(template)
            job.update(id=job_id,status='completed',verdict='no_match',message='历史记录',createdAt=now_ms-1000,finishedAt=now_ms-500)
            state[('jobs',job_id)]={'value':job,'version':1,'schema_version':1};ids.append(job_id)
        state[('control','main')]['value']['jobIds'].extend(ids)
        old=ids[0];state[('jobs',old)]['value']['finishedAt']=now_ms-4*86400000
        self.host=Host(state=state);self.addCleanup(self.host.close)
        self.assertEqual(len(self.host.request('api/bootstrap')['jobs']),501)
        submitted=self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'after-500-history'})['submitted'][0]
        self.assertNotIn(('jobs',old),self.host.state)
        self.host.request('api/report?id='+old,ok=False)
        jobs=self.host.request('api/jobs')['jobs'];self.assertIn(submitted,[j['id'] for j in jobs]);self.assertEqual(len(jobs),501)

    def test_large_history_management_responses_stay_within_host_limit(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();self.host.tick(2)
        self.host.close();state=copy.deepcopy(self.host.state)
        template=state[('jobs',identity)]['value']
        now_ms=int(time.time()*1000);ids=[]
        for i in range(1200):
            job_id=f'large-history-{i:04d}';job=copy.deepcopy(template)
            job.update(id=job_id,message='历史检测结果与错误详情'*60,createdAt=now_ms-1000,finishedAt=now_ms-500)
            state[('jobs',job_id)]={'value':job,'version':1,'schema_version':1};ids.append(job_id)
        state[('control','main')]['value']['jobIds']=[identity,*ids]
        self.host=Host(state=state);self.addCleanup(self.host.close)
        expected=[*reversed(ids),identity]
        for path in ['api/bootstrap','api/jobs','api/progress']:
            result=self.host.request(path)
            self.assertEqual([j['id'] for j in result['jobs']],expected)
        unchanged=self.host.request('api/progress?since='+result['revision'])
        self.assertFalse(unchanged['changed']);self.assertIsNone(unchanged['jobs'])
        self.assertEqual(self.host.request('api/report?id='+ids[-1])['message'],'历史检测结果与错误详情'*60)
        self.assertEqual(len(self.host.state),len(state))
        _,payload=self.host.call('management.handle',{'method':'GET','path':'api/jobs','query':'','content_type':None,'headers':[]})
        page=json.loads(payload)['pagination']
        self.host.request('api/delete',{'id':ids[-1]})
        data,payload=self.host.call('management.handle',{'method':'GET','path':'api/jobs',
            'query':urlencode({'pageOffset':page['nextOffset'],'pageSnapshot':page['snapshot']}),
            'content_type':None,'headers':[]})
        self.assertEqual(data['status'],409)
        self.assertEqual(json.loads(payload)['error']['code'],'pagination_changed')
        self.assertEqual(len(self.host.request('api/jobs')['jobs']),1200)

    def test_large_account_page_stays_within_host_limit(self):
        self.host.additional_accounts=[{'id':f'large-account-{i:04d}','name':'长账户名称'*200,
            'provider':'openai','enabled':True,'status':'normal','planType':'plus','groups':[]}
            for i in range(499)]
        accounts=self.host.request('api/accounts')['accounts']
        self.assertEqual(len(accounts),500)
        self.assertEqual(len({a['id'] for a in accounts}),500)

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
        # Isolate the configured request timeout from cold extraction, which
        # also consumes the host's 30-second budget (covered by artifact tests).
        self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'timeout-warmup'});self.host.tick()
        self.host.model_read_delay=21;identity=self.submit();self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(len(report['attempts']),1);self.assertIsNone(report['attempts'][0]['error'])
        if report['status']=='running':
            self.assertFalse(report['pendingProbe']);self.assertEqual(len(report['samples']),1)
            self.host.tick();report=self.host.request('api/report?id='+identity)
        self.assertEqual(len(self.host.executions),2) # Warmup + one slow probe; scoring never replays it.
        self.assertEqual(report['status'],'completed');self.assertEqual(len(report['rounds']),1);self.assertEqual(self.host.closes,0)
    def test_rpc_v7_cooperative_scheduling_and_callback_consumption(self):
        self.assertEqual(self.host.protocol_version,7)
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();self.host.callback_response_delay=0.05
        self.host.tick(2)
        self.assertEqual(self.host.request('api/report?id='+identity)['status'],'completed')
        kinds={message['type'] for message in self.host.scheduling}
        self.assertEqual(kinds,{'suspend','resume','callback_consumed'})
        self.assertFalse(self.host.pending_callbacks)
        self.assertFalse(self.host.suspended)
        self.assertEqual(len(self.host.executions),1)

    def test_declared_budget_allows_parallel_responses_after_thirty_seconds(self):
        self.assertEqual(self.host.protocol_version,7)
        self.assertEqual(self.host.call_budgets,{'maintenance':180000})
        self.config(lowConfidencePercent=0,degradedPercent=100,maxConcurrency=4)
        ids=['acct-test',*[f'acct-long-{i}' for i in range(3)]]
        self.host.additional_accounts=[{'id':i,'name':i,'provider':'openai','enabled':True,'status':'normal','groups':[]} for i in ids[1:]]
        submitted=self.host.request('api/submit',{'accountIds':ids,'requestId':'parallel-after-thirty'})['submitted']
        self.host.model_response_delay=31
        self.host.tick(budget_ms=self.host.call_budgets['maintenance'])
        for identity in submitted:
            report=self.host.request('api/report?id='+identity)
            self.assertEqual(report['status'],'completed')
            self.assertEqual(len(report['attempts']),1)
            self.assertIsNone(report['attempts'][0]['error'])
            diagnostics=report['attempts'][0]['diagnostics']
            self.assertGreaterEqual(diagnostics['elapsedMs'],31000)
            self.assertGreaterEqual(diagnostics['budgetMs'],119000)
            self.assertLessEqual(diagnostics['budgetMs'],120000)
        self.assertEqual(len(self.host.executions),4)
        self.assertEqual(self.host.closes,0)

    def test_declared_budget_allows_response_after_old_sdk_cap(self):
        self.config(timeoutSeconds=150,lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();self.host.model_response_delay=121
        self.host.tick(budget_ms=self.host.call_budgets['maintenance'])
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'completed')
        self.assertEqual(len(report['attempts']),1)
        self.assertIsNone(report['attempts'][0]['error'])
        self.assertGreaterEqual(report['attempts'][0]['diagnostics']['elapsedMs'],121000)
        self.assertGreaterEqual(report['attempts'][0]['diagnostics']['budgetMs'],149000)
        self.assertEqual(len(self.host.executions),1)
        self.assertEqual(self.host.closes,0)

    def test_late_response_scores_in_same_callback(self):
        # A response at 28.7s must use the extended request window and score
        # immediately, without waiting for another maintenance callback.
        self.config(lowConfidencePercent=0,degradedPercent=100)
        self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'waiting-score-warmup'});self.host.tick()
        for _ in range(4):
            if all(j['status'] not in ('queued','running') for j in self.host.request('api/jobs')['jobs']):break
            self.host.tick()
        identity=self.submit();self.host.model_read_delay=28.7
        self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'completed')
        self.assertEqual(len(report['attempts']),1);self.assertIsNone(report['attempts'][0]['error']);self.assertEqual(len(report['rounds']),1)
        self.assertGreater(report['attempts'][0]['diagnostics']['budgetMs'],28500)
        self.assertLessEqual(report['attempts'][0]['diagnostics']['budgetMs'],29400)
        self.host.model_read_delay=0;self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'completed');self.assertEqual(len(report['rounds']),1)
        self.assertEqual(len(self.host.executions),2) # Warmup + one slow probe; scoring never replays it.
    def test_detection_finishes_without_usage_queries(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();self.host.tick(2)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'completed')
        self.assertNotIn('usage',report)
        self.assertNotIn('usage',self.host.request('api/jobs')['jobs'][0])
        self.assertNotIn('usage',self.host.request('api/progress')['jobs'][0])
        self.assertNotIn('usage',report['attempts'][0])
        self.assertNotIn('costCheckedAt',report['attempts'][0])
        self.host.request('api/costs',{'ids':[identity]},ok=False)

    def test_legacy_usage_fields_do_not_break_history_after_restart(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();self.host.tick(2)
        before=self.host.request('api/report?id='+identity)
        state=copy.deepcopy(self.host.state);self.host.close()
        attempt=state[('jobs',identity)]['value']['attempts'][0]
        attempt.update(usage={'complete':False,'costs':[{'currency':'USD','estimatedAmount':'0.12'}]},costCheckedAt=123)
        self.host=Host(state=state);self.addCleanup(self.host.close)
        restored=self.host.request('api/report?id='+identity)
        # Use the upgrade check's tolerance for JSON float round trips.
        from upgrade_smoke import equivalent
        equivalent(before,restored)
        self.host.tick()
        self.assertEqual(len(self.host.executions),0)
        self.assertNotIn('usage',self.host.request('api/bootstrap')['jobs'][0])

    def test_scoring_defers_only_when_saving_consumes_remaining_budget(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'defer-warmup'});self.host.tick()
        identity=self.submit();original=self.host.callback;delayed=[]
        def callback(method,p,body):
            if method=='host.state.put' and p['namespace']=='jobs' and p['key']==identity and p['value']['samples'] and not delayed:
                delayed.append(True);self.host.callback_response_delay=0.7
            return original(method,p,body)
        self.host.callback=callback;self.host.model_read_delay=8.7;self.host.tick(budget_ms=10000)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'running');self.assertEqual(report['progress']['stage'],'waiting_score')
        self.assertEqual(len(self.host.state[('jobs',identity)]['value']['samples']),1)
        self.host.model_read_delay=0;self.host.tick()
        self.assertEqual(self.host.request('api/report?id='+identity)['status'],'completed')
        self.assertEqual(len(self.host.executions),2)

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
        self.config(lowConfidencePercent=0,degradedPercent=100)
        self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'restart-two'});state=self.host.state;self.host.close();self.host=Host(state=state);self.addCleanup(self.host.close)
        self.assertEqual(self.host.request('api/jobs')['jobs'][0]['status'],'queued')
        self.host.tick(2)
        self.assertEqual(self.host.request('api/jobs')['jobs'][0]['status'],'completed')
        self.assertEqual(len(self.host.executions),1)
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
        self.config(autoDisable=True,degradedModels=models,lowConfidencePercent=0,degradedPercent=0,confirmationRounds=3)
        identity=self.submit();self.host.tick(2);self.host.fail_disable=True;self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['verdict'],'degraded');self.assertEqual(len(report['rounds']),3);self.assertEqual(self.host.disable_calls,0)
        self.assertEqual(report['disableState'],'failed')
        self.host.fail_disable=False;self.host.request('api/retry-disable',{'id':identity});self.assertEqual(self.host.disable_calls,1);self.assertEqual(len(self.host.executions),3)
    def test_schedule_dynamic_scope_and_deduplication(self):
        c=self.config();c['schedule'].update({'enabled':True,'filter':{'plan':'plus','status':'normal','group':'group-test','keyword':'USER@'}})
        b=self.host.request('api/bootstrap');self.host.request('api/config',{'config':c,'revision':b['configRevision']})
        self.host.close();state=self.host.state;state[('control','main')]['value']['nextDue']=1
        self.host=Host(state=state);self.addCleanup(self.host.close);self.host.tick(2)
        jobs=self.host.request('api/jobs')['jobs'];self.assertEqual(len(jobs),1);self.assertEqual(jobs[0]['source'],'schedule')
    def test_schedule_filter_matches_multiple_plans(self):
        accounts=[{'id':'acct-pro','name':'pro','provider':'openai','enabled':True,'status':'normal','planType':'pro','groups':[]},{'id':'acct-free','name':'free','provider':'openai','enabled':True,'status':'normal','planType':'free','groups':[]}]
        self.host.additional_accounts=accounts
        c=self.config();c['schedule'].update({'enabled':True,'filter':{'plans':['plus','pro'],'status':'normal','group':'','keyword':''}})
        b=self.host.request('api/bootstrap');self.host.request('api/config',{'config':c,'revision':b['configRevision']})
        self.host.close();state=self.host.state;state[('control','main')]['value']['nextDue']=1
        self.host=Host(state=state);self.host.additional_accounts=accounts;self.addCleanup(self.host.close);self.host.tick(2)
        jobs=self.host.request('api/jobs')['jobs']
        self.assertEqual({j['accountId'] for j in jobs},{'acct-test','acct-pro'})
        self.assertTrue(all(j['source']=='schedule' for j in jobs))
    def arm_due_schedule(self, **overrides):
        cfg=self.config(**overrides)
        cfg['schedule'].update(enabled=True,scope='ids',accountIds=['acct-test'])
        b=self.host.request('api/bootstrap')
        self.host.request('api/config',{'config':cfg,'revision':b['configRevision']})
        self.host.close();state=self.host.state
        state[('control','main')]['value']['nextDue']=1
        self.host=Host(state=state);self.addCleanup(self.host.close)
        self.host.request('api/banks/builtin',{})

    def test_manual_does_not_skip_schedule_or_change_next_due(self):
        self.arm_due_schedule()
        # Identical request strings from different sources must remain independent.
        body={'accountIds':['acct-test'],'requestId':'schedule-1'}
        manual=self.host.request('api/submit',body)['submitted'][0]
        self.assertEqual(self.host.request('api/jobs')['nextDue'],1)
        # Assert duplicate rejection while the manual task is still queued.
        self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'duplicate-manual'},ok=False)
        self.host.tick()
        snapshot=self.host.request('api/jobs');jobs=snapshot['jobs']
        self.assertEqual({j['source'] for j in jobs},{'manual','schedule'})
        self.assertEqual(len({j['id'] for j in jobs}),2)
        self.assertEqual(self.host.request('api/submit',body)['submitted'],[manual])
        self.assertEqual(self.host.request('api/jobs')['nextDue'],snapshot['nextDue'])
        self.assertGreater(snapshot['nextDue'],1)

    def test_schedule_does_not_block_manual_and_sources_take_turns(self):
        self.arm_due_schedule(lowConfidencePercent=100,degradedPercent=100,lowConfidenceRetries=3)
        self.host.tick() # Queue automatic work first.
        scheduled=self.host.request('api/jobs')['jobs'][0]['id']
        manual=self.submit()
        before={identity:len(self.host.request('api/report?id='+identity)['rounds'])
                for identity in [scheduled,manual]}
        executions_before=len(self.host.executions)
        # Each ready source advances one round in the shared callback.
        self.host.tick()
        for identity in [scheduled,manual]:
            report=self.host.request('api/report?id='+identity)
            self.assertEqual(len(report['rounds']),before[identity]+1)
            self.assertEqual(report['status'],'running')
        self.assertEqual(len(self.host.executions),executions_before+2)

    def test_source_scoped_cancel_keeps_other_queue_and_schedule(self):
        self.arm_due_schedule(lowConfidencePercent=100,degradedPercent=100,lowConfidenceRetries=3)
        manual=self.submit();self.host.tick()
        jobs=self.host.request('api/jobs');due=jobs['nextDue']
        scheduled=next(j['id'] for j in jobs['jobs'] if j['source']=='schedule')
        scheduled_status=self.host.request('api/report?id='+scheduled)['status']
        self.assertIn(scheduled_status,('queued','running'))
        self.host.request('api/cancel',{'source':'invalid'},ok=False)
        self.host.request('api/cancel',{'source':'manual'})
        self.assertEqual(self.host.request('api/report?id='+manual)['status'],'cancelled')
        self.assertEqual(self.host.request('api/report?id='+scheduled)['status'],scheduled_status)
        other=self.host.request('api/submit',{'accountIds':['acct-test'],'requestId':'another-manual'})['submitted'][0]
        self.host.request('api/cancel',{'source':'schedule'})
        self.assertEqual(self.host.request('api/report?id='+scheduled)['status'],'cancelled')
        self.assertEqual(self.host.request('api/report?id='+other)['status'],'queued')
        self.assertEqual(self.host.request('api/jobs')['nextDue'],due)

    def test_source_cancel_during_probe_does_not_cancel_other_source(self):
        self.arm_due_schedule(lowConfidencePercent=0,degradedPercent=100)
        self.host.tick();scheduled=self.host.request('api/jobs')['jobs'][0]['id']
        manual=self.submit();original=self.host.callback;observed=[]
        def callback(method,p,body):
            if method=='host.model.stream_read' and not observed:
                observed.append(True)
                self.host.request('api/cancel',{'source':'manual'})
                jobs=self.host.request('api/progress')['jobs']
                auto=next(j for j in jobs if j['id']==scheduled)
                self.assertNotEqual(auto.get('progress',{}).get('stage'),'cancelling')
            return original(method,p,body)
        self.host.callback=callback;self.host.tick(2)
        self.assertTrue(observed)
        self.assertEqual(self.host.request('api/report?id='+manual)['status'],'cancelled')
        self.assertEqual(self.host.request('api/report?id='+scheduled)['status'],'completed')

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
        self.host.fail_put=lambda key,value:key[0]=='jobs' and bool(value.get('pendingOutcome'))
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
        # Within one callback the ready accounts are dispatched together, so
        # execution order is concurrent and not ordered by submission.
        self.assertEqual(sorted(p['account_id'] for p,_ in self.host.executions),['acct-test','acct-two'])
        for identity in ids:self.assertEqual(len(self.host.request('api/report?id='+identity)['rounds']),1)

    def test_parallel_wave_runs_accounts_together_in_one_callback(self):
        self.host.additional_accounts=[
            {'id':'acct-two','name':'second','provider':'openai','enabled':True,'status':'normal','groups':[]},
            {'id':'acct-three','name':'third','provider':'openai','enabled':True,'status':'normal','groups':[]}]
        self.assertEqual(self.host.request('api/bootstrap')['config']['maxConcurrency'],4)
        self.config(lowConfidencePercent=0,degradedPercent=100,maxConcurrency=3)
        ids=self.host.request('api/submit',{'accountIds':['acct-test','acct-two','acct-three'],'requestId':'parallel-wave'})['submitted']
        # A three-second upstream read: serial chains would spend about nine
        # seconds, an overlapped wave waits for one three-second window.
        self.host.model_read_delay=3
        started=time.monotonic();self.host.tick();elapsed=time.monotonic()-started
        self.host.model_read_delay=0
        self.assertLess(elapsed,7.2)
        self.assertEqual(len(self.host.executions),3)
        self.assertEqual(sorted(p['account_id'] for p,_ in self.host.executions),['acct-test','acct-three','acct-two'])
        for identity in ids:
            report=self.host.request('api/report?id='+identity)
            self.assertEqual(report['status'],'completed');self.assertEqual(len(report['rounds']),1)
            self.assertFalse(report['pendingProbe'])

    def test_parallel_wave_cancel_targets_single_job(self):
        self.host.additional_accounts=[{'id':'acct-two','name':'second','provider':'openai','enabled':True,'status':'normal','groups':[]}]
        self.config(lowConfidencePercent=0,degradedPercent=100,maxConcurrency=2)
        ids=self.host.request('api/submit',{'accountIds':['acct-test','acct-two'],'requestId':'parallel-cancel'})['submitted']
        original=self.host.callback;seen=[]
        def callback(method,p,body):
            if method=='host.model.stream_read' and not seen:
                seen.append(method)
                self.host.request('api/cancel',{'id':ids[1]})
            return original(method,p,body)
        self.host.callback=callback;self.host.model_read_delay=2;self.host.tick()
        self.host.model_read_delay=0
        self.assertTrue(seen)
        left=self.host.request('api/report?id='+ids[0]);right=self.host.request('api/report?id='+ids[1])
        self.assertEqual(left['status'],'completed');self.assertEqual(len(left['rounds']),1)
        self.assertEqual(right['status'],'cancelled')
        self.assertEqual(right['attempts'][0]['error'],'管理员取消检测')
        self.assertFalse(right['pendingProbe'])

    def test_sixteen_way_wave_finishes_near_deadline_with_slow_storage(self):
        ids=['acct-test']+[f'acct-{i}' for i in range(1,16)]
        self.host.additional_accounts=[{'id':i,'name':i,'provider':'openai','enabled':True,'status':'normal','groups':[]} for i in ids[1:]]
        self.config(lowConfidencePercent=0,degradedPercent=100,maxConcurrency=16)
        submitted=self.host.request('api/submit',{'accountIds':ids,'requestId':'sixteen-near-deadline'})['submitted']
        self.assertEqual(len(submitted),16)
        original=self.host.callback
        def callback(method,p,body):
            if method=='host.state.put' and p['namespace']=='jobs' and p['value'].get('samples'):
                time.sleep(0.08)
            return original(method,p,body)
        self.host.callback=callback;self.host.model_response_delay=25
        started=time.monotonic();self.host.tick();self.assertLess(time.monotonic()-started,30)
        reports=[self.host.request('api/report?id='+identity) for identity in submitted]
        self.assertTrue(all(not r['pendingProbe'] and len(r['attempts'])==1 for r in reports))
        if any(r['status']=='running' for r in reports):
            self.host.tick();reports=[self.host.request('api/report?id='+identity) for identity in submitted]
        self.assertTrue(all(r['status']=='completed' and len(r['rounds'])==1 for r in reports))
        self.assertEqual(len(self.host.executions),16)

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

    def test_invalid_samples_keep_healthy_worker(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();original=self.host.callback;workers=set()
        def callback(method,p,body):
            workers.update(Path(f'/proc/{self.host.process.pid}/task/{self.host.process.pid}/children').read_text().split())
            if method=='host.model.stream_read':
                return model_events({'type':'response.completed','response':{'output':[{'content':[{'type':'output_text','text':'1,2,3'}]}]}},end=True)
            return original(method,p,body)
        self.host.callback=callback;self.host.tick(3)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(len(report['attempts']),3);self.assertEqual(len(workers),1)
        self.assertTrue(all('有效数字' in a['error'] for a in report['attempts']))

    def test_restart_replays_receipt_and_saved_sample_without_model_request(self):
        for boundary in ('receipt','sample'):
            with self.subTest(boundary=boundary):
                if boundary=='sample':
                    self.host.close();self.host=Host();self.addCleanup(self.host.close)
                self.config(lowConfidencePercent=0,degradedPercent=100)
                identity=self.submit();original=self.host.callback;captured=[]
                def callback(method,p,body):
                    result=original(method,p,body)
                    if method=='host.state.put' and p['namespace']=='jobs' and not captured:
                        if (p['value'].get('pendingOutcome') if boundary=='receipt' else p['value']['samples']):
                            captured.append(copy.deepcopy(self.host.state))
                    return result
                self.host.callback=callback;self.host.tick();self.assertEqual(len(captured),1)
                self.host.close();self.host=Host(state=captured[0]);self.addCleanup(self.host.close)
                report=self.host.request('api/report?id='+identity)
                self.assertEqual(report['status'],'running');self.assertNotIn('pendingOutcome',report)
                self.assertNotIn('pendingOutcome',self.host.request('api/progress')['jobs'][0])
                self.host.tick(2);report=self.host.request('api/report?id='+identity)
                self.assertEqual(report['status'],'completed');self.assertEqual(len(report['attempts']),1)
                self.assertEqual(len(report['rounds']),1);self.assertEqual(len(self.host.executions),0)

    def test_validation_save_failure_replays_receipt_once(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();failed=[]
        def fail(key,value):
            if key==('jobs',identity) and value['samples'] and not failed:
                failed.append(True);return True
            return False
        self.host.fail_put=fail
        with self.assertRaisesRegex(AssertionError,'state unavailable'):self.host.tick()
        saved=self.host.state[('jobs',identity)]['value']
        self.assertIsNotNone(saved.get('pendingOutcome'));self.assertEqual(saved['attempts'],[])
        self.host.fail_put=None;self.host.request('api/bootstrap');self.host.tick(2)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'completed');self.assertEqual(len(report['attempts']),1)
        self.assertEqual(len(self.host.executions),1)

    def test_receipt_write_ack_timeout_recovers_known_result(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();original=self.host.callback;delayed=[]
        def callback(method,p,body):
            result=original(method,p,body)
            if method=='host.state.put' and p['namespace']=='jobs' and p['value'].get('pendingOutcome') and not delayed:
                delayed.append(True);self.host.callback_response_delay=20
            return result
        self.host.callback=callback
        with self.assertRaisesRegex(AssertionError,'保存超过维护预算'):self.host.tick(budget_ms=8000)
        self.host.request('api/bootstrap');self.host.tick(2)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'completed');self.assertEqual(len(report['attempts']),1)
        self.assertEqual(len(self.host.executions),1)

    def test_sixteen_invalid_answers_near_deadline_do_not_interrupt(self):
        self.host.additional_accounts=[{'id':f'acct-invalid-{i}','name':str(i),'provider':'openai','enabled':True,'status':'normal','groups':[]} for i in range(15)]
        self.config(maxConcurrency=16,lowConfidencePercent=0,degradedPercent=100)
        ids=self.host.request('api/submit',{'accountIds':['acct-test',*[a['id'] for a in self.host.additional_accounts]],'requestId':'invalid-wave'})['submitted']
        original=self.host.callback
        def callback(method,p,body):
            if method=='host.model.stream_read':
                text=','.join(str(i%355+1) for i in range(150))
                return model_events({'type':'response.completed','response':{'output':[{'content':[{'type':'output_text','text':text}]}]}},end=True)
            result=original(method,p,body)
            if method=='host.state.put' and p['namespace']=='jobs' and (p['value'].get('pendingOutcome') or p['value']['attempts']):
                self.host.callback_response_delay=0.08
            return result
        self.host.callback=callback;self.host.model_response_delay=26.2
        self.host.tick();self.host.request('api/bootstrap')
        reports=[self.host.request('api/report?id='+identity) for identity in ids]
        self.assertTrue(all(r['status']=='running' and not r['pendingProbe'] for r in reports))
        self.assertTrue(all(len(r['attempts'])==1 for r in reports));self.assertEqual(len(self.host.executions),16)

    def test_full_sixteen_way_timeouts_stop_after_two_waves(self):
        self.host.additional_accounts=[{'id':f'acct-timeout-{i}','name':str(i),'provider':'openai','enabled':True,'status':'normal','groups':[]} for i in range(15)]
        self.config(maxConcurrency=16)
        ids=self.host.request('api/submit',{'accountIds':['acct-test',*[a['id'] for a in self.host.additional_accounts]],'requestId':'full-wave-timeout'})['submitted']
        self.host.model_response_delay=10
        self.host.tick(2,budget_ms=8000)
        reports=[self.host.request('api/report?id='+identity) for identity in ids]
        self.assertTrue(all(r['status']=='failed' and len(r['attempts'])==2 for r in reports))
        for r in reports:
            self.assertIn('连续两次',r['message'])
            self.assertTrue(all(a['diagnostics']['fullWindow'] for a in r['attempts']))
            self.assertTrue(all(a['diagnostics']['budgetMs']<6500 for a in r['attempts']))
        self.assertEqual(len(self.host.executions),32)

    def test_later_wave_defers_without_spending_attempt(self):
        self.host.additional_accounts=[{'id':'acct-later','name':'later','provider':'openai','enabled':True,'status':'normal','groups':[]}]
        self.config(maxConcurrency=1,lowConfidencePercent=0,degradedPercent=100)
        ids=self.host.request('api/submit',{'accountIds':['acct-test','acct-later'],'requestId':'later-wave'})['submitted']
        self.host.model_response_delay=3;self.host.tick(budget_ms=10000)
        self.assertEqual(len(self.host.executions),1)
        later=self.host.request('api/report?id='+ids[1])
        self.assertEqual(later['roundAttempts'],0);self.assertFalse(later['pendingProbe'])
        self.assertEqual(later['progress']['stage'],'waiting_request')
        self.host.model_response_delay=0;self.host.tick()
        self.assertEqual(self.host.request('api/report?id='+ids[1])['status'],'completed')
        self.assertEqual(len(self.host.executions),2)

    def test_completed_event_drains_host_eof_without_cancellation(self):
        self.config(lowConfidencePercent=0,degradedPercent=100)
        identity=self.submit();original=self.host.callback;reads=[]
        def callback(method,p,body):
            if method=='host.model.stream_read':
                reads.append(True)
                if len(reads)==1:
                    text=','.join(str(i%355+1) for i in range(330))
                    return model_events({'type':'response.completed','response':{'output':[{'content':[{'type':'output_text','text':text}]}]}},end=False)
                return model_events(end=True)
            return original(method,p,body)
        self.host.callback=callback;self.host.tick()
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'completed');self.assertEqual(len(reads),2)
        self.assertEqual(self.host.closes,0);self.assertEqual(report['attempts'][0]['diagnostics']['kind'],'completed')

    def test_oversized_receipt_is_bounded_and_not_retried(self):
        self.config(autoDisable=True)
        identity=self.submit();original=self.host.callback
        def callback(method,p,body):
            if method=='host.model.stream_read':
                return model_events({'type':'response.completed','response':{'output':[{'content':[{'type':'output_text','text':'1,'*20000}]}]}},end=True)
            return original(method,p,body)
        self.host.callback=callback;self.host.tick(2)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'failed');self.assertIn('可持久化样本上限',report['message'])
        self.assertEqual(len(self.host.executions),1);self.assertEqual(len(report['attempts']),1)
        self.assertNotIn('pendingOutcome',report);self.assertEqual(self.host.disable_calls,0)

    def test_revoked_token_is_actionable_and_never_retried(self):
        self.config(autoDisable=True);identity=self.submit();original=self.host.callback;self.host.fail_model=True
        def callback(method,p,body):
            try:return original(method,p,body)
            except SimulatedFault:
                if method=='host.model.execute_stream':
                    raise SimulatedFault('OpenAI upstream failure: kind=unauthorized, status=401, code=token_revoked')
                raise
        self.host.callback=callback;self.host.tick(3)
        report=self.host.request('api/report?id='+identity)
        self.assertEqual(report['status'],'failed');self.assertIn('刷新凭据或重新授权',report['message'])
        self.assertEqual(len(self.host.executions),1);self.assertEqual(self.host.disable_calls,0)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=BINARY);parser.add_argument('--deny-timestamps',action='store_true',help='Use strace to deny utimensat and exercise restricted runtime extraction');parser.add_argument('--runtime-tmpdir',type=Path,help='Set TMPDIR for the plugin process to exercise runtime directory fallback');args,remaining=parser.parse_known_args();BINARY=args.binary;DENY_TIMESTAMPS=args.deny_timestamps;RUNTIME_TMPDIR=args.runtime_tmpdir
    # Host defaults are evaluated at definition time.
    Host.__init__.__defaults__=(BINARY,None,7)
    unittest.main(argv=[sys.argv[0],*remaining])
