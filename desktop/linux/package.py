#!/usr/bin/env python3
"""Build an allowlisted, reproducible Linux host archive; never includes user configuration."""
from pathlib import Path
import argparse,gzip,hashlib,io,json,tarfile

ROOT=Path(__file__).resolve().parents[2]
def package(binary,destination,version,commit):
    binary=Path(binary);destination=Path(destination);destination.parent.mkdir(parents=True,exist_ok=True)
    manifest=dict(version=version,sourceCommit=commit,target='x86_64-unknown-linux-gnu',nodeApi=1,backendProtocol=5,binarySha256=hashlib.sha256(binary.read_bytes()).hexdigest())
    files={'kite-node':(binary.read_bytes(),0o755),'host-manifest.json':((json.dumps(manifest,indent=2)+'\n').encode(),0o644),'LICENSE':((ROOT/'LICENSE').read_bytes(),0o644)}
    for name in ['desktop.py','strings.json','install.py']:files[name]=((ROOT/'desktop/linux'/name).read_bytes(),0o755 if name.endswith('.py') else 0o644)
    for state in ['ready','degraded','offline','busy']:
        name='icons/kite-'+state+'.svg';files[name]=((ROOT/'desktop/linux'/name).read_bytes(),0o644)
    prefix='kite-host-'+version+'-linux-x86_64'
    with destination.open('wb') as raw:
        with gzip.GzipFile(filename='',mode='wb',fileobj=raw,mtime=0) as compressed:
            with tarfile.open(fileobj=compressed,mode='w') as archive:
                for name,(data,mode) in sorted(files.items()):
                    item=tarfile.TarInfo(prefix+'/'+name);item.size=len(data);item.mode=mode;item.mtime=0;item.uid=item.gid=0;item.uname=item.gname=''
                    archive.addfile(item,io.BytesIO(data))
    return manifest
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--version',required=True);p.add_argument('--commit',required=True);a=p.parse_args()
    print(json.dumps(package(a.binary,a.output,a.version,a.commit)))
