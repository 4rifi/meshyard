import subprocess,json,os,time,tempfile,pathlib,select
base=pathlib.Path(tempfile.mkdtemp(prefix='p4-',dir='/Volumes/TokensAbuse/tmp'))
node=pathlib.Path(os.environ.get('MESHYARD_TEST_NODE_BIN',str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'debug/meshyard-node')))
hook=pathlib.Path(os.environ['MESHYARD_TEST_HOOK_BIN'])
import hashlib
print('Node SHA256:',hashlib.sha256(node.read_bytes()).hexdigest())
print('Hook SHA256:',hashlib.sha256(hook.read_bytes()).hexdigest())
peers=[]
def start(name):
    home=base/name;home.mkdir(mode=0o700,exist_ok=True)
    data=home/'data'
    env=dict(os.environ,HOME=str(home),MESHYARD_DATA_DIR=str(data),MESHYARD_HOOK_HOME=str(home),MESHYARD_KEY_STORE='file',MESHYARD_TEST_DISABLE_MAPPING='1')
    log=open(home/'stderr.log','a')
    p=subprocess.Popen([str(node),'--data-dir',str(data),'--key-store','file'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,env=env,text=True,bufsize=1)
    peers.append((p,log));return p,home

def req(peer,action,**kwargs):
    peer.stdin.write(json.dumps(dict(action=action,**kwargs))+'\n');peer.stdin.flush()
    if not select.select([peer.stdout],[],[],20)[0]:raise AssertionError('Node response deadline')
    line=peer.stdout.readline()
    if not line:raise AssertionError('Node exited')
    result=json.loads(line)
    if 'error' in result:raise AssertionError('Node rejected '+action+': '+result['error'])
    return result['ok']

def wait(fn,seconds=15):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        value=fn()
        if value:return value
        time.sleep(.1)
    raise AssertionError('Expected state not reached')
def feed(home, kind, fixture):
    root=pathlib.Path(__file__).resolve().parents[1]/'crates/meshyard-agents/tests/fixtures'
    folder='claude-code-doc-derived-2026-11-03' if kind=='claude-code' else 'codex-doc-derived-2026-11-05'
    data=(root/folder/(fixture+'.json')).read_bytes()
    r=subprocess.run([str(hook),kind,'--socket',str(home/'data/ingest/ingest.sock')],input=data,capture_output=True,timeout=2)
    assert r.returncode==0 and not r.stdout and not r.stderr

def remote_state(peer,label,state,provenance):
    status=req(peer,'status')
    if 'Ready' not in status:return False
    agents=status['Ready']['agents']
    return any(a['label']==label and a['state']==state and a['provenance']==provenance and not a['is_you'] for a in agents)

try:
    for scenario in ['joiner-first','owner-first-resume']:
        a,ah=start(scenario+'-owner');b,bh=start(scenario+'-joiner')
        assert 'Ready' in req(a,'create',name='Direct yard',display_name='Owner')
        req(a,'start');time.sleep(1)
        ticket=req(a,'invite')['link']
        req(b,'join',link=ticket,display_name='Joiner')
        ap=wait(lambda:req(a,'pairings')['pairings']);bp=wait(lambda:req(b,'pairings')['pairings'])
        words=ap[0]['words'];assert words==bp[0]['words'] and len(words)==6
        assert len(req(a,'status')['Ready']['members'])==1
        if scenario=='owner-first-resume':
            req(a,'confirm',id=ap[0]['id'],matches=True)
            assert len(req(a,'status')['Ready']['members'])==1
            # Stop only our exact children while reservation is durable; retain both APFS profiles.
            a.terminate();a.wait(timeout=10);b.terminate();b.wait(timeout=10)
            a,ah=start(scenario+'-owner');b,bh=start(scenario+'-joiner')
            req(a,'start');time.sleep(.5)
            req(b,'join',link=ticket,display_name='Joiner')
            ap=wait(lambda:req(a,'pairings')['pairings']);bp=wait(lambda:req(b,'pairings')['pairings'])
            assert ap[0]['words']==bp[0]['words']==words
            assert ap[0]['local_confirmed']
            assert len(req(a,'status')['Ready']['members'])==1
            req(b,'confirm',id=bp[0]['id'],matches=True)
        else:
            req(b,'confirm',id=bp[0]['id'],matches=True)
            assert len(req(a,'status')['Ready']['members'])==1
            req(a,'confirm',id=ap[0]['id'],matches=True)
        wait(lambda:len(req(a,'status')['Ready']['members'])==2)
        wait(lambda:'Ready' in req(b,'status'))
        assert len(req(b,'status')['Ready']['members'])==2
        req(a,'register',kind='ClaudeCode',label='Owner Claude')
        req(b,'register',kind='Codex',label='Joiner Codex')
        wait(lambda:len(req(a,'status')['Ready']['agents'])==2)
        wait(lambda:len(req(b,'status')['Ready']['agents'])==2)
        wait(lambda:(ah/'data/ingest/ingest.sock').exists() and (bh/'data/ingest/ingest.sock').exists())
        for fixture in ['SessionStart','UserPromptSubmit','PreToolUse']:
            feed(ah,'claude-code',fixture);feed(bh,'codex',fixture);time.sleep(.15)
        wait(lambda:remote_state(b,'Owner Claude','Building',{'Inferred':{'rule':'ToolEdit'}}))
        wait(lambda:remote_state(a,'Joiner Codex','Building',{'Inferred':{'rule':'ToolEdit'}}))
        assert any(p['path']=='Direct P2P' for p in req(a,'pairings')['peers'])
        assert any(p['path']=='Direct P2P' for p in req(b,'pairings')['peers'])
        # Interrupt the owner after live sync, then prove cursor-based automatic reconnect.
        req(a,'stop');time.sleep(.5);req(a,'start')
        feed(ah,'claude-code','Stop')
        wait(lambda:remote_state(b,'Owner Claude','Finished',{'Verified':{'source':'Stop'}}),25)
        wait(lambda:req(b,'pairings')['error'] is None)
        c,ch=start(scenario+'-second-joiner')
        req(c,'join',link=ticket,display_name='Other')
        wait(lambda:req(c,'pairings')['error'])
        assert req(c,'status')=='Setup'
        wait(lambda:not req(c,'pairings')['running'])
        fresh=req(a,'invite')['link']
        req(c,'join',link=fresh,display_name='Other')
        cp=wait(lambda:req(c,'pairings')['pairings'])
        ap=wait(lambda:[p for p in req(a,'pairings')['pairings'] if p['state']=='Reserved'])
        assert cp[0]['words']==ap[0]['words']
        assert req(c,'pairings')['error'] is None
        req(a,'confirm',id=ap[0]['id'],matches=False)
        req(c,'confirm',id=cp[0]['id'],matches=False)
        wait(lambda:not req(c,'pairings')['running'])
        assert len(req(a,'status')['Ready']['members'])==2
        req(c,'quit');c.wait(timeout=10)
        req(a,'quit');a.wait(timeout=10);req(b,'quit');b.wait(timeout=10)
        print('PASS:',scenario,'matching SAS; no early membership; signed-owner admission; real bundled hook fixtures -> remote Building/ToolEdit; automatic direct reconnect -> Finished/Stop; single-use rejects second joiner; same-process manual retry reaches matching SAS without restart')
    print('Evidence profiles:',base)
finally:
    for p,log in peers:
        if p.poll() is None:
            try:req(p,'quit');p.wait(timeout=10)
            except Exception:p.terminate();p.wait(timeout=10)
        log.close()
