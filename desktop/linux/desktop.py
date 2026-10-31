#!/usr/bin/env python3
"""Native status panel. Lifecycle, service policy and IPC live in the Rust application."""
from pathlib import Path
import argparse,json,os,subprocess,threading
import gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk,Gdk,GLib

ROOT=Path(__file__).resolve().parent
TEXT=json.loads((ROOT/'strings.json').read_text())
NODE=os.environ.get('KITE_NODE_BINARY',str(Path.home()/'.local/bin/kite-node'))

class Window(Gtk.Window):
    def __init__(self,snapshot=None):
        super().__init__(title=TEXT['title'])
        self.set_default_size(620,460);self.set_border_width(24)
        self.pending=False;self.snapshot=snapshot
        self.connect('destroy',Gtk.main_quit)
        box=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=14);self.add(box)
        title=Gtk.Label(label=TEXT['heading'],xalign=0);title.get_style_context().add_class('heading');box.pack_start(title,False,False,0)
        self.status=Gtk.Label(label=TEXT['busy'],xalign=0);box.pack_start(self.status,False,False,0)
        self.machine=Gtk.Label(xalign=0);self.machine.get_style_context().add_class('muted');box.pack_start(self.machine,False,False,0)
        self.services=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=10);box.pack_start(self.services,False,False,0)
        actions=Gtk.Box(spacing=10);box.pack_start(actions,False,False,4)
        self.ensure=Gtk.Button(label=TEXT['ensure']);self.ensure.connect('clicked',lambda _:self.call('ensure','all'));actions.pack_start(self.ensure,False,False,0)
        refresh=Gtk.Button(label=TEXT['refresh']);refresh.connect('clicked',lambda _:self.call('status'));actions.pack_start(refresh,False,False,0)
        sessions=Gtk.Button(label=TEXT['sessions']);sessions.connect('clicked',lambda _:self.call('sessions'));actions.pack_start(sessions,False,False,0)
        for key in ['routing','mcp','background']:
            label=Gtk.Label(label=TEXT[key],xalign=0);label.set_line_wrap(True);label.get_style_context().add_class('muted');box.pack_start(label,False,False,0)
        expander=Gtk.Expander(label=TEXT['details']);box.pack_start(expander,True,True,0)
        scroll=Gtk.ScrolledWindow();expander.add(scroll);self.details=Gtk.Label(xalign=0,yalign=0);self.details.set_selectable(True);self.details.set_line_wrap(True);scroll.add(self.details)
        close=Gtk.Button(label=TEXT['close']);close.connect('clicked',lambda _:self.destroy());box.pack_end(close,False,False,0)
        self.show_all();self.call('status');GLib.timeout_add_seconds(3,self.poll)
    def poll(self):
        self.call('status');return True
    def call(self,*args):
        if self.pending:return
        self.pending=True;self.ensure.set_sensitive(False)
        def run():
            try:
                p=subprocess.run([NODE,*args],capture_output=True,text=True,timeout=35)
                if p.returncode:raise RuntimeError(p.stderr.strip())
                value=json.loads(p.stdout);error=None
            except Exception as e:value=None;error=str(e)
            GLib.idle_add(self.result,args[0],value,error)
        threading.Thread(target=run,daemon=True).start()
    def result(self,kind,value,error):
        self.pending=False;self.ensure.set_sensitive(True)
        if error:
            self.status.set_text(TEXT['offline']);self.status.get_style_context().remove_class('success');self.details.set_text(error);self.machine.set_text(TEXT['unavailable']);return False
        if kind=='sessions':
            rows=value.get('sessions',[])
            dialog=Gtk.MessageDialog(transient_for=self,modal=True,message_type=Gtk.MessageType.INFO,buttons=Gtk.ButtonsType.CLOSE,text=TEXT['session_title'])
            dialog.format_secondary_text(str(len(rows))+' '+TEXT['session_count']+'\n\n'+'\n'.join(str(r.get('title') or r.get('id')) for r in rows[:30]));dialog.run();dialog.destroy();return False
        if kind=='ensure':self.call('status');return False
        self.status.set_text(TEXT['ready']);self.status.get_style_context().add_class('success')
        self.machine.set_text(value.get('machine','')+' · '+value.get('version',''))
        for child in self.services.get_children():self.services.remove(child)
        for service in value.get('services',[]):
            row=Gtk.Box(spacing=10);row.get_style_context().add_class('service')
            name=Gtk.Label(label=service.get('name',''),xalign=0);row.pack_start(name,True,True,0)
            state=Gtk.Label(label=TEXT['active' if service.get('active') else 'inactive']);state.get_style_context().add_class('success' if service.get('active') else 'muted');row.pack_start(state,False,False,0)
            ownership=Gtk.Label(label=TEXT['external' if service.get('ownership')=='external' else 'managed']);ownership.get_style_context().add_class('muted');row.pack_start(ownership,False,False,0)
            self.services.pack_start(row,False,False,0)
        self.services.show_all();self.details.set_text(json.dumps(value,ensure_ascii=False,indent=2))
        if self.snapshot:GLib.timeout_add(300,self.capture)
        return False
    def capture(self):
        if not self.snapshot:return False
        path=self.snapshot;self.snapshot=None
        image=Gdk.pixbuf_get_from_window(self.get_window(),0,0,self.get_allocated_width(),self.get_allocated_height())
        image.savev(path,'png',[],[]);return False

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--snapshot');parser.add_argument('--close-after',type=int);args=parser.parse_args()
    Gtk.Settings.get_default().set_property('gtk-application-prefer-dark-theme',True)
    css=Gtk.CssProvider();css.load_from_data(b'window{background:#111316;color:#e5e9ee} .heading{font-size:22px;font-weight:600} .muted{color:#9aa4b1;font-size:12px} .success{color:#a4d8b3} .service{background:#191c20;border-radius:12px;padding:14px} button{background:#23334a;color:#e5e9ee;border:1px solid #343b44;border-radius:8px;padding:8px 12px}')
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(),css,Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
    window=Window(args.snapshot)
    if args.close_after:GLib.timeout_add_seconds(args.close_after,lambda:(window.destroy(),False)[1])
    Gtk.main()
if __name__=='__main__':main()
