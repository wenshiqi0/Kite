#!/usr/bin/env python3
"""Kite's X11 platform adapter. One bounded JSON request; no shell interpreter."""
import ctypes as C
import ctypes.util
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import struct
import subprocess
import sys
import time
import uuid
import zlib

MAX_IMAGE = 8 * 1024 * 1024


class DesktopError(Exception):
    pass


def session_environment():
    env = os.environ.copy()
    display = env.get('DISPLAY', '')
    if not display:
        raise DesktopError('display_not_configured')
    session_keys = ['DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'XDG_CONFIG_HOME',
                    'XDG_CACHE_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME', 'XAUTHORITY',
                    'XDG_CURRENT_DESKTOP', 'DESKTOP_SESSION', 'XDG_SESSION_TYPE',
                    'CHROME_CONFIG_HOME', 'CHROME_USER_DATA_DIR']
    for key in ['XDG_CONFIG_HOME','XDG_CACHE_HOME','XDG_DATA_HOME','XDG_STATE_HOME',
                'CHROME_CONFIG_HOME','CHROME_USER_DATA_DIR']:
        env.pop(key, None)
    names = {'xfce4-session', 'gnome-session-b', 'gnome-shell', 'ksmserver', 'plasmashell',
             'mate-session', 'cinnamon-sessio', 'lxqt-session'}
    for process in Path('/proc').iterdir():
        try:
            if not process.name.isdigit() or process.stat().st_uid != os.getuid():
                continue
            if (process / 'comm').read_text().strip() not in names:
                continue
            entries = (process / 'environ').read_bytes()[:262144].split(b'\0')
            values = dict(item.split(b'=', 1) for item in entries if b'=' in item)
            actual = values.get(b'DISPLAY', b'').decode().removesuffix('.0')
            if actual != display.removesuffix('.0'):
                continue
            for key in session_keys:
                env.pop(key, None)
                if key.encode() in values:
                    env[key] = values[key.encode()].decode()
            break
        except (OSError, UnicodeError):
            continue
    if env.get('XDG_SESSION_TYPE') == 'wayland':
        raise DesktopError('wayland_session_unsupported')
    # Neither graphical apps nor their own notify helpers may take over Node's
    # supervisor, inherit another agent's identity, or use a task's browser profile.
    for key in list(env):
        if key in {'NOTIFY_SOCKET', 'LISTEN_PID', 'LISTEN_FDS', 'LISTEN_FDNAMES',
                   'WATCHDOG_PID', 'WATCHDOG_USEC', 'INVOCATION_ID', 'SYSTEMD_EXEC_PID',
                   'CODEX_THREAD_ID', 'KITE_DESKTOP_ARTIFACTS'} or key.startswith('HERDR_'):
            env.pop(key, None)
    return env


class Image(C.Structure):
    _fields_ = [(name, kind) for name, kind in [
        ('width', C.c_int), ('height', C.c_int), ('xoffset', C.c_int), ('format', C.c_int),
        ('data', C.c_void_p), ('byte_order', C.c_int), ('bitmap_unit', C.c_int),
        ('bitmap_bit_order', C.c_int), ('bitmap_pad', C.c_int), ('depth', C.c_int),
        ('bytes_per_line', C.c_int), ('bits_per_pixel', C.c_int),
        ('red_mask', C.c_ulong), ('green_mask', C.c_ulong), ('blue_mask', C.c_ulong),
        ('obdata', C.c_void_p)]]


class X11:
    def __init__(self):
        library = ctypes.util.find_library('X11')
        if not library:
            raise DesktopError('libx11_missing')
        self.lib = C.CDLL(library)
        bindings = {
            'XOpenDisplay': ([C.c_char_p], C.c_void_p),
            'XDefaultScreen': ([C.c_void_p], C.c_int),
            'XRootWindow': ([C.c_void_p, C.c_int], C.c_ulong),
            'XDisplayWidth': ([C.c_void_p, C.c_int], C.c_int),
            'XDisplayHeight': ([C.c_void_p, C.c_int], C.c_int),
            'XCloseDisplay': ([C.c_void_p], C.c_int),
            'XInternAtom': ([C.c_void_p, C.c_char_p, C.c_int], C.c_ulong),
            'XFree': ([C.c_void_p], C.c_int),
            'XGetWindowProperty': ([C.c_void_p, C.c_ulong, C.c_ulong, C.c_long, C.c_long,
                                   C.c_int, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_int),
                                   C.POINTER(C.c_ulong), C.POINTER(C.c_ulong), C.POINTER(C.c_void_p)], C.c_int),
            'XGetGeometry': ([C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_int),
                             C.POINTER(C.c_int), C.POINTER(C.c_uint), C.POINTER(C.c_uint),
                             C.POINTER(C.c_uint), C.POINTER(C.c_uint)], C.c_int),
            'XTranslateCoordinates': ([C.c_void_p, C.c_ulong, C.c_ulong, C.c_int, C.c_int,
                                       C.POINTER(C.c_int), C.POINTER(C.c_int), C.POINTER(C.c_ulong)], C.c_int),
            'XGetImage': ([C.c_void_p, C.c_ulong, C.c_int, C.c_int, C.c_uint, C.c_uint,
                          C.c_ulong, C.c_int], C.POINTER(Image)),
            'XDestroyImage': ([C.POINTER(Image)], C.c_int),
        }
        for name, (arguments, result) in bindings.items():
            function = getattr(self.lib, name)
            function.argtypes, function.restype = arguments, result
        callback = C.CFUNCTYPE(C.c_int, C.c_void_p, C.c_void_p)
        self.error_handler = callback(lambda *_: 0)  # Windows can disappear mid-query.
        self.lib.XSetErrorHandler.argtypes = [callback]
        self.lib.XSetErrorHandler(self.error_handler)
        self.display = self.lib.XOpenDisplay(os.environ['DISPLAY'].encode())
        if not self.display:
            raise DesktopError('x11_unavailable')
        self.screen = self.lib.XDefaultScreen(self.display)
        self.root = self.lib.XRootWindow(self.display, self.screen)
        self.width = self.lib.XDisplayWidth(self.display, self.screen)
        self.height = self.lib.XDisplayHeight(self.display, self.screen)

    def close(self):
        self.lib.XCloseDisplay(self.display)

    def property(self, window, name):
        atom = self.lib.XInternAtom(self.display, name.encode(), 1)
        if not atom:
            return None
        kind, size, count, after, data = C.c_ulong(), C.c_int(), C.c_ulong(), C.c_ulong(), C.c_void_p()
        result = self.lib.XGetWindowProperty(self.display, window, atom, 0, 2048, 0, 0,
                                             C.byref(kind), C.byref(size), C.byref(count),
                                             C.byref(after), C.byref(data))
        if result or not data.value:
            return None
        try:
            if size.value == 32:
                return list(C.cast(data, C.POINTER(C.c_ulong))[:min(count.value, 512)])
            if size.value == 8:
                return C.string_at(data, min(count.value, 8192)).decode('utf-8', 'replace')
            return None
        finally:
            self.lib.XFree(data)

    def window(self, window):
        parent, x, y = C.c_ulong(), C.c_int(), C.c_int()
        width, height, border, depth = C.c_uint(), C.c_uint(), C.c_uint(), C.c_uint()
        if not self.lib.XGetGeometry(self.display, window, C.byref(parent), C.byref(x), C.byref(y),
                                     C.byref(width), C.byref(height), C.byref(border), C.byref(depth)):
            return None
        self.lib.XTranslateCoordinates(self.display, window, self.root, 0, 0,
                                        C.byref(x), C.byref(y), C.byref(parent))
        title = self.property(window, '_NET_WM_NAME') or self.property(window, 'WM_NAME') or ''
        return dict(id=hex(window), title=str(title)[:160], x=x.value, y=y.value,
                    width=width.value, height=height.value)

    def point(self, x, y):
        if not (0 <= x < self.width and 0 <= y < self.height):
            raise DesktopError('coordinate_out_of_bounds')

    def screenshot(self, region=None):
        x, y, width, height = (region[k] for k in ['x', 'y', 'width', 'height']) if region else (0, 0, self.width, self.height)
        self.point(x, y)
        if width <= 0 or height <= 0 or x + width > self.width or y + height > self.height or width * height > 16777216:
            raise DesktopError('invalid_capture_region')
        image = self.lib.XGetImage(self.display, self.root, x, y, width, height, C.c_ulong(-1).value, 2)
        if not image:
            raise DesktopError('capture_failed')
        try:
            frame = image.contents
            if not (frame.byte_order == 0 and frame.bits_per_pixel == 32 and
                    (frame.red_mask, frame.green_mask, frame.blue_mask) == (0xff0000, 0xff00, 0xff)):
                raise DesktopError('unsupported_pixel_format')
            raw = C.string_at(frame.data, frame.bytes_per_line * height)
            rows = []
            for row in range(height):
                pixels = raw[row * frame.bytes_per_line:row * frame.bytes_per_line + width * 4]
                rgb = bytearray(width * 3)
                rgb[0::3], rgb[1::3], rgb[2::3] = pixels[2::4], pixels[1::4], pixels[0::4]
                rows.append(b'\0' + rgb)
            def chunk(name, data):
                return struct.pack('!I', len(data)) + name + data + struct.pack('!I', zlib.crc32(name + data) & 0xffffffff)
            png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!IIBBBBB', width, height, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b''.join(rows), 3)) + chunk(b'IEND', b'')
            if len(png) > MAX_IMAGE:
                raise DesktopError('image_too_large_use_region')
            return png, dict(width=width, height=height, origin=dict(x=x, y=y))
        finally:
            self.lib.XDestroyImage(image)


def input_command(arguments, env, text=None):
    executable = shutil.which('xdotool', path=env.get('PATH'))
    if not executable:
        raise DesktopError('xdotool_missing')
    try:
        subprocess.run([executable, *arguments], env=env, input=text, text=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True,
                       timeout=max(10,len(text)*.03+2) if text else 10)
    except (OSError, subprocess.SubprocessError):
        raise DesktopError('input_dispatch_failed') from None


def dispatch(request, env, x11):
    action = request['action']
    if action == 'status':
        active = x11.property(x11.root, '_NET_ACTIVE_WINDOW') or []
        return dict(available=True, backend='x11', display=env['DISPLAY'], width=x11.width,
                    height=x11.height, active_window=x11.window(active[0]) if active else None,
                    input_available=shutil.which('xdotool', path=env.get('PATH')) is not None)
    if action == 'windows':
        windows = x11.property(x11.root, '_NET_CLIENT_LIST_STACKING') or x11.property(x11.root, '_NET_CLIENT_LIST') or []
        rows = [x11.window(window) for window in reversed(windows)]
        rows = [window for window in rows if window]
        return dict(windows=rows[:request.get('limit', 16)], total=len(rows), display=env['DISPLAY'])
    if action == 'screenshot':
        png, metadata = x11.screenshot(request.get('region'))
        digest = hashlib.sha256(png).hexdigest()
        metadata.update(sha256=digest, mime='image/png', unchanged=request.get('known_sha256') == digest)
        if metadata['unchanged']:
            return metadata
        directory = Path(os.environ['KITE_DESKTOP_ARTIFACTS'])
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        if directory.is_symlink() or directory.stat().st_uid != os.getuid() or stat.S_IMODE(directory.stat().st_mode) & 0o077:
            raise DesktopError('artifact_directory_not_private')
        for old in directory.glob('shot-*.png'):
            if re.fullmatch(r'shot-[0-9a-f]{32}\.png', old.name) and not old.is_symlink() and time.time() - old.stat().st_mtime > 3600:
                old.unlink(missing_ok=True)
        path = directory / ('shot-' + uuid.uuid4().hex + '.png')
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, 'wb') as output:
            output.write(png)
        metadata.update(path=str(path), bytes=len(png))
        return metadata
    if action in ('move', 'click'):
        x11.point(request['x'], request['y'])
        arguments = ['mousemove', str(request['x']), str(request['y'])]
        if action == 'click':
            arguments += ['click', str({'left': 1, 'middle': 2, 'right': 3}[request.get('button', 'left')])]
        input_command(arguments, env)
    elif action == 'scroll':
        if request.get('x') is not None:
            x11.point(request['x'], request['y'])
            input_command(['mousemove', str(request['x']), str(request['y'])], env)
        for count, positive, negative in [(request['dx'], 7, 6), (request['dy'], 5, 4)]:
            if count:
                input_command(['click', '--repeat', str(abs(count)), '--delay', '20', str(positive if count > 0 else negative)], env)
    elif action == 'key':
        input_command(['key', '--clearmodifiers', *request['keys']], env)
    elif action == 'type':
        input_command(['type', '--clearmodifiers', '--delay', '24' if any(ord(char)>127 for char in request['text']) else '1', '--file', '-'], env, request['text'])
    elif action == 'launch':
        process = subprocess.Popen(request['argv'], env=env, cwd=request.get('cwd') or str(Path.home()),
                                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL, start_new_session=True)
        return dict(launched=True, pid=process.pid, display=env['DISPLAY'])
    else:
        raise DesktopError('unknown_action')
    return dict(dispatched=True, action=action, display=env['DISPLAY'])


def main():
    os.umask(0o077)
    request = json.loads(sys.stdin.buffer.read(32769))
    env = session_environment()
    os.environ.update(DISPLAY=env['DISPLAY'])
    if env.get('XAUTHORITY'):
        os.environ['XAUTHORITY'] = env['XAUTHORITY']
    else:
        os.environ.pop('XAUTHORITY',None)
    desktop = X11()
    try:
        result = dispatch(request, env, desktop)
        print(json.dumps(dict(ok=True, result=result), ensure_ascii=False))
    finally:
        desktop.close()


if __name__ == '__main__':
    try:
        main()
    except DesktopError as error:
        print(json.dumps(dict(ok=False, error=str(error))))
    except (OSError, ValueError, KeyError, TypeError):
        print(json.dumps(dict(ok=False, error='desktop_adapter_failed')))
