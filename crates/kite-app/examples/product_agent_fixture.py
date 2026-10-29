#!/usr/bin/python3
"""Controlled herdr + Claude-format data fixture. No model or sample command runs.

Owns only its reported pane, transcript and event counters. The UI identifies it
as a product-acceptance fixture. Never execute incoming chat or approval text.
"""
import argparse
import ctypes
import json
import os
from pathlib import Path
import re
import select
import signal
import subprocess
import sys
import termios
import time
import tty
import uuid

parser = argparse.ArgumentParser()
parser.add_argument('--base', required=True)
parser.add_argument('--history', required=True)
parser.add_argument('--herdr', required=True)
parser.add_argument('--seconds', type=int, default=3600)
a = parser.parse_args()
# Give the controlled test double the expected foreground process identity.
ctypes.CDLL(None).prctl(15, b'claude', 0, 0, 0)
base, history = Path(a.base), Path(a.history)
base.mkdir(parents=True, exist_ok=True); history.parent.mkdir(parents=True, exist_ok=True)
pane = os.environ['HERDR_PANE_ID']
seq = int(time.time() * 1000)
running = True
mode = 'working'
selected = 0
paste = False
line = bytearray()
raw = bytearray()
started = time.monotonic()
state = {'pid': os.getpid(), 'pane_id': pane, 'messages': 0, 'approved': 0, 'rejected': 0, 'ticks': 0, 'alive': True, 'mode': mode}
(base/'agent.pid').write_text(str(os.getpid()))

def api(method, params):
    global seq
    seq += 1
    value = {'id': 'kite-fixture-'+str(seq), 'method': method, 'params': params}
    r = subprocess.run([a.herdr, 'remote-api-bridge'], input=json.dumps(value)+'\n', text=True, capture_output=True, timeout=10)
    response = json.loads(r.stdout)
    if 'error' in response: raise RuntimeError(response['error'])
    return response['result']

def append(kind, content):
    with history.open('a') as f:
        f.write(json.dumps({'type':kind, 'uuid':str(uuid.uuid4()), 'message':{'role':kind, 'content':content}}, ensure_ascii=False)+'\n')

def save():
    tmp=base/'events.tmp';tmp.write_text(json.dumps(state));tmp.replace(base/'events.json')

def report():
    state['mode'] = mode
    api('pane.report_agent', {'pane_id':pane, 'source':'herdr:claude', 'agent':'claude', 'state':mode,
                             'seq':seq+1, 'agent_session_id':history.stem})
    save()

def display():
    sys.stdout.write('\x1b[2J\x1b[H')
    print('Kite product acceptance fixture (no real model, no commands executed)')
    print('Claude-compatible transcript and live herdr terminal interaction.\n')
    if mode == 'blocked':
        print("Test command: printf 'Kite fixture approval'\nDo you want to proceed?")
        for index,label in enumerate(['Yes', 'No']):
            print(('❯ ' if index == selected else '  ')+str(index+1)+'. '+label)
        print('\nEnter to select · Esc to cancel')
    elif mode == 'working':
        print('Controlled computer-use fixture is working.\nDesktop takeover does not pause this process.')
        print('\n✶ Thinking…')
    else:
        print('Controlled fixture ready. Incoming chat is recorded, never executed.\n')
        print('❯ ', end='')
    sys.stdout.flush()

def change(new):
    global mode, selected
    mode=new; selected=0
    if mode == 'blocked':
        append('user',[{'type':'tool_result','tool_use_id':'fixture-computer','content':'Controlled desktop is ready.'}])
        append('assistant',[{'type':'text','text':'测试页面已打开。接下来需要你确认一个测试操作。'},
                            {'type':'tool_use','id':'fixture-approval-'+str(seq),'name':'Bash','input':{'command':"printf 'Kite fixture approval'"}}])
    report();display()

def stop(*_):
    global running
    running=False

signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
history.write_text('')
append('user','打开浏览器测一下登录流程')
append('assistant',[{'type':'text','text':'好的，我先启动开发服务器，然后在浏览器里走一遍登录。'},
                    {'type':'tool_use','id':'fixture-shell','name':'Bash','input':{'command':'npm run dev'}}])
append('user',[{'type':'tool_result','tool_use_id':'fixture-shell','content':'Controlled fixture: no command was executed.'}])
append('assistant',[{'type':'tool_use','id':'fixture-computer','name':'computer','input':{'action':'查看测试登录页面'}}])
report()
api('agent.rename', {'target':pane, 'name':'kite-product-fixture'})
old=termios.tcgetattr(sys.stdin.fileno());tty.setcbreak(sys.stdin.fileno())
control_stamp=None
try:
    display()
    while running and time.monotonic()-started<a.seconds:
        state['ticks']+=1
        control=base/'control.json'
        if control.exists():
            stamp=control.stat().st_mtime_ns
            if stamp!=control_stamp:
                control_stamp=stamp
                value=json.loads(control.read_text());new=value.get('mode',mode)
                if new in ['working','idle','blocked'] and new!=mode:change(new)
        if select.select([sys.stdin],[],[],.15)[0]:raw.extend(os.read(sys.stdin.fileno(),4096))
        while raw:
            if raw[0]==27:
                match=re.match(rb'\x1b\[[0-9;]*[~A-Za-z]',raw)
                if match:
                    token=bytes(match[0]);del raw[:len(token)]
                    if token==b'\x1b[200~':paste=True
                    elif token==b'\x1b[201~':paste=False
                    elif mode=='blocked' and token in [b'\x1b[A',b'\x1b[B']:
                        selected=1-selected;display()
                    continue
                if len(raw)<3:break
                raw.pop(0)
                if mode=='blocked':state['rejected']+=1;change('idle')
                continue
            byte=raw.pop(0)
            if byte in [10,13] and not paste:
                if mode=='blocked':
                    state['approved' if selected==0 else 'rejected']+=1;change('idle')
                elif line:
                    text=line.decode(errors='replace');line.clear();state['messages']+=1
                    state['last_message_matched']=text=='Kite 产品消息 29'
                    append('user',text)
                    append('assistant','已收到，测试夹具已记录这条消息。')
                    change('idle')
                continue
            if byte in [8,127]:
                if line:line.pop()
            elif byte>=32 or (paste and byte in [10,13]):line.append(byte)
        save()
finally:
    state['alive']=False;save()
    termios.tcsetattr(sys.stdin.fileno(),termios.TCSADRAIN,old)
    try:api('pane.release_agent',{'pane_id':pane,'source':'herdr:claude','agent':'claude','seq':seq+1})
    except Exception:pass
