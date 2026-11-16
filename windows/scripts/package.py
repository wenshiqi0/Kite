#!/usr/bin/env python3
"""Verify PE architecture, exports and loader dependencies before making a ZIP."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import zipfile


def pe(path):
    data = path.read_bytes()
    if data[:2] != b'MZ':
        raise ValueError(f'{path.name}: not a PE file')
    offset = struct.unpack_from('<I', data, 0x3c)[0]
    if data[offset:offset+4] != b'PE\0\0':
        raise ValueError(f'{path.name}: invalid PE signature')
    machine, count = struct.unpack_from('<HH', data, offset + 4)
    optional_size = struct.unpack_from('<H', data, offset + 20)[0]
    optional = offset + 24
    if machine != 0x8664 or struct.unpack_from('<H', data, optional)[0] != 0x20b:
        raise ValueError(f'{path.name}: expected Windows x64 PE32+')
    sections = [struct.unpack_from('<IIII', data, optional+optional_size+n*40+8) for n in range(count)]

    def rva(address):
        for virtual_size, virtual_address, raw_size, raw_address in sections:
            if virtual_address <= address < virtual_address + max(virtual_size, raw_size):
                return address - virtual_address + raw_address
        raise ValueError(f'{path.name}: invalid RVA {address:x}')

    def string(address):
        start = rva(address)
        return data[start:data.index(b'\0', start)].decode('ascii')

    imports = []
    import_rva = struct.unpack_from('<I', data, optional + 120)[0]
    if import_rva:
        at = rva(import_rva)
        while any(data[at:at+20]):
            imports.append(string(struct.unpack_from('<I', data, at+12)[0]))
            at += 20
    exports = []
    export_rva = struct.unpack_from('<I', data, optional + 112)[0]
    if export_rva:
        at = rva(export_rva)
        names = struct.unpack_from('<I', data, at+24)[0]
        table = rva(struct.unpack_from('<I', data, at+32)[0])
        exports = [string(struct.unpack_from('<I', data, table+n*4)[0]) for n in range(names)]
    return data, optional, imports, exports


def main():
    directory = Path(sys.argv[1]).resolve()
    # Retain the notices from the exact runtime packs used by dotnet publish.
    repo = Path(__file__).resolve().parents[2]
    assets = json.loads((repo/'windows/Kite/obj/project.assets.json').read_text())
    package_roots = [Path(os.environ['NUGET_PACKAGES'])] if 'NUGET_PACKAGES' in os.environ else [Path(path) for path in assets['packageFolders']]
    config = json.loads((directory/'Kite.runtimeconfig.json').read_text())
    for framework in config['runtimeOptions']['includedFrameworks']:
        package = framework['name'].lower()+'.runtime.win-x64'
        root = next((root/package/framework['version'] for root in package_roots if (root/package/framework['version']).is_dir()), None)
        if root is None:
            raise ValueError(f'runtime license source not found: {package}')
        for file in root.iterdir():
            if file.name.upper() in {'LICENSE','LICENSE.TXT','THIRD-PARTY-NOTICES.TXT'}:
                shutil.copyfile(file, directory/(framework['name']+'-'+file.name))
    exe = directory/'Kite.exe'
    data, optional, _, _ = pe(exe)
    # Cross-targeted .NET apphosts can retain the console subsystem. This is a GUI.
    subsystem = struct.unpack_from('<H', data, optional+68)[0]
    if subsystem == 3:
        changed = bytearray(data)
        struct.pack_into('<H', changed, optional+68, 2)
        exe.write_bytes(changed)
    elif subsystem != 2:
        raise ValueError('Kite.exe: invalid application subsystem')
    _, _, core_imports, exports = pe(directory/'kite_ffi.dll')
    for name in ['kite_windows_create', 'kite_windows_request', 'kite_windows_destroy', 'kite_windows_buffer_free',
                 'kite_windows_media_register', 'kite_windows_media_detach', 'kite_windows_media_text',
                 'kite_windows_media_key', 'kite_windows_media_release_input']:
        if name not in exports:
            raise ValueError(f'missing core export {name}')
    _, _, media_imports, _ = pe(directory/'kite_windows_media.dll')
    shipped = {file.name.lower() for file in directory.iterdir()}
    system = {'advapi32.dll','bcrypt.dll','bcryptprimitives.dll','crypt32.dll','dbghelp.dll','dnsapi.dll','gdi32.dll','iphlpapi.dll',
              'kernel32.dll','mfplat.dll','msvcrt.dll','ncrypt.dll','netapi32.dll','ntdll.dll','ole32.dll',
              'oleaut32.dll','powrprof.dll','propsys.dll','psapi.dll','rpcrt4.dll','secur32.dll','setupapi.dll',
              'shell32.dll','shlwapi.dll','user32.dll','userenv.dll','version.dll','winmm.dll','ws2_32.dll',
              'wtsapi32.dll','dwmapi.dll','ucrtbase.dll'}
    for dependency in core_imports + media_imports:
        if dependency.lower() not in system | shipped and not dependency.lower().startswith(('api-ms-win-', 'ext-ms-win-')):
            raise ValueError(f'unshipped native dependency: {dependency}')
    if shipped & {'kite-node.exe','herdr.exe','sunshine.exe','bun.exe'}:
        raise ValueError('client package contains a host executable')
    try:
        source = subprocess.check_output(['git','rev-parse','HEAD'], text=True).strip()
        dirty = bool(subprocess.check_output(['git','status','--porcelain'], text=True).strip())
    except subprocess.CalledProcessError:
        source, dirty = 'unknown', True
    version = dict(line.split('=', 1) for line in (repo/'android/version.properties').read_text().splitlines() if line and not line.startswith('#'))['versionName']
    files = {file.name: hashlib.sha256(file.read_bytes()).hexdigest() for file in sorted(directory.iterdir()) if file.is_file() and file.name != 'build-manifest.json'}
    (directory/'build-manifest.json').write_text(json.dumps({'source_commit':source,'version':version,'dirty':dirty,'target':'win-x64','built_on':'linux','host_role':False,'native_imports':{'core':core_imports,'media':media_imports},'sha256':files},indent=2)+'\n')
    archive = directory.parent/'Kite-windows-x64.zip'
    with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as bundle:
        for file in sorted(directory.iterdir()):
            if file.is_file(): bundle.write(file, f'Kite/{file.name}')
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix('.zip.sha256').write_text(f'{digest}  {archive.name}\n')
    print(f'Built {archive} ({archive.stat().st_size/1048576:.1f} MiB)\nSHA-256: {digest}')


if __name__ == '__main__':
    main()
