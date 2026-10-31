#!/usr/bin/env python3
"""Native X11 status UI. Service health and lifecycle policy belong to Kite Node."""
from pathlib import Path
import json
import os
import subprocess
import sys
import threading

import gi
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, Gdk, GdkPixbuf, Gio, GLib

ROOT = Path(__file__).resolve().parent
TEXT = json.loads((ROOT / 'strings.json').read_text())
NODE = os.environ.get('KITE_NODE_BINARY', str(Path.home() / '.local/bin/kite-node'))


class Window(Gtk.ApplicationWindow):
    def __init__(self, app):
        super().__init__(application=app, title=TEXT['title'])
        self.set_default_size(620, 460)
        self.set_border_width(24)
        self.set_icon_from_file(str(ROOT / 'icons/kite-ready.svg'))
        self.connect('delete-event', lambda *_: app.close_panel())
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14)
        self.add(box)
        title = Gtk.Label(label=TEXT['heading'], xalign=0)
        title.get_style_context().add_class('heading')
        box.pack_start(title, False, False, 0)
        self.status = Gtk.Label(xalign=0)
        box.pack_start(self.status, False, False, 0)
        self.machine = Gtk.Label(xalign=0)
        self.machine.get_style_context().add_class('muted')
        box.pack_start(self.machine, False, False, 0)
        self.services = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        box.pack_start(self.services, False, False, 0)
        actions = Gtk.Box(spacing=10)
        box.pack_start(actions, False, False, 4)
        self.ensure = Gtk.Button(label=TEXT['ensure'])
        self.ensure.connect('clicked', lambda _: app.call('ensure', 'all'))
        actions.pack_start(self.ensure, False, False, 0)
        refresh = Gtk.Button(label=TEXT['refresh'])
        refresh.connect('clicked', lambda _: app.call('status'))
        actions.pack_start(refresh, False, False, 0)
        sessions = Gtk.Button(label=TEXT['sessions'])
        sessions.connect('clicked', lambda _: app.call('sessions'))
        actions.pack_start(sessions, False, False, 0)
        self.hint = Gtk.Label(xalign=0)
        self.hint.set_line_wrap(True)
        self.hint.get_style_context().add_class('muted')
        box.pack_start(self.hint, False, False, 0)
        for key in ['routing', 'mcp']:
            label = Gtk.Label(label=TEXT[key], xalign=0)
            label.set_line_wrap(True)
            label.get_style_context().add_class('muted')
            box.pack_start(label, False, False, 0)
        expander = Gtk.Expander(label=TEXT['details'])
        box.pack_start(expander, True, True, 0)
        scroll = Gtk.ScrolledWindow()
        expander.add(scroll)
        self.details = Gtk.Label(xalign=0, yalign=0)
        self.details.set_selectable(True)
        self.details.set_line_wrap(True)
        scroll.add(self.details)
        close = Gtk.Button(label=TEXT['close'])
        close.connect('clicked', lambda _: app.close_panel())
        box.pack_end(close, False, False, 0)

    def render(self, app):
        self.status.set_text(TEXT[app.state])
        style = self.status.get_style_context()
        for name in ['success', 'warning', 'error']:
            style.remove_class(name)
        style.add_class({'ready': 'success', 'offline': 'error'}.get(app.state, 'warning'))
        value = app.value or {}
        self.machine.set_text(' · '.join(filter(None, [value.get('machine'), value.get('version')])))
        self.ensure.set_sensitive(not app.pending and app.value is not None)
        self.hint.set_text(TEXT['background' if app.embedded() else 'no_tray'])
        for child in self.services.get_children():
            self.services.remove(child)
        for service in value.get('services', []):
            row = Gtk.Box(spacing=10)
            row.get_style_context().add_class('service')
            row.pack_start(Gtk.Label(label=service['name'], xalign=0), True, True, 0)
            state = Gtk.Label(label=TEXT['active' if service['active'] else 'inactive'])
            state.get_style_context().add_class('success' if service['active'] else 'warning')
            row.pack_start(state, False, False, 0)
            self.services.pack_start(row, False, False, 0)
        self.services.show_all()
        self.details.set_text(app.error or json.dumps(value, ensure_ascii=False, indent=2))


class Application(Gtk.Application):
    def __init__(self):
        super().__init__(application_id='io.github.wenshiqi0.Kite.Host',
                         flags=Gio.ApplicationFlags.HANDLES_COMMAND_LINE)
        for name, kind in [('background', GLib.OptionArg.NONE), ('quit', GLib.OptionArg.NONE),
                           ('snapshot', GLib.OptionArg.FILENAME), ('close-after', GLib.OptionArg.INT)]:
            self.add_main_option(name, 0, GLib.OptionFlags.NONE, kind, name, None)
        self.window = None
        self.value = None
        self.state = 'busy'
        self.error = None
        self.pending = False
        self.exiting = False
        self.snapshot = None
        self.icon_state = None

    def do_startup(self):
        Gtk.Application.do_startup(self)
        self.hold()
        Gtk.Settings.get_default().set_property('gtk-application-prefer-dark-theme', True)
        css = Gtk.CssProvider()
        css.load_from_data(b'window{background:#111316;color:#e5e9ee} .heading{font-size:22px;font-weight:600} .muted{color:#9aa4b1;font-size:12px} .success{color:#a4d8b3} .warning{color:#f1ca78} .error{color:#f09898} .service{background:#191c20;border-radius:12px;padding:14px} button{background:#23334a;color:#e5e9ee;border:1px solid #343b44;border-radius:8px;padding:8px 12px}')
        Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        # This host currently supports X11; an unavailable tray never strands the UI.
        self.tray = Gtk.StatusIcon()
        self.tray.set_title(TEXT['tray_title'])
        self.tray.set_name('kite-host')
        self.tray.connect('activate', lambda _: self.present_window())
        self.tray.connect('popup-menu', self.popup)
        self.tray.connect('size-changed', lambda _, size: self.resize_icon(size))
        self.tray.connect('notify::embedded', self.tray_changed)
        self.menu = Gtk.Menu()
        self.status_items = [Gtk.MenuItem(), Gtk.MenuItem()]
        for item in self.status_items:
            item.set_sensitive(False)
            self.menu.append(item)
        self.service_items = []
        self.service_names = ()
        self.menu.append(Gtk.SeparatorMenuItem())
        self.actions = {}
        for key, callback in [('open', self.present_window), ('ensure', lambda: self.call('ensure', 'all')),
                              ('refresh', lambda: self.call('status')), ('quit_tray', self.quit)]:
            item = Gtk.MenuItem(label=TEXT[key])
            item.connect('activate', lambda _, fn=callback: fn())
            self.menu.append(item)
            self.actions[key] = item
        self.render()
        self.tray.set_visible(True)
        self.poll_source = GLib.timeout_add_seconds(3, self.poll)
        self.call('status')
        for name, callback in [('show', self.present_window), ('refresh', lambda: self.call('status')),
                               ('hide', self.close_panel), ('quit', self.quit)]:
            action = Gio.SimpleAction.new(name, None)
            action.connect('activate', lambda _, __, fn=callback: fn())
            self.add_action(action)

    def do_command_line(self, command_line):
        options = command_line.get_options_dict().end().unpack()
        if options.get('quit'):
            self.quit()
            return 0
        if options.get('snapshot'):
            path = options['snapshot']
            self.snapshot = os.fsdecode(bytes(path).rstrip(b'\0'))
        if not options.get('background'):
            self.present_window()
        else:
            GLib.timeout_add(1500, self.check_tray)
        if options.get('close-after'):
            GLib.timeout_add_seconds(options['close-after'], lambda: (self.quit(), False)[1])
        return 0

    def do_activate(self):
        self.present_window()

    def embedded(self):
        return self.tray.is_embedded()

    def check_tray(self):
        if self.exiting:
            return False
        if not self.embedded():
            self.present_window()
        return False

    def tray_changed(self, *_):
        if self.exiting:
            return
        if not self.embedded():
            GLib.timeout_add(1500, self.check_tray)
        if self.window:
            self.window.render(self)

    def present_window(self):
        if self.window is None:
            self.window = Window(self)
        self.window.render(self)
        self.window.show_all()
        self.window.present()

    def close_panel(self):
        if self.embedded():
            if self.window:
                self.window.hide()
        else:
            self.quit()
        return True

    def resize_icon(self, size):
        self.tray.set_from_pixbuf(GdkPixbuf.Pixbuf.new_from_file_at_scale(
            str(ROOT / ('icons/kite-' + self.state + '.svg')), max(16, size), max(16, size), True))
        return True

    def render(self):
        if self.icon_state != self.state:
            self.icon_state = self.state
            self.resize_icon(max(16, self.tray.get_size()))
        node_state = 'active' if self.value else 'offline' if self.state == 'offline' else 'busy'
        lines = [TEXT[self.state], TEXT['node'] + ' · ' + TEXT[node_state]]
        for service in (self.value or {}).get('services', []):
            lines.append(service['name'] + ' · ' + TEXT['active' if service['active'] else 'inactive'])
        self.tray.set_tooltip_text('Kite\n' + '\n'.join(lines))
        for item, line in zip(self.status_items, lines[:2]):
            item.set_label(line)
        names = tuple(service['name'] for service in (self.value or {}).get('services', []))
        if names != self.service_names:
            for item in self.service_items:
                self.menu.remove(item)
            self.service_items = []
            self.service_names = names
            for index, name in enumerate(names):
                item = Gtk.MenuItem()
                item.set_sensitive(False)
                self.menu.insert(item, index + 2)
                self.service_items.append(item)
        for item, line in zip(self.service_items, lines[2:]):
            item.set_label(line)
        self.actions['ensure'].set_sensitive(not self.pending and self.value is not None)
        self.menu.show_all()
        if self.window:
            self.window.render(self)

    def popup(self, _, button, time):
        self.menu.popup(None, None, Gtk.StatusIcon.position_menu, self.tray, button, time)

    def poll(self):
        self.call('status')
        return True

    def call(self, *args):
        if self.pending or self.exiting:
            return
        self.pending = True
        if args[0] != 'status':
            self.state = 'busy'
        self.render()

        def run():
            try:
                p = subprocess.run([NODE, *args], capture_output=True, text=True,
                                   timeout=3 if args[0] == 'status' else 35)
                if p.returncode:
                    raise RuntimeError(p.stderr.strip())
                value, error = json.loads(p.stdout), None
            except Exception as e:
                value, error = None, str(e)
            GLib.idle_add(self.result, args[0], value, error)
        threading.Thread(target=run, daemon=True).start()

    def result(self, kind, value, error):
        if self.exiting:
            return False
        self.pending = False
        if kind == 'status':
            try:
                if error:
                    raise RuntimeError(error)
                if value['health'] not in ('ready', 'degraded'):
                    raise ValueError('Invalid host health')
                self.value, self.state, self.error = value, value['health'], None
            except Exception as e:
                self.value, self.state, self.error = None, 'offline', str(e)
            self.render()
            if self.snapshot and self.window:
                GLib.timeout_add(300, self.capture)
        else:
            if error:
                self.error = error
                self.present_window()
            elif kind == 'sessions':
                self.present_window()
                rows = value.get('sessions', [])
                dialog = Gtk.MessageDialog(transient_for=self.window, modal=True, message_type=Gtk.MessageType.INFO,
                                           buttons=Gtk.ButtonsType.CLOSE, text=TEXT['session_title'])
                dialog.format_secondary_text(str(len(rows)) + ' ' + TEXT['session_count'] + '\n\n' +
                                             '\n'.join(str(r.get('title') or r.get('id')) for r in rows[:30]))
                dialog.run()
                dialog.destroy()
            self.call('status')
        return False

    def capture(self):
        if self.snapshot and self.window and self.window.get_visible():
            path, self.snapshot = self.snapshot, None
            image = Gdk.pixbuf_get_from_window(self.window.get_window(), 0, 0,
                                              self.window.get_allocated_width(), self.window.get_allocated_height())
            image.savev(path, 'png', [], [])
        return False

    def do_shutdown(self):
        self.exiting = True
        GLib.source_remove(self.poll_source)
        self.tray.set_visible(False)
        Gtk.Application.do_shutdown(self)


if __name__ == '__main__':
    sys.exit(Application().run(sys.argv))
