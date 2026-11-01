#!/usr/bin/env python3
"""Real CLI/MCP/X11 integration on an owned target and a private Node."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import shutil
import struct
import subprocess
import tempfile
import time

parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--evidence',type=Path);args=parser.parse_args()
binary=args.binary.resolve();source=Path(__file__).resolve().parent
assert os.environ.get('DISPLAY'), 'Run on an owned X11 display'


class MCP:
    def __init__(self,socket):
        self.process=subprocess.Popen([str(binary),'--socket',str(socket),'mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        self.buffer=bytearray();self.sequence=0
    def call(self,method,params):
        self.sequence+=1;identifier=self.sequence
        self.process.stdin.write((json.dumps(dict(jsonrpc='2.0',id=identifier,method=method,params=params))+'\n').encode());self.process.stdin.flush()
        deadline=time.monotonic()+20
        while time.monotonic()<deadline:
            while b'\n' in self.buffer:
                line,_,tail=self.buffer.partition(b'\n');self.buffer=bytearray(tail);value=json.loads(line)
                if value.get('id')==identifier:return value
            ready,_,_=select.select([self.process.stdout],[],[],max(0,deadline-time.monotonic()))
            if ready:
                data=os.read(self.process.stdout.fileno(),65536)
                assert data,'MCP process closed';self.buffer.extend(data)
        raise TimeoutError(method)
    def tool(self,name,arguments):
        response=self.call('tools/call',dict(name=name,arguments=arguments));assert 'error' not in response,response
        value=response['result'];assert not value.get('isError'),value
        return value
    def close(self):
        self.process.stdin.close();self.process.wait(timeout=5)


with tempfile.TemporaryDirectory(prefix='kite-desktop-api-') as directory:
    root=Path(directory);node_dir=root/'node';node_dir.mkdir(mode=0o700)
    stable=root/'test-node';shutil.copyfile(binary,stable);stable.chmod(0o700);binary=stable
    interpreter=root/'interpreter'
    interpreter.write_text('#!/usr/bin/python3\nimport os,sys\nif len(sys.argv)>2 and sys.argv[2]=="notification-facts": print("{\\\"facts\\\":[]}")\nelse: os.execv("/usr/bin/python3",["/usr/bin/python3",*sys.argv[1:]])\n');interpreter.chmod(0o700)
    socket=node_dir/'host.sock';cfg=root/'node.json';state=root/'fixture.json'
    env={k:os.environ[k] for k in ['DISPLAY','XAUTHORITY','DBUS_SESSION_BUS_ADDRESS','XDG_RUNTIME_DIR','PATH'] if k in os.environ}
    cfg.write_text(json.dumps(dict(node_id='owned-desktop-api',machine='owned-fixture',socket=str(socket),backend=str(root/'backend.py'),herdr='/usr/bin/true',python=str(interpreter),systemctl='/usr/bin/false',services=[],environment=env)))
    process_env=dict(os.environ,NOTIFY_SOCKET='/owned-not-a-manager',LISTEN_FDS='99',CODEX_THREAD_ID='owned-not-a-real-thread',HERDR_ENV='1')
    node=subprocess.Popen([str(binary),'serve','--config',str(cfg)],env=process_env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    mcp=None;fixture_pid=None
    def cli(*arguments,stdin=None):
        result=subprocess.run([str(binary),'--socket',str(socket),'desktop',*map(str,arguments)],input=stdin,text=True,capture_output=True,timeout=30)
        assert result.returncode==0,result.stderr
        return json.loads(result.stdout)
    def fixture():return json.loads(state.read_text())
    def wait_for(predicate,label):
        deadline=time.monotonic()+6
        while time.monotonic()<deadline:
            if predicate():return
            time.sleep(.03)
        raise AssertionError(label + ': ' + json.dumps(fixture(),sort_keys=True))
    def own_focus():
        focused=int(subprocess.check_output(['xdotool','getwindowfocus'],env=env,text=True).strip())
        assert focused==fixture()['window'],'Focus moved away from owned fixture; refusing input'
    try:
        wait_for(lambda:socket.exists(),'private listener readiness')
        status=cli('status');assert status['available'] and status['width']>0
        launched=cli('launch','--','/usr/bin/python3',source/'agent_desktop_fixture.py','--state',state);fixture_pid=launched['pid']
        wait_for(lambda:state.exists() and 'tile' in fixture(),'owned window readiness')
        assert fixture()['manager_environment_cleared']
        own_focus();entry=fixture()['entry'];cli('click',entry['x']+10,entry['y']+entry['height']//2)
        own_focus();cli('key','ctrl+a');cli('type','--stdin',stdin='Kite 你好 19')
        wait_for(lambda:fixture()['text_matched'],'real UTF-8 text delivery')
        # The documented bound must work without partial timeout or Unicode keymap loss.
        own_focus();cli('key','ctrl+a');large='你好'*256;cli('type','--stdin',stdin=large)
        wait_for(lambda:fixture().get('text_sha256')==hashlib.sha256(large.encode()).hexdigest(),'maximum-length UTF-8 delivery')
        own_focus();cli('key','F8');wait_for(lambda:fixture()['f8'],'real key delivery')
        target=fixture()['target'];x=target['x']+target['width']//2;y=target['y']+target['height']//2
        own_focus();cli('move',x,y);cli('click',x,y);cli('click',x,y,'right');cli('scroll',0,2,'--at',x,y)
        wait_for(lambda:fixture()['left']>0 and fixture()['right']>0 and fixture()['scroll']>=2,'actual button and scroll events')
        mcp=MCP(socket);mcp.call('initialize',dict(protocolVersion='2025-11-25',capabilities={},clientInfo=dict(name='neutral-contract-probe',version='1')))
        tools=mcp.call('tools/list',{})['result']['tools'];assert {t['name'] for t in tools}=={'kite_host_status','kite_sessions_list','kite_docs','kite_desktop'}
        first=mcp.tool('kite_docs',dict(topics=['desktop']))
        text=first['content'][0]['text'];rev=text.split('revision=',1)[1].split(']',1)[0]
        cached=mcp.tool('kite_docs',dict(topics=['desktop'],known={'desktop':rev}))
        assert len(cached['content'][0]['text'])<len(text)//5
        region=fixture()['tile'];crop={**region,'x':region['x']+5,'y':region['y']+5,'width':40,'height':20}
        shot=mcp.tool('kite_desktop',dict(action='screenshot',params=dict(region=crop)))
        metadata=shot['structuredContent'];images=[c for c in shot['content'] if c['type']=='image'];assert len(images)==1
        png=base64.b64decode(images[0]['data']);assert png[:8]==b'\x89PNG\r\n\x1a\n'
        assert struct.unpack('!II',png[16:24])==(40,20)
        assert metadata['origin']==dict(x=crop['x'],y=crop['y']) and hashlib.sha256(png).hexdigest()==metadata['sha256']
        assert all('data' not in c and images[0]['data'] not in c.get('text','') for c in shot['content'] if c['type']=='text')
        assert Path(metadata['path']).stat().st_mode&0o777==0o600
        same=mcp.tool('kite_desktop',dict(action='screenshot',params=dict(region=crop,known_sha256=metadata['sha256'])))
        assert same['structuredContent']['unchanged'] and not any(c['type']=='image' for c in same['content'])
        bad=mcp.call('tools/call',dict(name='kite_desktop',arguments=dict(action='status',params=dict(action='launch',argv=['bad']))))
        assert bad.get('error',{}).get('code')==-32602
        invalid=cli('windows');assert isinstance(invalid['windows'],list)
        result=dict(status='PASS',actualX11=True,utf8Text=True,maxLengthUtf8Text=True,keyChordAndF8=True,leftAndRightClicks=True,scrollEvents=True,
                    configuredTarget=True,launchEnvironmentSanitized=True,standardMcpImage=True,cropOriginAndPixels=True,
                    privateArtifacts=True,unchangedImageOmitted=True,docsConditionalBody=True,actionOverrideRejected=True)
        print(json.dumps(result),flush=True)
        if args.evidence:
            args.evidence.mkdir(parents=True,exist_ok=True);(args.evidence/'native-validation.json').write_text(json.dumps(result,indent=2)+'\n');(args.evidence/'owned-crop.png').write_bytes(png)
    except BaseException:
        if args.evidence and state.exists():
            args.evidence.mkdir(parents=True,exist_ok=True)
            (args.evidence/'failed-state.json').write_text(state.read_text())
            region=fixture()['entry']
            shot=cli('screenshot','--region',region['x'],region['y'],region['width'],region['height'],'--output',args.evidence/'failed-entry.png')
        raise
    finally:
        if mcp:mcp.close()
        if fixture_pid:
            try:os.kill(fixture_pid,signal.SIGTERM)
            except ProcessLookupError:pass
        node.terminate();node.communicate(timeout=5)
