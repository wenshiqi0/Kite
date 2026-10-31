#!/usr/bin/env python3
"""Install a user-owned Kite node/panel, adopting explicitly named existing services."""
from pathlib import Path
import argparse,json,os,shutil,subprocess,uuid,tempfile,socket,hashlib,re

ROOT=Path(__file__).resolve().parent
def atomic(path,data,mode=0o600):
    path.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent,delete=False) as f:f.write(data);f.flush();os.fsync(f.fileno());name=f.name
    os.chmod(name,mode);os.replace(name,path)
def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'kite-node');p.add_argument('--enable',action='store_true');args=p.parse_args()
    home=Path.home();binary=args.binary.resolve();assert binary.is_file()
    manifest_path=ROOT/'host-manifest.json'
    if manifest_path.exists():
        manifest=json.loads(manifest_path.read_text())
        assert hashlib.sha256(binary.read_bytes()).hexdigest()==manifest['binarySha256'],'Host binary checksum mismatch'
    installed=home/'.local/bin/kite-node'
    changed=not installed.is_file() or hashlib.sha256(installed.read_bytes()).digest()!=hashlib.sha256(binary.read_bytes()).digest()
    version=json.loads(subprocess.check_output([str(binary),'version'],text=True))['version']
    release=home/'.local/lib/kite-desktop'/version;release.mkdir(parents=True,exist_ok=True,mode=0o700)
    atomic(release/'kite-node',binary.read_bytes(),0o700)
    for name in ['desktop.py','strings.json']:atomic(release/name,(ROOT/name).read_bytes(),0o700 if name.endswith('.py') else 0o600)
    for state in ['ready','degraded','offline','busy']:
        name='icons/kite-'+state+'.svg';atomic(release/name,(ROOT/name).read_bytes(),0o644)
    link=home/'.local/bin/kite-node';link.parent.mkdir(parents=True,exist_ok=True)
    pending=link.with_name('kite-node.new');pending.unlink(missing_ok=True);pending.symlink_to(release/'kite-node');pending.replace(link)
    gui=home/'.local/bin/kite-desktop'
    atomic(gui,('#!/bin/sh\nexec /usr/bin/python3 '+"'"+str(release/'desktop.py').replace("'","'\\''")+"'"+' "$@"\n').encode(),0o700)
    config_path=home/'.config/kite/node.json'
    if config_path.exists():config=json.loads(config_path.read_text())
    else:
        herdr=shutil.which('herdr') or str(home/'.local/bin/herdr')
        assert Path(herdr).is_file(),'Install herdr first'
        env={'PATH':str(home/'.local/bin')+':'+str(home/'.cargo/bin')+':/usr/local/bin:/usr/bin:/bin','XDG_RUNTIME_DIR':os.environ.get('XDG_RUNTIME_DIR','/run/user/'+str(os.getuid()))}
        # Adopt the actual capture session without changing the user's systemd environment.
        for proc in Path('/proc').iterdir():
            try:
                if not proc.name.isdigit() or (proc/'comm').read_text().strip()!='sunshine':continue
                for item in (proc/'environ').read_bytes().split(b'\0'):
                    key,_,value=item.partition(b'=')
                    if key.decode() in ['DISPLAY','XAUTHORITY','DBUS_SESSION_BUS_ADDRESS']:env[key.decode()]=value.decode()
                break
            except OSError:continue
        config=dict(node_id=str(uuid.uuid4()),machine=socket.gethostname(),socket=str(home/'.local/state/kite/node/host.sock'),backend=str(home/'.local/lib/kite/backend-v5.py'),herdr=herdr,python='/usr/bin/python3',systemctl=shutil.which('systemctl') or '/usr/bin/systemctl',environment=env,services=[dict(name='Herdr',unit='herdr.service',external=True),dict(name='Sunshine',unit='app-dev.lizardbyte.app.Sunshine.service',external=True)])
    atomic(config_path,(json.dumps(config,indent=2)+'\n').encode())
    unit=home/'.config/systemd/user/kite-node.service'
    assert all(re.fullmatch(r'[A-Za-z0-9@_.][A-Za-z0-9@_.-]*\.service',s['unit']) for s in config['services']),'Invalid service unit'
    units=' '.join(s['unit'] for s in config['services'])
    quote=lambda p:'"'+str(p).replace('\\','\\\\').replace('"','\\"').replace('%','%%')+'"'
    contents='[Unit]\nDescription=Kite desktop host node\nAfter=network.target '+units+'\nWants='+units+'\n\n[Service]\nType=simple\nExecStart='+quote(link)+' serve --config '+quote(config_path)+'\nRestart=on-failure\nRestartSec=2\nUMask=0077\nKillMode=process\n\n[Install]\nWantedBy=default.target\n'
    atomic(unit,contents.encode())
    entry=home/'.local/share/applications/io.github.wenshiqi0.Kite.desktop'
    desktop_quote=lambda path:'"'+str(path).replace('\\','\\\\').replace('"','\\"').replace('`','\\`').replace('$','\\$').replace('%','%%')+'"'
    desktop_entry='[Desktop Entry]\nType=Application\nName=Kite\nComment=Kite Linux host services\nExec='+desktop_quote(gui)+'\nIcon='+str(release/'icons/kite-ready.svg')+'\nTerminal=false\nCategories=Network;RemoteAccess;\n'
    atomic(entry,desktop_entry.encode(),0o644)
    autostart=home/'.config/autostart/io.github.wenshiqi0.Kite.desktop'
    # The launcher path is stable across upgrades. Preserve user opt-out/customization.
    if not autostart.exists():
        atomic(autostart,desktop_entry.replace('Exec='+desktop_quote(gui),'Exec='+desktop_quote(gui)+' --background').encode(),0o644)
    subprocess.run(['systemctl','--user','daemon-reload'],check=True)
    if args.enable:
        subprocess.run(['systemctl','--user','enable','--now','kite-node.service'],check=True)
        if changed:subprocess.run(['systemctl','--user','restart','kite-node.service'],check=True)
        # A service becoming active precedes its private listener being ready.
        import time
        for _ in range(50):
            probe=subprocess.run([str(link),'api-version'],capture_output=True,timeout=3)
            if probe.returncode==0:break
            time.sleep(.1)
        else:raise RuntimeError('Kite node listener did not become ready')
        subprocess.run([str(link),'ensure','all'],check=True)
    print(json.dumps(dict(version=version,node=str(link),desktop=str(gui),service=str(unit))))
if __name__=='__main__':main()
