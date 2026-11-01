#!/usr/bin/python3
"""Owned X11 target: records counters and a known text match, never arbitrary input."""
import argparse
import ctypes as C
import json
import os
from pathlib import Path
import signal
import hashlib
import gi
gi.require_version('Gtk','3.0')
gi.require_version('GdkX11','3.0')
from gi.repository import Gtk, Gdk, GdkX11, GLib

parser=argparse.ArgumentParser();parser.add_argument('--state',type=Path,required=True);args=parser.parse_args()
x=C.CDLL('libX11.so.6');x.XOpenDisplay.argtypes=[C.c_char_p];x.XOpenDisplay.restype=C.c_void_p
x.XGetInputFocus.argtypes=[C.c_void_p,C.POINTER(C.c_ulong),C.POINTER(C.c_int)]
x.XSetInputFocus.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_ulong];x.XSync.argtypes=[C.c_void_p,C.c_int]
display=x.XOpenDisplay(os.environ['DISPLAY'].encode());old=C.c_ulong();revert=C.c_int()
x.XGetInputFocus(display,C.byref(old),C.byref(revert))
state=dict(left=0,right=0,scroll=0,f8=False,text_matched=False,
           manager_environment_cleared=not any(k in os.environ for k in ['NOTIFY_SOCKET','LISTEN_FDS','CODEX_THREAD_ID','HERDR_ENV']))
window=Gtk.Window(title='Kite owned desktop API validation');window.set_default_size(520,360);window.set_border_width(20)
box=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=16);window.add(box)
box.pack_start(Gtk.Label(label='Kite · owned desktop API test'),False,False,0)
entry=Gtk.Entry();entry.set_placeholder_text('Owned UTF-8 test only');box.pack_start(entry,False,False,0)
target=Gtk.EventBox();target.add(Gtk.Label(label='CLICK / SCROLL TARGET'));target.set_size_request(-1,120)
target.add_events(Gdk.EventMask.BUTTON_PRESS_MASK|Gdk.EventMask.SCROLL_MASK);box.pack_start(target,True,True,0)
tile=Gtk.DrawingArea();tile.set_size_request(-1,40);box.pack_start(tile,False,False,0)
def paint(_,context):context.set_source_rgb(.12,.65,.35);context.paint();return False
tile.connect('draw',paint)
def save():
    if window.get_window():
        origin=window.get_window().get_origin();ox,oy=origin[-2:]
        state['window']=window.get_window().get_xid()
        for name,widget in [('entry',entry),('target',target),('tile',tile)]:
            px,py=widget.translate_coordinates(window,0,0);a=widget.get_allocation()
            state[name]=dict(x=ox+px,y=oy+py,width=a.width,height=a.height)
    temporary=args.state.with_suffix('.pending');temporary.write_text(json.dumps(state));temporary.replace(args.state)
    return True
def button(_,event):
    if event.button==1:state['left']+=1
    if event.button==3:state['right']+=1
    save();return False
def scroll(*_):state['scroll']+=1;save();return False
def key(_,event):
    if event.keyval==Gdk.KEY_F8:
        state['f8']=True;save()
    return False
# The 100 ms timer persists observations; synchronous geometry/X11 round trips
# on every key would make the target itself interfere with Unicode key mapping.
entry.connect('changed',lambda widget:state.update(text_matched=widget.get_text()=='Kite 你好 19',text_length=len(widget.get_text()),text_sha256=hashlib.sha256(widget.get_text().encode()).hexdigest()))
target.connect('button-press-event',button);target.connect('scroll-event',scroll);window.connect('key-press-event',key)
def stop(*_):
    current=C.c_ulong();mode=C.c_int();x.XGetInputFocus(display,C.byref(current),C.byref(mode))
    if window.get_window() and current.value==window.get_window().get_xid():
        handler=C.CFUNCTYPE(C.c_int,C.c_void_p,C.c_void_p)(lambda *_:0)
        x.XSetErrorHandler.argtypes=[C.c_void_p];x.XSetErrorHandler(C.cast(handler,C.c_void_p))
        x.XSetInputFocus(display,old.value,revert.value,0);x.XSync(display,0)
    Gtk.main_quit()
window.connect('destroy',stop);signal.signal(signal.SIGTERM,stop)
window.show_all();window.present();entry.grab_focus()
def focus():
    # This fixture owns this window. Allows headless X11 tests without a WM.
    x.XSetInputFocus(display,window.get_window().get_xid(),1,0);x.XSync(display,0);state['input_ready']=True;save();return False
GLib.timeout_add(200,focus);GLib.timeout_add(100,save);Gtk.main()
