//! Small typed bindings to the system GTK 3 ABI. No web view or script runtime.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::state::Model;
use libloading::Library;
use serde_json::Value;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_uint, c_void},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    ptr,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
type P = *mut c_void;
macro_rules! bindings {
    ($($name:ident($($arg:ty),*) -> $ret:ty;)*) => {
        struct Gtk { _library: Library, $( $name: unsafe extern "C" fn($($arg),*) -> $ret, )* }
        impl Gtk { unsafe fn open() -> Result<Self,String> {
            let library=Library::new("libgtk-3.so.0").map_err(|e|e.to_string())?;
            $(let $name=*library.get::<unsafe extern "C" fn($($arg),*) -> $ret>(concat!(stringify!($name),"\0").as_bytes()).map_err(|e|e.to_string())?;)*
            Ok(Self{_library:library,$($name,)*})
        }}
    }
}
bindings! {
    gtk_application_new(*const c_char,c_int)->P;
    g_application_run(P,c_int,*mut *mut c_char)->c_int;
    g_application_hold(P)->(); g_application_quit(P)->();
    g_application_add_main_option(P,*const c_char,c_char,c_int,c_int,*const c_char,*const c_char)->();
    g_application_command_line_get_options_dict(P)->P;
    g_variant_dict_lookup_value(P,*const c_char,P)->P;
    g_variant_get_boolean(P)->c_int;g_variant_get_int32(P)->c_int;g_variant_get_bytestring(P)->*const c_char;g_variant_unref(P)->();
    g_signal_connect_data(P,*const c_char,*const (),P,P,c_int)->u64;
    g_object_unref(P)->();
    g_timeout_add(c_uint,unsafe extern "C" fn(P)->c_int,P)->c_uint;
    g_simple_action_new(*const c_char,P)->P;g_action_map_add_action(P,P)->();g_action_get_name(P)->*const c_char;
    gtk_application_window_new(P)->P;gtk_window_set_title(P,*const c_char)->();gtk_window_set_default_size(P,c_int,c_int)->();gtk_window_present(P)->();gtk_window_set_icon_from_file(P,*const c_char,*mut P)->c_int;
    gtk_container_set_border_width(P,c_uint)->();gtk_container_add(P,P)->();
    gtk_box_new(c_int,c_int)->P;gtk_box_pack_start(P,P,c_int,c_int,c_uint)->();gtk_box_pack_end(P,P,c_int,c_int,c_uint)->();
    gtk_label_new(*const c_char)->P;gtk_label_set_text(P,*const c_char)->();gtk_label_set_xalign(P,f32)->();gtk_label_set_yalign(P,f32)->();gtk_label_set_line_wrap(P,c_int)->();gtk_label_set_selectable(P,c_int)->();
    gtk_button_new_with_label(*const c_char)->P;gtk_widget_set_sensitive(P,c_int)->();gtk_widget_show_all(P)->();gtk_widget_hide(P)->();gtk_widget_show(P)->();gtk_widget_set_visible(P,c_int)->();gtk_widget_set_no_show_all(P,c_int)->();gtk_widget_get_visible(P)->c_int;gtk_widget_destroy(P)->();
    gtk_widget_get_style_context(P)->P;gtk_style_context_add_class(P,*const c_char)->();gtk_style_context_remove_class(P,*const c_char)->();
    gtk_dialog_new()->P;gtk_dialog_get_content_area(P)->P;gtk_dialog_add_button(P,*const c_char,c_int)->P;gtk_window_set_transient_for(P,P)->();gtk_window_set_modal(P,c_int)->();
    gtk_expander_new(*const c_char)->P;gtk_scrolled_window_new(P,P)->P;
    gtk_status_icon_new()->P;gtk_status_icon_set_title(P,*const c_char)->();gtk_status_icon_set_name(P,*const c_char)->();gtk_status_icon_set_visible(P,c_int)->();gtk_status_icon_set_from_pixbuf(P,P)->();gtk_status_icon_get_size(P)->c_int;gtk_status_icon_is_embedded(P)->c_int;gtk_status_icon_set_tooltip_text(P,*const c_char)->();
    gdk_pixbuf_new_from_file_at_scale(*const c_char,c_int,c_int,c_int,*mut P)->P;
    gtk_menu_new()->P;gtk_menu_shell_append(P,P)->();gtk_menu_shell_insert(P,P,c_int)->();gtk_menu_item_set_label(P,*const c_char)->();gtk_menu_item_new_with_label(*const c_char)->P;gtk_separator_menu_item_new()->P;
    gtk_menu_popup_at_pointer(P,P)->();
    gtk_css_provider_new()->P;gtk_css_provider_load_from_data(P,*const c_char,isize,*mut P)->c_int;gdk_screen_get_default()->P;gtk_style_context_add_provider_for_screen(P,P,c_uint)->();
    gtk_widget_get_window(P)->P;gtk_widget_get_allocated_width(P)->c_int;gtk_widget_get_allocated_height(P)->c_int;gdk_pixbuf_get_from_window(P,c_int,c_int,c_int,c_int)->P;gdk_pixbuf_savev(P,*const c_char,*const c_char,*mut *mut c_char,*mut *mut c_char,*mut P)->c_int;
}
struct Completion {
    args: Vec<String>,
    value: Result<Value, String>,
    timeout: bool,
}
struct App {
    gtk: Gtk,
    app: P,
    window: P,
    tray: P,
    menu: P,
    status_items: [P; 2],
    service_items: Vec<P>,
    status: P,
    machine: P,
    services: P,
    hint: P,
    activity: P,
    error: P,
    error_box: P,
    error_dismiss: P,
    service_rows: Vec<P>,
    details: P,
    ensure: P,
    menu_ensure: P,
    model: Model,
    text: Value,
    assets: PathBuf,
    node: PathBuf,
    launcher: PathBuf,
    original_launcher: Option<Vec<u8>>,
    sender: Sender<Completion>,
    receiver: Receiver<Completion>,
    last_poll: Instant,
    exiting: bool,
    snapshot: Option<PathBuf>,
    capture_after: Option<Instant>,
    close_after: Option<Instant>,
}
fn c(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap()
}
impl App {
    fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.text[key].as_str().unwrap_or(key)
    }
    unsafe fn connect(&self, target: P, name: &str, callback: *const (), data: P) {
        (self.gtk.g_signal_connect_data)(
            target,
            c(name).as_ptr(),
            callback,
            data,
            ptr::null_mut(),
            0,
        );
    }
    unsafe fn label(&self, text: &str) -> P {
        let w = (self.gtk.gtk_label_new)(c(text).as_ptr());
        (self.gtk.gtk_label_set_xalign)(w, 0.0);
        (self.gtk.gtk_label_set_line_wrap)(w, 1);
        w
    }
    unsafe fn pack(&self, container: P, widget: P) {
        (self.gtk.gtk_box_pack_start)(container, widget, 0, 0, 0);
    }
    unsafe fn styled(&self, w: P, class: &str) {
        (self.gtk.gtk_style_context_add_class)(
            (self.gtk.gtk_widget_get_style_context)(w),
            c(class).as_ptr(),
        );
    }
    unsafe fn action(&self, container: P, key: &str, callback: *const (), data: P) -> P {
        let w = (self.gtk.gtk_button_new_with_label)(c(self.t(key)).as_ptr());
        self.connect(w, "clicked", callback, data);
        self.pack(container, w);
        w
    }
    unsafe fn embedded(&self) -> bool {
        !self.tray.is_null() && (self.gtk.gtk_status_icon_is_embedded)(self.tray) != 0
    }
    unsafe fn visible(&self) -> bool {
        !self.window.is_null() && (self.gtk.gtk_widget_get_visible)(self.window) != 0
    }
    unsafe fn build(&mut self, data: P) {
        let g = &self.gtk;
        self.window = (g.gtk_application_window_new)(self.app);
        (g.gtk_window_set_title)(self.window, c(self.t("title")).as_ptr());
        (g.gtk_window_set_default_size)(self.window, 620, 460);
        (g.gtk_container_set_border_width)(self.window, 24);
        (g.gtk_window_set_icon_from_file)(
            self.window,
            c(&self.assets.join("icons/kite-ready.png").to_string_lossy()).as_ptr(),
            ptr::null_mut(),
        );
        self.connect(self.window, "delete-event", close_event as *const (), data);
        let box_ = (g.gtk_box_new)(1, 14);
        (g.gtk_container_add)(self.window, box_);
        let heading = self.label(self.t("heading"));
        self.styled(heading, "heading");
        self.pack(box_, heading);
        self.status = self.label("");
        self.pack(box_, self.status);
        self.machine = self.label("");
        self.styled(self.machine, "muted");
        self.pack(box_, self.machine);
        self.services = (g.gtk_box_new)(1, 10);
        self.pack(box_, self.services);
        let actions = (g.gtk_box_new)(0, 10);
        self.pack(box_, actions);
        self.ensure = self.action(actions, "ensure", ensure_click as *const (), data);
        self.action(actions, "refresh", refresh_click as *const (), data);
        self.action(actions, "sessions", sessions_click as *const (), data);
        self.hint = self.label("");
        self.styled(self.hint, "muted");
        self.pack(box_, self.hint);
        self.activity = self.label("");
        self.pack(box_, self.activity);
        let errors = (g.gtk_box_new)(0, 10);
        self.error_box = errors;
        (g.gtk_widget_set_no_show_all)(errors, 1);
        self.error = self.label("");
        self.styled(self.error, "error");
        self.pack(errors, self.error);
        self.error_dismiss = self.action(errors, "dismiss", dismiss_click as *const (), data);
        self.pack(box_, errors);
        for key in ["routing", "mcp"] {
            let label = self.label(self.t(key));
            self.styled(label, "muted");
            self.pack(box_, label);
        }
        let expander = (g.gtk_expander_new)(c(self.t("details")).as_ptr());
        (g.gtk_box_pack_start)(box_, expander, 1, 1, 0);
        let scroll = (g.gtk_scrolled_window_new)(ptr::null_mut(), ptr::null_mut());
        (g.gtk_container_add)(expander, scroll);
        self.details = self.label("");
        (g.gtk_label_set_yalign)(self.details, 0.0);
        (g.gtk_label_set_selectable)(self.details, 1);
        (g.gtk_container_add)(scroll, self.details);
        let close = (g.gtk_button_new_with_label)(c(self.t("close")).as_ptr());
        self.connect(close, "clicked", close_click as *const (), data);
        (g.gtk_box_pack_end)(box_, close, 0, 0, 0);
    }
    unsafe fn present(&mut self, data: P) {
        if self.window.is_null() {
            self.build(data);
        }
        self.render();
        (self.gtk.gtk_widget_show_all)(self.window);
        (self.gtk.gtk_window_present)(self.window);
    }
    unsafe fn close(&mut self) {
        if self.embedded() {
            if !self.window.is_null() {
                (self.gtk.gtk_widget_hide)(self.window);
            }
            self.check_upgrade();
        } else {
            self.exiting = true;
            (self.gtk.g_application_quit)(self.app);
        }
    }
    unsafe fn startup(&mut self, data: P) {
        (self.gtk.g_application_hold)(self.app);
        let css = (self.gtk.gtk_css_provider_new)();
        let style = "window{background:#111316;color:#e5e9ee}.heading{font-size:22px;font-weight:600}.muted{color:#9aa4b1;font-size:12px}.success{color:#a4d8b3}.warning{color:#f1ca78}.error{color:#f09898}.service{background:#191c20;border-radius:12px;padding:14px}button{background:#23334a;color:#e5e9ee;border:1px solid #343b44;border-radius:8px;padding:8px 12px}";
        (self.gtk.gtk_css_provider_load_from_data)(css, c(style).as_ptr(), -1, ptr::null_mut());
        (self.gtk.gtk_style_context_add_provider_for_screen)(
            (self.gtk.gdk_screen_get_default)(),
            css,
            600,
        );
        (self.gtk.g_object_unref)(css);
        self.tray = (self.gtk.gtk_status_icon_new)();
        (self.gtk.gtk_status_icon_set_title)(self.tray, c(self.t("tray_title")).as_ptr());
        (self.gtk.gtk_status_icon_set_name)(self.tray, c("kite-host").as_ptr());
        self.connect(self.tray, "activate", show_click as *const (), data);
        self.connect(self.tray, "popup-menu", popup as *const (), data);
        self.menu = (self.gtk.gtk_menu_new)();
        for i in 0..2 {
            let item = (self.gtk.gtk_menu_item_new_with_label)(c("").as_ptr());
            (self.gtk.gtk_widget_set_sensitive)(item, 0);
            (self.gtk.gtk_menu_shell_append)(self.menu, item);
            self.status_items[i] = item;
        }
        (self.gtk.gtk_menu_shell_append)(self.menu, (self.gtk.gtk_separator_menu_item_new)());
        for (key, callback) in [
            ("open", show_click as *const ()),
            ("ensure", ensure_click as *const ()),
            ("refresh", refresh_click as *const ()),
            ("quit_tray", quit_click as *const ()),
        ] {
            let item = (self.gtk.gtk_menu_item_new_with_label)(c(self.t(key)).as_ptr());
            self.connect(item, "activate", callback, data);
            (self.gtk.gtk_menu_shell_append)(self.menu, item);
            if key == "ensure" {
                self.menu_ensure = item;
            }
        }
        for name in ["show", "refresh", "hide", "quit", "upgrade"] {
            let action = (self.gtk.g_simple_action_new)(c(name).as_ptr(), ptr::null_mut());
            self.connect(action, "activate", gio_action as *const (), data);
            (self.gtk.g_action_map_add_action)(self.app, action);
            (self.gtk.g_object_unref)(action);
        }
        (self.gtk.gtk_status_icon_set_visible)(self.tray, 1);
        (self.gtk.g_timeout_add)(100, tick, data);
        self.request_status(false);
        self.render();
    }
    fn spawn(&self, args: Vec<String>) {
        let sender = self.sender.clone();
        let node = self.node.clone();
        thread::spawn(move || {
            let timeout = if args.first().is_some_and(|s| s == "status") {
                Duration::from_secs(6)
            } else {
                Duration::from_secs(35)
            };
            let (value, timed_out) = command(&node, &args, timeout);
            let _ = sender.send(Completion {
                args,
                value,
                timeout: timed_out,
            });
        });
    }
    fn request_status(&mut self, refresh: bool) {
        if self.model.status_pending {
            self.model.refresh_requested |= refresh;
            return;
        }
        self.model.status_pending = true;
        self.spawn(if refresh {
            vec!["status".into(), "--refresh".into()]
        } else {
            vec!["status".into()]
        });
    }
    unsafe fn call(&mut self, args: Vec<String>) {
        self.model.enqueue(args);
        self.next_action();
    }
    unsafe fn next_action(&mut self) {
        if let Some(args) = self.model.next() {
            self.spawn(args);
        }
        self.render();
    }
    unsafe fn render(&mut self) {
        if self.tray.is_null() {
            return;
        }
        let health = if self.model.health.is_empty() {
            "busy"
        } else {
            self.model.health
        };
        let mut lines = vec![
            self.t(health).to_owned(),
            format!(
                "{} · {}",
                self.t("node"),
                self.t(if self.model.value.is_some() {
                    "active"
                } else {
                    health
                })
            ),
        ];
        let services = self
            .model
            .value
            .as_ref()
            .and_then(|v| v["services"].as_array());
        if let Some(services) = services {
            for service in services {
                lines.push(format!(
                    "{} · {}",
                    service["name"].as_str().unwrap_or(""),
                    self.t(if service["error"].is_string() {
                        "unknown"
                    } else if service["active"] == true {
                        "active"
                    } else {
                        "inactive"
                    })
                ));
            }
        }
        (self.gtk.gtk_status_icon_set_tooltip_text)(
            self.tray,
            c(&format!("Kite\n{}", lines.join("\n"))).as_ptr(),
        );
        for (item, line) in self.status_items.iter().zip(lines.iter()) {
            // GtkMenuItem is a GtkBin with a label child.
            (self.gtk.gtk_menu_item_set_label)(*item, c(line).as_ptr());
        }
        let service_lines = lines.iter().skip(2).collect::<Vec<_>>();
        if self.service_items.len() != service_lines.len() {
            for item in self.service_items.drain(..) {
                (self.gtk.gtk_widget_destroy)(item);
            }
            for (i, _) in service_lines.iter().enumerate() {
                let item = (self.gtk.gtk_menu_item_new_with_label)(c("").as_ptr());
                (self.gtk.gtk_widget_set_sensitive)(item, 0);
                (self.gtk.gtk_menu_shell_insert)(self.menu, item, (i + 2) as c_int);
                self.service_items.push(item);
            }
        }
        for (item, line) in self.service_items.iter().zip(service_lines) {
            (self.gtk.gtk_menu_item_set_label)(*item, c(line).as_ptr());
        }
        let state = if self.model.active.is_some() {
            "busy"
        } else if health == "unresponsive" {
            "degraded"
        } else {
            health
        };
        let size = (self.gtk.gtk_status_icon_get_size)(self.tray).max(16);
        let image = (self.gtk.gdk_pixbuf_new_from_file_at_scale)(
            c(&self
                .assets
                .join(format!("icons/kite-{state}.png"))
                .to_string_lossy())
            .as_ptr(),
            size,
            size,
            1,
            ptr::null_mut(),
        );
        if !image.is_null() {
            (self.gtk.gtk_status_icon_set_from_pixbuf)(self.tray, image);
            (self.gtk.g_object_unref)(image);
        }
        (self.gtk.gtk_widget_set_sensitive)(
            self.menu_ensure,
            (self.model.active.is_none() && self.model.value.is_some()) as c_int,
        );
        (self.gtk.gtk_widget_show_all)(self.menu);
        if self.window.is_null() {
            return;
        }
        (self.gtk.gtk_label_set_text)(self.status, c(self.t(health)).as_ptr());
        let style = (self.gtk.gtk_widget_get_style_context)(self.status);
        for class in ["success", "warning", "error"] {
            (self.gtk.gtk_style_context_remove_class)(style, c(class).as_ptr());
        }
        (self.gtk.gtk_style_context_add_class)(
            style,
            c(if health == "ready" {
                "success"
            } else if health == "offline" {
                "error"
            } else {
                "warning"
            })
            .as_ptr(),
        );
        let v = self.model.value.as_ref();
        let machine = v
            .map(|v| {
                format!(
                    "{} · {}",
                    v["machine"].as_str().unwrap_or(""),
                    v["version"].as_str().unwrap_or("")
                )
            })
            .unwrap_or_default();
        (self.gtk.gtk_label_set_text)(self.machine, c(&machine).as_ptr());
        let service_lines = lines.iter().skip(2).collect::<Vec<_>>();
        if self.service_rows.len() != service_lines.len() {
            for row in self.service_rows.drain(..) {
                (self.gtk.gtk_widget_destroy)(row);
            }
            for text in &service_lines {
                let row = self.label(text);
                self.styled(row, "service");
                self.pack(self.services, row);
                self.service_rows.push(row);
            }
        }
        for (row, text) in self.service_rows.iter().zip(service_lines) {
            (self.gtk.gtk_label_set_text)(*row, c(text).as_ptr());
        }
        (self.gtk.gtk_widget_show_all)(self.services);
        (self.gtk.gtk_widget_set_visible)(
            self.error_box,
            self.model.action_error.is_some() as c_int,
        );
        if self.model.action_error.is_some() {
            (self.gtk.gtk_widget_show)(self.error);
            (self.gtk.gtk_widget_show)(self.error_dismiss);
        }
        let hint = format!(
            "{}{}",
            self.t(if self.embedded() {
                "background"
            } else {
                "no_tray"
            }),
            if self.model.update_pending {
                format!("\n{}", self.t("update_pending"))
            } else {
                String::new()
            }
        );
        (self.gtk.gtk_label_set_text)(self.hint, c(&hint).as_ptr());
        (self.gtk.gtk_label_set_text)(
            self.activity,
            c(if !self.model.queue.is_empty() {
                self.t("queued")
            } else if self.model.active.is_some() {
                self.t("busy")
            } else {
                ""
            })
            .as_ptr(),
        );
        (self.gtk.gtk_label_set_text)(
            self.error,
            c(self.model.action_error.as_deref().unwrap_or("")).as_ptr(),
        );
        (self.gtk.gtk_widget_set_sensitive)(
            self.ensure,
            (self.model.active.is_none() && v.is_some()) as c_int,
        );
        let details = self.model.error.clone().unwrap_or_else(|| {
            v.map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                .unwrap_or_default()
        });
        (self.gtk.gtk_label_set_text)(self.details, c(&details).as_ptr());
    }
    unsafe fn check_upgrade(&mut self) {
        if self
            .original_launcher
            .as_ref()
            .is_some_and(|before| std::fs::read(&self.launcher).ok().as_ref() != Some(before))
        {
            self.model.update_pending = true;
        }
        if self.model.can_upgrade(self.embedded(), self.visible()) {
            use std::os::unix::process::CommandExt;
            let error = Command::new(&self.launcher).arg("--background").exec();
            self.model.action_error = Some(error.to_string());
        }
    }
    unsafe fn capture(&mut self) {
        let Some(path) = self.snapshot.take() else {
            return;
        };
        if !self.visible() {
            return;
        }
        let image = (self.gtk.gdk_pixbuf_get_from_window)(
            (self.gtk.gtk_widget_get_window)(self.window),
            0,
            0,
            (self.gtk.gtk_widget_get_allocated_width)(self.window),
            (self.gtk.gtk_widget_get_allocated_height)(self.window),
        );
        if !image.is_null() {
            let pending = path.with_extension(format!("{}.pending", std::process::id()));
            let saved = (self.gtk.gdk_pixbuf_savev)(
                image,
                c(&pending.to_string_lossy()).as_ptr(),
                c("png").as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            (self.gtk.g_object_unref)(image);
            if saved == 0 || std::fs::rename(&pending, &path).is_err() {
                let _ = std::fs::remove_file(pending);
                eprintln!("native snapshot could not be saved");
            }
        }
    }
}
fn command(node: &Path, args: &[String], timeout: Duration) -> (Result<Value, String>, bool) {
    let result = (|| {
        let mut child = Command::new(node)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out = thread::spawn(move || {
            let mut b = Vec::new();
            let _ = stdout.take(4 * 1024 * 1024 + 1).read_to_end(&mut b);
            b
        });
        let err = thread::spawn(move || {
            let mut b = Vec::new();
            let _ = stderr.take(65537).read_to_end(&mut b);
            b
        });
        let deadline = Instant::now() + timeout;
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                _ => {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait().map_err(|e| e.to_string())?;
                }
            }
        };
        let output = out.join().unwrap_or_default();
        let errors = err.join().unwrap_or_default();
        let value = if timed_out {
            Err("Node command timed out".into())
        } else if output.len() > 4 * 1024 * 1024 {
            Err("Node response exceeds limit".into())
        } else if !status.success() {
            Err(String::from_utf8_lossy(&errors[..errors.len().min(4096)])
                .trim()
                .to_string())
        } else {
            serde_json::from_slice(&output).map_err(|e| e.to_string())
        };
        Ok((value, timed_out))
    })();
    result.unwrap_or_else(|e| (Err(e), false))
}
unsafe extern "C" fn startup(_: P, data: P) {
    (&mut *(data as *mut App)).startup(data);
}
unsafe extern "C" fn activate(_: P, data: P) {
    (&mut *(data as *mut App)).present(data);
}
unsafe extern "C" fn command_line(_: P, line: P, data: P) -> c_int {
    let app = &mut *(data as *mut App);
    let options = (app.gtk.g_application_command_line_get_options_dict)(line);
    let lookup = |name: &str| {
        (app.gtk.g_variant_dict_lookup_value)(options, c(name).as_ptr(), ptr::null_mut())
    };
    let flag = |name: &str| {
        let value = lookup(name);
        if value.is_null() {
            false
        } else {
            let result = (app.gtk.g_variant_get_boolean)(value) != 0;
            (app.gtk.g_variant_unref)(value);
            result
        }
    };
    if flag("quit") {
        app.exiting = true;
        (app.gtk.g_application_quit)(app.app);
        return 0;
    }
    let snapshot = lookup("snapshot");
    if !snapshot.is_null() {
        app.snapshot = Some(PathBuf::from(
            CStr::from_ptr((app.gtk.g_variant_get_bytestring)(snapshot))
                .to_string_lossy()
                .into_owned(),
        ));
        (app.gtk.g_variant_unref)(snapshot);
    }
    let close_after = lookup("close-after");
    if !close_after.is_null() {
        let seconds = (app.gtk.g_variant_get_int32)(close_after);
        if seconds > 0 {
            app.close_after = Some(Instant::now() + Duration::from_secs(seconds as u64));
        }
        (app.gtk.g_variant_unref)(close_after);
    }
    if !flag("background") {
        app.present(data);
    }
    0
}
unsafe extern "C" fn show_click(_: P, data: P) {
    (&mut *(data as *mut App)).present(data);
}
unsafe extern "C" fn close_click(_: P, data: P) {
    (&mut *(data as *mut App)).close();
}
unsafe extern "C" fn close_event(_: P, _: P, data: P) -> c_int {
    (&mut *(data as *mut App)).close();
    1
}
unsafe extern "C" fn ensure_click(_: P, data: P) {
    (&mut *(data as *mut App)).call(vec!["ensure".into(), "all".into()]);
}
unsafe extern "C" fn refresh_click(_: P, data: P) {
    (&mut *(data as *mut App)).request_status(true);
}
unsafe extern "C" fn sessions_click(_: P, data: P) {
    (&mut *(data as *mut App)).call(vec!["sessions".into()]);
}
unsafe extern "C" fn dismiss_click(_: P, data: P) {
    let app = &mut *(data as *mut App);
    app.model.action_error = None;
    app.model.failed_action = None;
    app.render();
}
unsafe extern "C" fn quit_click(_: P, data: P) {
    let app = &mut *(data as *mut App);
    app.exiting = true;
    (app.gtk.g_application_quit)(app.app);
}
unsafe extern "C" fn popup(_: P, _: c_uint, _: c_uint, data: P) {
    let app = &*(data as *mut App);
    (app.gtk.gtk_menu_popup_at_pointer)(app.menu, ptr::null_mut());
}
unsafe extern "C" fn gio_action(action: P, _: P, data: P) {
    let app = &mut *(data as *mut App);
    let name = CStr::from_ptr((app.gtk.g_action_get_name)(action)).to_string_lossy();
    match name.as_ref() {
        "show" => app.present(data),
        "refresh" => app.request_status(true),
        "hide" => app.close(),
        "quit" => {
            app.exiting = true;
            (app.gtk.g_application_quit)(app.app);
        }
        "upgrade" => app.check_upgrade(),
        _ => {}
    }
}
unsafe extern "C" fn destroy_dialog(dialog: P, _: c_int, data: P) {
    let app = &*(data as *mut App);
    (app.gtk.gtk_widget_destroy)(dialog);
}
unsafe extern "C" fn tick(data: P) -> c_int {
    let app = &mut *(data as *mut App);
    if app.exiting {
        return 0;
    }
    while let Ok(result) = app.receiver.try_recv() {
        if result.args.first().is_some_and(|s| s == "status") {
            app.model.status(result.value, result.timeout);
            if app.snapshot.is_some() {
                app.capture_after = Some(Instant::now() + Duration::from_millis(300));
            }
            if app.model.refresh_requested {
                app.model.refresh_requested = false;
                app.request_status(true);
            }
        } else {
            if result.args.first().is_some_and(|s| s == "sessions")
                && let Ok(ref value) = result.value
            {
                app.present(data);
                let titles = value["sessions"]
                    .as_array()
                    .map(|rows| {
                        rows.iter()
                            .take(30)
                            .map(|r| r["title"].as_str().or(r["id"].as_str()).unwrap_or(""))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                let dialog = (app.gtk.gtk_dialog_new)();
                (app.gtk.gtk_window_set_title)(dialog, c(app.t("session_title")).as_ptr());
                (app.gtk.gtk_window_set_transient_for)(dialog, app.window);
                (app.gtk.gtk_window_set_modal)(dialog, 1);
                (app.gtk.gtk_container_set_border_width)(dialog, 20);
                (app.gtk.gtk_window_set_default_size)(dialog, 420, 280);
                let content = (app.gtk.gtk_dialog_get_content_area)(dialog);
                app.pack(content, app.label(&titles));
                (app.gtk.gtk_dialog_add_button)(dialog, c(app.t("close")).as_ptr(), -7);
                app.connect(dialog, "response", destroy_dialog as *const (), data);
                (app.gtk.gtk_widget_show_all)(dialog);
            }
            app.model.settled(result.value.err());
            if app.model.action_error.is_some() {
                app.present(data);
            }
            app.next_action();
            app.request_status(true);
        }
        app.render();
    }
    let interval = if app.visible() { 3 } else { 15 };
    if app.last_poll.elapsed() >= Duration::from_secs(interval) {
        app.last_poll = Instant::now();
        app.check_upgrade();
        app.request_status(false);
        if !app.embedded() && !app.visible() {
            app.present(data);
        }
    }
    if app
        .capture_after
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        app.capture_after = None;
        app.capture();
    }
    if app
        .close_after
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        app.exiting = true;
        (app.gtk.g_application_quit)(app.app);
        return 0;
    }
    1
}
pub fn run() -> Result<(), String> {
    unsafe {
        let gtk = Gtk::open()?;
        let app = (gtk.gtk_application_new)(c("io.github.wenshiqi0.Kite.Host").as_ptr(), 8);
        if app.is_null() {
            return Err("GTK application unavailable".into());
        }
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        let assets = std::env::var_os("KITE_DESKTOP_ASSETS")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .to_path_buf()
            });
        let launcher = std::env::var_os("KITE_DESKTOP_LAUNCHER")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/bin/kite-desktop"));
        let original_launcher = std::fs::read(&launcher).ok();
        let (sender, receiver) = mpsc::channel();
        let mut state = Box::new(App {
            gtk,
            app,
            window: ptr::null_mut(),
            tray: ptr::null_mut(),
            menu: ptr::null_mut(),
            status_items: [ptr::null_mut(); 2],
            service_items: vec![],
            status: ptr::null_mut(),
            machine: ptr::null_mut(),
            services: ptr::null_mut(),
            hint: ptr::null_mut(),
            activity: ptr::null_mut(),
            error: ptr::null_mut(),
            error_box: ptr::null_mut(),
            error_dismiss: ptr::null_mut(),
            service_rows: vec![],
            details: ptr::null_mut(),
            ensure: ptr::null_mut(),
            menu_ensure: ptr::null_mut(),
            model: Model {
                health: "busy",
                ..Default::default()
            },
            text: serde_json::from_str(include_str!("../../../desktop/linux/strings.json"))
                .map_err(|e| e.to_string())?,
            assets,
            node: std::env::var_os("KITE_NODE_BINARY")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/bin/kite-node")),
            launcher,
            original_launcher,
            sender,
            receiver,
            last_poll: Instant::now(),
            exiting: false,
            snapshot: None,
            capture_after: None,
            close_after: None,
        });
        let data = (&mut *state as *mut App).cast();
        for (name, kind) in [
            ("background", 0),
            ("quit", 0),
            ("snapshot", 4),
            ("close-after", 2),
        ] {
            (state.gtk.g_application_add_main_option)(
                app,
                c(name).as_ptr(),
                0,
                0,
                kind,
                c(name).as_ptr(),
                ptr::null(),
            );
        }
        state.connect(app, "startup", startup as *const (), data);
        state.connect(app, "activate", activate as *const (), data);
        state.connect(app, "command-line", command_line as *const (), data);
        let args = std::env::args().map(|s| c(&s)).collect::<Vec<_>>();
        let mut pointers = args
            .iter()
            .map(|s| s.as_ptr() as *mut c_char)
            .chain([ptr::null_mut()])
            .collect::<Vec<_>>();
        let code = (state.gtk.g_application_run)(app, args.len() as c_int, pointers.as_mut_ptr());
        if !state.tray.is_null() {
            (state.gtk.gtk_status_icon_set_visible)(state.tray, 0);
        }
        (state.gtk.g_object_unref)(app);
        if code != 0 {
            return Err(format!("GTK exit status {code}"));
        }
        Ok(())
    }
}
