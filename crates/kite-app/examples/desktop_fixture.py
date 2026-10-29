#!/usr/bin/python3
"""Optional Linux integration fixture, not a host runtime dependency.

Paints an animated target and records only event counters and exact test-token
matches. Never records arbitrary keyboard text. Closes and restores focus.
"""
import argparse
import ctypes as C
import json
import math
import os
from pathlib import Path
import signal
import time
import gi

gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, Gdk, GLib

parser = argparse.ArgumentParser()
parser.add_argument('--state', required=True)
parser.add_argument('--seconds', type=int, default=180)
args = parser.parse_args()
state_path = Path(args.state)
state = dict(frame=0, left=0, right=0, scroll=0, keys=0, f8=False, text_matched=False, alive=True)
started = time.monotonic()
stopping = False
x = C.CDLL('libX11.so.6')
x.XOpenDisplay.argtypes = [C.c_char_p]; x.XOpenDisplay.restype = C.c_void_p
x.XGetInputFocus.argtypes = [C.c_void_p, C.POINTER(C.c_ulong), C.POINTER(C.c_int)]
x.XSetInputFocus.argtypes = [C.c_void_p, C.c_ulong, C.c_int, C.c_ulong]
x.XSync.argtypes = [C.c_void_p, C.c_int]
x.XCloseDisplay.argtypes = [C.c_void_p]
display = x.XOpenDisplay(os.getenv('DISPLAY', ':10').encode())
old_focus, old_revert = C.c_ulong(), C.c_int()
if display: x.XGetInputFocus(display, C.byref(old_focus), C.byref(old_revert))

window = Gtk.Window(title='Kite remote desktop validation')
window.set_default_size(700, 1000)
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
window.add(box)
heading = Gtk.Label()
heading.set_markup('<span font_desc="Sans 28" foreground="#2563eb">KITE · LIVE STREAM TEST</span>')
box.pack_start(heading, False, False, 16)
canvas = Gtk.DrawingArea()
canvas.set_hexpand(True); canvas.set_vexpand(True)
canvas.add_events(Gdk.EventMask.BUTTON_PRESS_MASK | Gdk.EventMask.SCROLL_MASK)
box.pack_start(canvas, True, True, 0)
entry = Gtk.Entry()
entry.set_placeholder_text('Type test text here')
entry.set_size_request(-1, 90)
box.pack_start(entry, False, False, 12)
label = Gtk.Label()
box.pack_start(label, False, False, 16)


def save():
    temp = state_path.with_suffix('.tmp')
    temp.write_text(json.dumps(state))
    temp.replace(state_path)


def draw(widget, ctx):
    width, height = widget.get_allocated_width(), widget.get_allocated_height()
    state['canvas_width'], state['canvas_height'] = width, height
    ctx.set_source_rgb(.07, .13, .22); ctx.paint()
    for yy in range(0, height, 80):
        for xx in range(0, width, 80):
            ctx.set_source_rgb(*((.12, .25, .36) if (xx//80+yy//80)%2 else (.09, .18, .28)))
            ctx.rectangle(xx, yy, 80, 80); ctx.fill()
    # A click flips the target colour, making input-to-video return observable.
    ctx.set_source_rgb(*((.12, .75, .32) if state['left'] % 2 else (.95, .38, .12)))
    ctx.rectangle(width*.2, height*.4, width*.6, height*.2); ctx.fill()
    ctx.set_source_rgb(1, 1, 1); ctx.select_font_face('Sans', 0, 1); ctx.set_font_size(30)
    ctx.move_to(width*.22, height*.5); ctx.show_text('CLICK / RIGHT CLICK')
    ctx.set_source_rgb(.15, .85, .55)
    ctx.arc((state['frame']*9)%max(1,width), height*.25, 35, 0, math.pi*2); ctx.fill()
    ctx.set_source_rgb(1,1,1); ctx.move_to(20,60); ctx.show_text('FRAME %06d' % state['frame'])
    return False


def button(_, event):
    if event.button == 1: state['left'] += 1
    if event.button == 3: state['right'] += 1
    state['last_root_x'] = round(event.x_root, 2); state['last_root_y'] = round(event.y_root, 2)
    save(); return False


def scroll(_, event):
    state['scroll'] += 1; save(); return False


def key(_, event):
    state['keys'] += 1
    if event.keyval == Gdk.KEY_F8: state['f8'] = True
    save(); return False


def changed(widget):
    state['text_matched'] = widget.get_text() == 'Kite 远程桌面 29'
    save()


def tick():
    if stopping or time.monotonic()-started > args.seconds:
        window.destroy(); return False
    state['frame'] += 1
    label.set_text('LEFT %d   RIGHT %d   SCROLL %d   KEYS %d   TEXT %s' % (state['left'],state['right'],state['scroll'],state['keys'],state['text_matched']))
    canvas.queue_draw()
    if state['frame']%10 == 0: save()
    return True


def stop(*_):
    state['alive'] = False; save(); Gtk.main_quit()


def request_stop(*_):
    global stopping
    stopping = True

canvas.connect('draw', draw); canvas.connect('button-press-event', button); canvas.connect('scroll-event', scroll)
window.connect('key-press-event', key); window.connect('destroy', stop); entry.connect('changed', changed)
signal.signal(signal.SIGTERM, request_stop); signal.signal(signal.SIGINT, request_stop)
window.show_all(); window.fullscreen(); window.present(); entry.grab_focus()
GLib.timeout_add(33, tick)
try:
    Gtk.main()
finally:
    if display:
        # The prior window may have closed while testing; suppress only X errors on restore.
        handler = C.CFUNCTYPE(C.c_int, C.c_void_p, C.c_void_p)(lambda *_: 0)
        x.XSetErrorHandler.argtypes = [C.c_void_p]
        x.XSetErrorHandler(C.cast(handler, C.c_void_p))
        x.XSetInputFocus(display, old_focus.value, old_revert.value, 0)
        x.XSync(display, 0); x.XCloseDisplay(display)
