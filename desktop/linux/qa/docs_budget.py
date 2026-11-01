#!/usr/bin/env python3
"""Measure shipped docs/tool definitions with named reference tokenizers, no model turn.

Run: uv run --no-project --with tiktoken python desktop/linux/qa/docs_budget.py --binary target/debug/kite-node
"""
import argparse
import json
from pathlib import Path
import subprocess
import tiktoken

parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--output',type=Path);args=parser.parse_args()
binary=str(args.binary.resolve())
def cli(*arguments):return subprocess.check_output([binary,*arguments],text=True)
index=cli('docs');manifest=json.loads(cli('docs','--json'));desktop=cli('docs','desktop')
skill=cli('docs','skill');revision=next(topic['revision'] for topic in manifest['topics'] if topic['id']=='desktop')
cached=cli('docs','desktop','--known','desktop='+revision)
manual=index+'\n'.join(cli('docs',topic['id']) for topic in manifest['topics'] if topic['id']!='skill')
catalogue='kite: '+skill.split('description: ',1)[1].split('\n',1)[0]
messages=[dict(jsonrpc='2.0',id=1,method='initialize',params=dict(protocolVersion='2025-11-25',capabilities={},clientInfo=dict(name='token-budget',version='1'))),dict(jsonrpc='2.0',id=2,method='tools/list',params={})]
response=subprocess.run([binary,'--socket','/missing/owned-budget.sock','mcp'],input=''.join(json.dumps(m)+'\n' for m in messages),capture_output=True,text=True,check=True,timeout=10)
tools=next(json.loads(line)['result']['tools'] for line in response.stdout.splitlines() if json.loads(line).get('id')==2)
schema=json.dumps(tools,separators=(',',':'),ensure_ascii=False)
initial=next(json.loads(line)['result']['instructions'] for line in response.stdout.splitlines() if json.loads(line).get('id')==1)
parts={'skill_catalogue':catalogue,'skill_body':skill,'index':index,'desktop_guide':desktop,
       'advanced_input_guide':cli('docs','desktop/input'),'conditional_unchanged':cached,
       'mcp_tool_definitions':schema,'mcp_initial_instructions':initial,'full_manual_once':manual}
result={'scope':'Named reference encodings, not a guarantee of native Claude/DeepSeek accounting; excludes images, model reasoning and unrelated harness context.', 'bytes':{name:len(text.encode()) for name,text in parts.items()},'encodings':{}}
for name in ['cl100k_base','o200k_base']:
    encode=tiktoken.get_encoding(name);counts={key:len(encode.encode(text)) for key,text in parts.items()}
    first=counts['skill_catalogue']+counts['skill_body']+counts['desktop_guide']
    result['encodings'][name]={'parts':counts,'first_desktop_cli':first,
        'first_desktop_mcp_if_schemas_eager':first+counts['mcp_tool_definitions']+counts['mcp_initial_instructions'],
        'desktop_then_advanced':first+counts['advanced_input_guide'],
        'body_recheck_saving_percent':round((1-counts['conditional_unchanged']/counts['desktop_guide'])*100,1),
        'first_vs_bulk_once_saving_percent':round((1-first/counts['full_manual_once'])*100,1)}
    assert counts['skill_catalogue']<=64 and counts['skill_body']<=220 and counts['index']<=350
    assert counts['desktop_guide']<=700 and counts['mcp_tool_definitions']<=850
    assert counts['conditional_unchanged']<counts['desktop_guide']/5
    assert first<counts['full_manual_once']*.65
result['status']='PASS';print(json.dumps(result,indent=2))
if args.output:
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(result,indent=2)+'\n')
