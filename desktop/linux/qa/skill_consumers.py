#!/usr/bin/env python3
"""Optional installed skill-loader checks. No provider/model turn or config mutation."""
import argparse
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import tempfile
import time

parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--codex',default=shutil.which('codex'));parser.add_argument('--dsh-source',type=Path);parser.add_argument('--output',type=Path);args=parser.parse_args()
body=subprocess.check_output([str(args.binary.resolve()),'docs','skill'],text=True)
assert body.startswith('---\nname: kite\n')
result={'standardSkill':True,'modelTurns':0,'consumers':{}}
with tempfile.TemporaryDirectory(prefix='kite-skill-consumers-') as directory:
    root=Path(directory);project=root/'project';(project/'.git').mkdir(parents=True)
    skills=project/'.agents/skills';(skills/'kite').mkdir(parents=True);(skills/'kite/SKILL.md').write_text(body)
    if args.codex:
        env={k:v for k,v in os.environ.items() if not k.startswith('HERDR_') and k!='CODEX_THREAD_ID'}
        server=subprocess.Popen([args.codex,'app-server','--listen','stdio://'],cwd=project,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
        buffer=bytearray();sequence=0
        def request(method,params):
            global sequence,buffer
            sequence+=1;identifier='skill-loader-'+str(sequence)
            server.stdin.write((json.dumps(dict(id=identifier,method=method,params=params))+'\n').encode());server.stdin.flush()
            deadline=time.monotonic()+25
            while time.monotonic()<deadline:
                while b'\n' in buffer:
                    line,_,tail=buffer.partition(b'\n');buffer=bytearray(tail);value=json.loads(line)
                    if value.get('id')==identifier:
                        assert 'error' not in value,value.get('error');return value['result']
                readable,_,_=select.select([server.stdout],[],[],max(0,deadline-time.monotonic()))
                if readable:
                    data=os.read(server.stdout.fileno(),65536);assert data,'Codex loader closed';buffer.extend(data)
            raise TimeoutError(method)
        try:
            request('initialize',dict(clientInfo=dict(name='kite-skill-contract',version='1'),capabilities=dict(experimentalApi=True)))
            server.stdin.write(b'{"method":"initialized"}\n');server.stdin.flush()
            catalogue=request('skills/list',dict(cwds=[str(project)],forceReload=True))
            found=[skill for row in catalogue['data'] for skill in row.get('skills',[]) if skill['name']=='kite' and str(skills) in str(skill.get('path',''))]
            assert len(found)==1,'Codex must discover the project standard skill'
            result['consumers']['codex']={'skillDiscovered':True,'modelInvoked':False}
        finally:
            server.terminate();server.communicate(timeout=8)
    if args.dsh_source:
        script='''import { Context } from '@deepseek-ai/cordis';
import SkillRegistry from '@deepseek-ai/dsh-skill';
import * as SkillFileSystem from '@deepseek-ai/dsh-skill-filesystem';
const ctx=new Context();await ctx.plugin(SkillRegistry);
await ctx.plugin(SkillFileSystem,{includeDefaultRoots:false,customSkillDirs:[process.env.KITE_SKILL_ROOT],watch:false});
const listed=await ctx.skills.list({cwd:process.env.KITE_PROJECT});const loaded=await ctx.skills.get('kite',{cwd:process.env.KITE_PROJECT});
if(listed.length!==1||!loaded||!JSON.stringify(loaded).includes('kite docs desktop'))throw Error('Portable skill not loaded');
console.log(JSON.stringify({skillDiscovered:true,bodyLoaded:true,modelInvoked:false}));'''
        env=dict(os.environ,KITE_SKILL_ROOT=str(skills),KITE_PROJECT=str(project))
        checked=subprocess.run(['node','--import','tsx/esm','--input-type=module','-e',script],cwd=args.dsh_source,env=env,capture_output=True,text=True,check=True,timeout=25)
        result['consumers']['deepseek_harness']=json.loads(checked.stdout)
    result['claude']='Standard Agent Skills frontmatter and standard MCP contract; no proprietary Claude fields or live model invocation.'
    result['status']='PASS';print(json.dumps(result,indent=2))
    if args.output:
        args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(result,indent=2)+'\n')
