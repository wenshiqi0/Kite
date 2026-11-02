/** Owned GTK input target. Records counts and hashes, never arbitrary input text. */
import { CString, dlopen, JSCallback, ptr, FFIType as T } from "bun:ffi";
import { parseArgs } from "node:util";
import { check } from "@kite/protocol";
import { atomic, sha256, sleep } from "@kite/runtime";
import { display, inputFocus, x11 } from "./x11";

const { values } = parseArgs({ options: { state: { type: "string" } }, strict: true });
check(values.state && process.env.DISPLAY, "owned fixture needs --state and DISPLAY");
// The owned widget tests direct key delivery, independently of host IM daemons.
process.env.GTK_IM_MODULE = "gtk-im-context-simple";
process.env.XMODIFIERS = "@im=none";
const statePath = values.state;
const gtk = dlopen("libgtk-3.so.0", {
  gtk_init_check: { args: [T.ptr, T.ptr], returns: T.i32 },
  gtk_window_new: { args: [T.i32], returns: T.ptr },
  gtk_window_set_title: { args: [T.ptr, T.ptr], returns: T.void },
  gtk_window_set_default_size: { args: [T.ptr, T.i32, T.i32], returns: T.void },
  gtk_container_set_border_width: { args: [T.ptr, T.u32], returns: T.void },
  gtk_container_add: { args: [T.ptr, T.ptr], returns: T.void },
  gtk_box_new: { args: [T.i32, T.i32], returns: T.ptr },
  gtk_box_pack_start: { args: [T.ptr, T.ptr, T.i32, T.i32, T.u32], returns: T.void },
  gtk_label_new: { args: [T.ptr], returns: T.ptr },
  gtk_entry_new: { args: [], returns: T.ptr },
  gtk_entry_set_placeholder_text: { args: [T.ptr, T.ptr], returns: T.void },
  gtk_entry_get_text: { args: [T.ptr], returns: T.ptr },
  gtk_event_box_new: { args: [], returns: T.ptr },
  gtk_drawing_area_new: { args: [], returns: T.ptr },
  gtk_widget_set_size_request: { args: [T.ptr, T.i32, T.i32], returns: T.void },
  gtk_widget_add_events: { args: [T.ptr, T.i32], returns: T.void },
  gtk_widget_get_window: { args: [T.ptr], returns: T.ptr },
  gtk_widget_translate_coordinates: {
    args: [T.ptr, T.ptr, T.i32, T.i32, T.ptr, T.ptr],
    returns: T.i32,
  },
  gtk_widget_get_allocated_width: { args: [T.ptr], returns: T.i32 },
  gtk_widget_get_allocated_height: { args: [T.ptr], returns: T.i32 },
  gtk_widget_show_all: { args: [T.ptr], returns: T.void },
  gtk_window_present: { args: [T.ptr], returns: T.void },
  gtk_widget_grab_focus: { args: [T.ptr], returns: T.void },
  gtk_events_pending: { args: [], returns: T.i32 },
  gtk_main_iteration_do: { args: [T.i32], returns: T.i32 },
  gtk_widget_destroy: { args: [T.ptr], returns: T.void },
});
const gdk = dlopen("libgdk-3.so.0", {
  gdk_window_get_origin: { args: [T.ptr, T.ptr, T.ptr], returns: T.i32 },
  gdk_x11_window_get_xid: { args: [T.ptr], returns: T.u64 },
  gdk_event_get_button: { args: [T.ptr, T.ptr], returns: T.i32 },
  gdk_event_get_keyval: { args: [T.ptr, T.ptr], returns: T.i32 },
});
const gobject = dlopen("libgobject-2.0.so.0", {
  g_signal_connect_data: { args: [T.ptr, T.ptr, T.ptr, T.ptr, T.ptr, T.i32], returns: T.u64 },
});
const cairo = dlopen("libcairo.so.2", {
  cairo_set_source_rgb: { args: [T.ptr, T.f64, T.f64, T.f64], returns: T.void },
  cairo_paint: { args: [T.ptr], returns: T.void },
});
const text = (value: string) => Buffer.from(`${value}\0`),
  api = gtk.symbols;
check(api.gtk_init_check(null, null), "GTK fixture initialization failed");
const handle = display(),
  previous = inputFocus(handle),
  window = api.gtk_window_new(0),
  box = api.gtk_box_new(1, 16),
  entry = api.gtk_entry_new(),
  target = api.gtk_event_box_new(),
  tile = api.gtk_drawing_area_new(),
  callbacks: JSCallback[] = [];
check(window && box && entry && target && tile, "GTK fixture widgets unavailable");
let running = true;
const state: Record<string, unknown> = {
  left: 0,
  right: 0,
  scroll: 0,
  f8: false,
  text_matched: false,
  manager_environment_cleared: ![
    "NOTIFY_SOCKET",
    "LISTEN_FDS",
    "CODEX_THREAD_ID",
    "HERDR_ENV",
  ].some((key) => key in process.env),
};
api.gtk_window_set_title(window, ptr(text("Kite owned desktop API validation")));
api.gtk_window_set_default_size(window, 520, 360);
api.gtk_container_set_border_width(window, 20);
api.gtk_container_add(window, box);
api.gtk_box_pack_start(box, api.gtk_label_new(ptr(text("Kite · owned desktop API test"))), 0, 0, 0);
api.gtk_entry_set_placeholder_text(entry, ptr(text("Owned UTF-8 test only")));
api.gtk_box_pack_start(box, entry, 0, 0, 0);
api.gtk_container_add(target, api.gtk_label_new(ptr(text("CLICK / SCROLL TARGET"))));
api.gtk_widget_set_size_request(target, -1, 120);
api.gtk_widget_add_events(target, (1 << 8) | (1 << 21));
api.gtk_box_pack_start(box, target, 1, 1, 0);
api.gtk_widget_set_size_request(tile, -1, 40);
api.gtk_box_pack_start(box, tile, 0, 0, 0);
function connect(widget: typeof window, name: string, callback: JSCallback): void {
  callbacks.push(callback);
  gobject.symbols.g_signal_connect_data(widget, ptr(text(name)), callback.ptr, null, null, 0);
}
function save(): void {
  const surface = api.gtk_widget_get_window(window);
  if (!surface) return;
  const ox = new Int32Array(1),
    oy = new Int32Array(1);
  gdk.symbols.gdk_window_get_origin(surface, ptr(ox), ptr(oy));
  state.window = Number(gdk.symbols.gdk_x11_window_get_xid(surface));
  for (const [name, widget] of [
    ["entry", entry],
    ["target", target],
    ["tile", tile],
  ] as const) {
    const px = new Int32Array(1),
      py = new Int32Array(1);
    api.gtk_widget_translate_coordinates(widget, window, 0, 0, ptr(px), ptr(py));
    state[name] = {
      x: (ox[0] || 0) + (px[0] || 0),
      y: (oy[0] || 0) + (py[0] || 0),
      width: api.gtk_widget_get_allocated_width(widget),
      height: api.gtk_widget_get_allocated_height(widget),
    };
  }
  atomic(statePath, state);
}
connect(
  tile,
  "draw",
  new JSCallback(
    (_widget, context) => {
      cairo.symbols.cairo_set_source_rgb(context, 0.12, 0.65, 0.35);
      cairo.symbols.cairo_paint(context);
      return 0;
    },
    { args: [T.ptr, T.ptr, T.ptr], returns: T.i32 },
  ),
);
connect(
  entry,
  "changed",
  new JSCallback(
    () => {
      const value = api.gtk_entry_get_text(entry);
      if (value) {
        const body = new CString(value).toString();
        state.text_matched = body === "Kite 你好 19";
        state.text_length = [...body].length;
        state.text_sha256 = sha256(body);
      }
    },
    { args: [T.ptr, T.ptr], returns: T.void },
  ),
);
connect(
  target,
  "button-press-event",
  new JSCallback(
    (_widget, event) => {
      const button = new Uint32Array(1);
      gdk.symbols.gdk_event_get_button(event, ptr(button));
      const key = button[0] === 1 ? "left" : button[0] === 3 ? "right" : "";
      if (key) state[key] = Number(state[key]) + 1;
      save();
      return 0;
    },
    { args: [T.ptr, T.ptr, T.ptr], returns: T.i32 },
  ),
);
connect(
  target,
  "scroll-event",
  new JSCallback(
    () => {
      state.scroll = Number(state.scroll) + 1;
      save();
      return 0;
    },
    { args: [T.ptr, T.ptr, T.ptr], returns: T.i32 },
  ),
);
connect(
  window,
  "key-press-event",
  new JSCallback(
    (_widget, event) => {
      const key = new Uint32Array(1);
      gdk.symbols.gdk_event_get_keyval(event, ptr(key));
      if (key[0] === 0xffc5) {
        state.f8 = true;
        save();
      }
      return 0;
    },
    { args: [T.ptr, T.ptr, T.ptr], returns: T.i32 },
  ),
);
connect(
  window,
  "destroy",
  new JSCallback(
    () => {
      running = false;
    },
    { args: [T.ptr, T.ptr], returns: T.void },
  ),
);
process.on("SIGTERM", () => {
  running = false;
});
api.gtk_widget_show_all(window);
api.gtk_window_present(window);
api.gtk_widget_grab_focus(entry);
const started = Date.now();
let saved = 0,
  focused = false;
try {
  while (running) {
    while (api.gtk_events_pending()) api.gtk_main_iteration_do(0);
    if (!focused && Date.now() - started >= 200) {
      const surface = api.gtk_widget_get_window(window);
      if (surface) {
        x11.symbols.XSetInputFocus(
          handle,
          Number(gdk.symbols.gdk_x11_window_get_xid(surface)),
          1,
          0,
        );
        x11.symbols.XSync(handle, 0);
        state.input_ready = true;
        focused = true;
      }
    }
    if (Date.now() - saved >= 100) {
      save();
      saved = Date.now();
    }
    await sleep(5);
  }
} finally {
  const surface = api.gtk_widget_get_window(window);
  if (
    surface &&
    inputFocus(handle).window === Number(gdk.symbols.gdk_x11_window_get_xid(surface))
  ) {
    x11.symbols.XSetInputFocus(handle, previous.window, previous.revert, 0);
    x11.symbols.XSync(handle, 0);
  }
  api.gtk_widget_destroy(window);
  x11.symbols.XCloseDisplay(handle);
  for (const callback of callbacks) callback.close();
}
