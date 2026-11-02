import { dlopen, JSCallback, type Pointer, ptr, FFIType as T, toArrayBuffer } from "bun:ffi";
import {
  existsSync,
  lstatSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { deflateSync } from "node:zlib";
import {
  array,
  check,
  integer,
  type Json,
  type ObjectValue,
  object,
  optionalString,
  string,
} from "@kite/protocol";
import { home, locked, privateDir, run, sha256, sleep } from "@kite/runtime";

const MAX_IMAGE = 8 * 1024 * 1024;
const ptrValue = (value: Pointer | null): Pointer => {
  check(value, "x11_unavailable");
  return value;
};
const bytes = (value: Pointer, size: number) => Buffer.from(toArrayBuffer(value, 0, size));
const cstr = (s: string) => Buffer.from(`${s}\0`);
export function sessionEnvironment(): NodeJS.ProcessEnv {
  const env = { ...process.env },
    display = env.DISPLAY;
  check(display, "display_not_configured");
  const keys = [
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XAUTHORITY",
    "XDG_CURRENT_DESKTOP",
    "DESKTOP_SESSION",
    "XDG_SESSION_TYPE",
    "CHROME_CONFIG_HOME",
    "CHROME_USER_DATA_DIR",
  ];
  for (const k of [
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "CHROME_CONFIG_HOME",
    "CHROME_USER_DATA_DIR",
  ])
    delete env[k];
  const names = new Set([
    "xfce4-session",
    "gnome-session-b",
    "gnome-shell",
    "ksmserver",
    "plasmashell",
    "mate-session",
    "cinnamon-sessio",
    "lxqt-session",
  ]);
  for (const pid of readdirSync("/proc")) {
    try {
      if (
        !/^\d+$/.test(pid) ||
        statSync(`/proc/${pid}`).uid !== process.getuid?.() ||
        !names.has(readFileSync(`/proc/${pid}/comm`, "utf8").trim())
      )
        continue;
      const values = Object.fromEntries(
        readFileSync(`/proc/${pid}/environ`)
          .subarray(0, 262144)
          .toString()
          .split("\0")
          .filter((v) => v.includes("="))
          .map((v) => {
            const at = v.indexOf("=");
            return [v.slice(0, at), v.slice(at + 1)];
          }),
      );
      if (values.DISPLAY?.replace(/\.0$/, "") !== display.replace(/\.0$/, "")) continue;
      for (const k of keys) {
        delete env[k];
        if (values[k] !== undefined) env[k] = values[k];
      }
      break;
    } catch {
      /* session process may exit */
    }
  }
  check(env.XDG_SESSION_TYPE !== "wayland", "wayland_session_unsupported");
  for (const key of Object.keys(env))
    if (
      key.startsWith("HERDR_") ||
      [
        "NOTIFY_SOCKET",
        "LISTEN_PID",
        "LISTEN_FDS",
        "LISTEN_FDNAMES",
        "WATCHDOG_PID",
        "WATCHDOG_USEC",
        "INVOCATION_ID",
        "SYSTEMD_EXEC_PID",
        "CODEX_THREAD_ID",
        "KITE_DESKTOP_ARTIFACTS",
      ].includes(key)
    )
      delete env[key];
  return env;
}
function library() {
  return dlopen("libX11.so.6", {
    XOpenDisplay: { args: [T.cstring], returns: T.ptr },
    XDefaultScreen: { args: [T.ptr], returns: T.i32 },
    XRootWindow: { args: [T.ptr, T.i32], returns: T.u64 },
    XDisplayWidth: { args: [T.ptr, T.i32], returns: T.i32 },
    XDisplayHeight: { args: [T.ptr, T.i32], returns: T.i32 },
    XCloseDisplay: { args: [T.ptr], returns: T.i32 },
    XSync: { args: [T.ptr, T.i32], returns: T.i32 },
    XSetErrorHandler: { args: [T.ptr], returns: T.ptr },
    XDisplayKeycodes: { args: [T.ptr, T.ptr, T.ptr], returns: T.i32 },
    XGetKeyboardMapping: { args: [T.ptr, T.u8, T.i32, T.ptr], returns: T.ptr },
    XChangeKeyboardMapping: { args: [T.ptr, T.i32, T.i32, T.ptr, T.i32], returns: T.i32 },
    XKeysymToKeycode: { args: [T.ptr, T.u64], returns: T.u8 },
    XInternAtom: { args: [T.ptr, T.cstring, T.i32], returns: T.u64 },
    XFree: { args: [T.ptr], returns: T.i32 },
    XGetWindowProperty: {
      args: [T.ptr, T.u64, T.u64, T.i64, T.i64, T.i32, T.u64, T.ptr, T.ptr, T.ptr, T.ptr, T.ptr],
      returns: T.i32,
    },
    XGetGeometry: {
      args: [T.ptr, T.u64, T.ptr, T.ptr, T.ptr, T.ptr, T.ptr, T.ptr, T.ptr],
      returns: T.i32,
    },
    XTranslateCoordinates: {
      args: [T.ptr, T.u64, T.u64, T.i32, T.i32, T.ptr, T.ptr, T.ptr],
      returns: T.i32,
    },
    XGetImage: { args: [T.ptr, T.u64, T.i32, T.i32, T.u32, T.u32, T.u64, T.i32], returns: T.ptr },
    XDestroyImage: { args: [T.ptr], returns: T.i32 },
  });
}
export class X11 {
  private lib = library();
  private x = this.lib.symbols;
  private handler = new JSCallback(() => 0, { args: [T.ptr, T.ptr], returns: T.i32 });
  readonly display: Pointer;
  readonly root: number | bigint;
  readonly width: number;
  readonly height: number;
  constructor(env: NodeJS.ProcessEnv) {
    check(["x64", "arm64"].includes(process.arch), "unsupported_x11_architecture");
    this.x.XSetErrorHandler(this.handler.ptr);
    this.display = ptrValue(this.x.XOpenDisplay(cstr(env.DISPLAY || "")));
    const screen = this.x.XDefaultScreen(this.display);
    this.root = this.x.XRootWindow(this.display, screen);
    this.width = this.x.XDisplayWidth(this.display, screen);
    this.height = this.x.XDisplayHeight(this.display, screen);
  }
  close(): void {
    this.x.XCloseDisplay(this.display);
    this.x.XSetErrorHandler(null);
    this.handler.close();
    this.lib.close();
  }
  property(window: number | bigint, name: string): string | number[] | undefined {
    const atom = this.x.XInternAtom(this.display, cstr(name), 1);
    if (!atom) return undefined;
    const kind = Buffer.alloc(8),
      size = Buffer.alloc(4),
      count = Buffer.alloc(8),
      after = Buffer.alloc(8),
      data = Buffer.alloc(8);
    const result = this.x.XGetWindowProperty(
      this.display,
      window,
      atom,
      0,
      2048,
      0,
      0,
      ptr(kind),
      ptr(size),
      ptr(count),
      ptr(after),
      ptr(data),
    );
    const address = Number(data.readBigUInt64LE()) as Pointer;
    if (result || !address) return undefined;
    try {
      const length = Math.min(
        Number(count.readBigUInt64LE()),
        size.readInt32LE() === 32 ? 512 : 8192,
      );
      if (size.readInt32LE() === 32) {
        const raw = bytes(address, length * 8);
        return Array.from({ length }, (_, i) => Number(raw.readBigUInt64LE(i * 8)));
      }
      if (size.readInt32LE() === 8) return bytes(address, length).toString();
      return undefined;
    } finally {
      this.x.XFree(address);
    }
  }
  window(window: number): Json | undefined {
    const parent = Buffer.alloc(8),
      x = Buffer.alloc(4),
      y = Buffer.alloc(4),
      w = Buffer.alloc(4),
      h = Buffer.alloc(4),
      border = Buffer.alloc(4),
      depth = Buffer.alloc(4);
    if (
      !this.x.XGetGeometry(
        this.display,
        window,
        ptr(parent),
        ptr(x),
        ptr(y),
        ptr(w),
        ptr(h),
        ptr(border),
        ptr(depth),
      )
    )
      return undefined;
    this.x.XTranslateCoordinates(
      this.display,
      window,
      this.root,
      0,
      0,
      ptr(x),
      ptr(y),
      ptr(parent),
    );
    return {
      id: `0x${window.toString(16)}`,
      title: [
        ...String(this.property(window, "_NET_WM_NAME") || this.property(window, "WM_NAME") || ""),
      ]
        .slice(0, 160)
        .join(""),
      x: x.readInt32LE(),
      y: y.readInt32LE(),
      width: w.readUInt32LE(),
      height: h.readUInt32LE(),
    };
  }
  point(x: number, y: number): void {
    check(
      Number.isSafeInteger(x) &&
        Number.isSafeInteger(y) &&
        x >= 0 &&
        y >= 0 &&
        x < this.width &&
        y < this.height,
      "coordinate_out_of_bounds",
    );
  }
  screenshot(region?: ObjectValue): { png: Buffer; metadata: ObjectValue } {
    const x = region ? integer(region.x) : 0,
      y = region ? integer(region.y) : 0,
      width = region ? integer(region.width) : this.width,
      height = region ? integer(region.height) : this.height;
    this.point(x, y);
    check(
      width > 0 &&
        height > 0 &&
        x + width <= this.width &&
        y + height <= this.height &&
        width * height <= 16777216,
      "invalid_capture_region",
    );
    const image = ptrValue(
      this.x.XGetImage(this.display, this.root, x, y, width, height, 0xffffffffffffffffn, 2),
    );
    try {
      const frame = bytes(image, 88);
      check(
        frame.readInt32LE(24) === 0 &&
          frame.readInt32LE(48) === 32 &&
          frame.readBigUInt64LE(56) === 0xff0000n &&
          frame.readBigUInt64LE(64) === 0xff00n &&
          frame.readBigUInt64LE(72) === 0xffn,
        "unsupported_pixel_format",
      );
      const stride = frame.readInt32LE(44);
      check(stride >= width * 4 && stride * height <= 128 * 1024 * 1024, "invalid_image_stride");
      const data = Number(frame.readBigUInt64LE(16)) as Pointer,
        raw = bytes(data, stride * height),
        rgb = Buffer.alloc(height * (1 + width * 3));
      for (let row = 0; row < height; row++)
        for (let col = 0; col < width; col++) {
          const source = row * stride + col * 4,
            target = row * (1 + width * 3) + 1 + col * 3;
          rgb[target] = raw[source + 2] ?? 0;
          rgb[target + 1] = raw[source + 1] ?? 0;
          rgb[target + 2] = raw[source] ?? 0;
        }
      const header = Buffer.alloc(13);
      header.writeUInt32BE(width, 0);
      header.writeUInt32BE(height, 4);
      header[8] = 8;
      header[9] = 2;
      const png = Buffer.concat([
        Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
        pngChunk("IHDR", header),
        pngChunk("IDAT", deflateSync(rgb, { level: 3 })),
        pngChunk("IEND", Buffer.alloc(0)),
      ]);
      check(png.length <= MAX_IMAGE, "image_too_large_use_region");
      return { png, metadata: { width, height, origin: { x, y } } };
    } finally {
      this.x.XDestroyImage(image);
    }
  }
  async type(text: string, env: NodeJS.ProcessEnv, artifacts: string): Promise<void> {
    const symbols = new Map(
      [...text]
        .filter((c) => (c.codePointAt(0) || 0) > 127)
        .map((c) => {
          const code = c.codePointAt(0) || 0;
          return [c, code <= 255 ? code : 0x01000000 | code] as const;
        })
        .filter(([, code]) => !this.x.XKeysymToKeycode(this.display, code)),
    );
    if (!symbols.size) {
      await inputCommand(["type", "--clearmodifiers", "--delay", "1", "--file", "-"], env, text);
      return;
    }
    await locked(join(privateDir(artifacts), "keyboard-map.lock"), async () => {
      const min = Buffer.alloc(4),
        max = Buffer.alloc(4),
        width = Buffer.alloc(4);
      this.x.XDisplayKeycodes(this.display, ptr(min), ptr(max));
      const count = max.readInt32LE() - min.readInt32LE() + 1,
        original = ptrValue(
          this.x.XGetKeyboardMapping(this.display, min.readInt32LE(), count, ptr(width)),
        ),
        columns = width.readInt32LE();
      check(
        columns > 0 && columns <= 256 && count > 0 && count <= 256,
        "keyboard_mapping_unavailable",
      );
      const slots: { code: number; before: Buffer }[] = [];
      try {
        const values = bytes(original, count * columns * 8);
        for (let i = 0; i < count; i++) {
          const before = Buffer.from(values.subarray(i * columns * 8, (i + 1) * columns * 8));
          if (before.every((v) => v === 0)) slots.push({ code: min.readInt32LE() + i, before });
        }
      } finally {
        this.x.XFree(original);
      }
      check(slots.length, "unicode_keycodes_unavailable");
      const batches: string[] = [];
      let batch = "",
        seen = new Set<string>();
      for (const char of text) {
        if (symbols.has(char) && !seen.has(char) && seen.size === slots.length) {
          batches.push(batch);
          batch = "";
          seen = new Set();
        }
        if (symbols.has(char)) seen.add(char);
        batch += char;
      }
      batches.push(batch);
      for (const part of batches) {
        const assigned: { code: number; before: Buffer; applied: Buffer }[] = [];
        try {
          let i = 0;
          for (const char of new Set([...part].filter((c) => symbols.has(c)))) {
            const slot = slots[i++];
            check(slot, "unicode_keycodes_unavailable");
            const values = Buffer.alloc(columns * 8);
            values.writeBigUInt64LE(BigInt(symbols.get(char) || 0));
            this.x.XChangeKeyboardMapping(this.display, slot.code, columns, ptr(values), 1);
            const appliedWidth = Buffer.alloc(4),
              applied = ptrValue(
                this.x.XGetKeyboardMapping(this.display, slot.code, 1, ptr(appliedWidth)),
              );
            try {
              assigned.push({
                ...slot,
                applied: Buffer.from(bytes(applied, appliedWidth.readInt32LE() * 8)),
              });
            } finally {
              this.x.XFree(applied);
            }
          }
          this.x.XSync(this.display, 0);
          // XSync finishes the server update; GTK/Xlib clients still need an
          // event-loop turn to consume MappingNotify before the first key.
          // Match the retention window so a 200–250ms busy target can refresh
          // its keymap before any key in this batch is queued.
          await sleep(350);
          await inputCommand(
            ["type", "--clearmodifiers", "--delay", "2", "--file", "-"],
            env,
            part,
          );
          // A short batch can still be queued in a busy application's event
          // loop. Keep its mapping stable before reusing the limited key pool.
          await sleep(batches.length > 1 ? 350 : Math.min(500, 50 + part.length));
        } finally {
          for (const slot of assigned) {
            const currentWidth = Buffer.alloc(4),
              current = this.x.XGetKeyboardMapping(this.display, slot.code, 1, ptr(currentWidth));
            if (current)
              try {
                if (
                  currentWidth.readInt32LE() * 8 === slot.applied.length &&
                  bytes(current, slot.applied.length).equals(slot.applied)
                )
                  this.x.XChangeKeyboardMapping(
                    this.display,
                    slot.code,
                    columns,
                    ptr(slot.before),
                    1,
                  );
              } finally {
                this.x.XFree(current);
              }
          }
          this.x.XSync(this.display, 0);
        }
      }
    });
  }
}
function pngChunk(name: string, data: Buffer): Buffer {
  const payload = Buffer.concat([Buffer.from(name), data]);
  let crc = 0xffffffff;
  for (const value of payload) {
    crc ^= value;
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  const head = Buffer.alloc(4),
    tail = Buffer.alloc(4);
  head.writeUInt32BE(data.length);
  tail.writeUInt32BE((crc ^ 0xffffffff) >>> 0);
  return Buffer.concat([head, payload, tail]);
}
async function inputCommand(args: string[], env: NodeJS.ProcessEnv, text?: string): Promise<void> {
  const path = Bun.which("xdotool", { PATH: env.PATH || "" });
  check(path, "xdotool_missing");
  await run([path, ...args], {
    env,
    ...(text === undefined ? {} : { input: text }),
    timeout: text ? Math.max(10000, text.length * 30 + 2000) : 10000,
  });
}
export async function desktop(request: ObjectValue): Promise<Json> {
  let x11: X11 | undefined;
  try {
    const env = sessionEnvironment();
    x11 = new X11(env);
    const action = string(request.action),
      artifacts = process.env.KITE_DESKTOP_ARTIFACTS || "";
    if (action === "status") {
      const active = x11.property(x11.root, "_NET_ACTIVE_WINDOW");
      return {
        ok: true,
        result: {
          available: true,
          backend: "x11",
          display: env.DISPLAY,
          width: x11.width,
          height: x11.height,
          active_window:
            Array.isArray(active) && active[0] ? (x11.window(active[0]) ?? null) : null,
          input_available: Boolean(Bun.which("xdotool", { PATH: env.PATH || "" })),
        },
      };
    }
    if (action === "windows") {
      const ids =
        x11.property(x11.root, "_NET_CLIENT_LIST_STACKING") ||
        x11.property(x11.root, "_NET_CLIENT_LIST") ||
        [];
      const rows = Array.isArray(ids)
        ? [...ids].reverse().flatMap((id) => {
            const w = x11?.window(id);
            return w ? [w] : [];
          })
        : [];
      return {
        ok: true,
        result: {
          windows: rows.slice(0, request.limit === undefined ? 16 : integer(request.limit)),
          total: rows.length,
          display: env.DISPLAY,
        },
      };
    }
    if (action === "screenshot") {
      const { png, metadata } = x11.screenshot(request.region ? object(request.region) : undefined),
        digest = sha256(png);
      Object.assign(metadata, {
        sha256: digest,
        mime: "image/png",
        unchanged: request.known_sha256 === digest,
      });
      if (!metadata.unchanged) {
        check(artifacts, "artifact_directory_not_private");
        if (existsSync(artifacts)) {
          const s = lstatSync(artifacts);
          check(
            s.isDirectory() && s.uid === process.getuid?.() && !(s.mode & 0o077),
            "artifact_directory_not_private",
          );
        }
        privateDir(artifacts);
        for (const name of readdirSync(artifacts))
          if (/^shot-[a-f0-9]{32}\.png$/.test(name)) {
            const p = join(artifacts, name),
              s = lstatSync(p);
            if (!s.isSymbolicLink() && Date.now() - s.mtimeMs > 3600000) rmSync(p, { force: true });
          }
        const path = join(artifacts, `shot-${crypto.randomUUID().replaceAll("-", "")}.png`);
        writeFileSync(path, png, { mode: 0o600, flag: "wx" });
        Object.assign(metadata, { path, bytes: png.length });
      }
      return { ok: true, result: metadata };
    }
    if (action === "move" || action === "click") {
      const x = integer(request.x),
        y = integer(request.y);
      x11.point(x, y);
      const args = ["mousemove", String(x), String(y)];
      if (action === "click") {
        const button = optionalString(request.button) || "left";
        check(["left", "middle", "right"].includes(button), "invalid_button");
        args.push("click", String(button === "left" ? 1 : button === "middle" ? 2 : 3));
      }
      await inputCommand(args, env);
    } else if (action === "scroll") {
      if (request.x !== undefined && request.x !== null) {
        const x = integer(request.x),
          y = integer(request.y);
        x11.point(x, y);
        await inputCommand(["mousemove", String(x), String(y)], env);
      }
      for (const [count, positive, negative] of [
        [integer(request.dx), 7, 6],
        [integer(request.dy), 5, 4],
      ] as const)
        if (count)
          await inputCommand(
            [
              "click",
              "--repeat",
              String(Math.abs(count)),
              "--delay",
              "20",
              String(count > 0 ? positive : negative),
            ],
            env,
          );
    } else if (action === "key")
      await inputCommand(["key", "--clearmodifiers", ...array(request.keys).map(string)], env);
    else if (action === "type") await x11.type(string(request.text), env, artifacts);
    else if (action === "launch") {
      const argv = array(request.argv).map(string);
      check(argv.length > 0 && argv.every((s) => s && !s.includes("\0")), "invalid_argv");
      const child = Bun.spawn(argv, {
        env,
        cwd: optionalString(request.cwd) || home(),
        stdin: "ignore",
        stdout: "ignore",
        stderr: "ignore",
        detached: true,
      });
      child.unref();
      return { ok: true, result: { launched: true, pid: child.pid, display: env.DISPLAY } };
    } else throw new Error("unknown_action");
    return { ok: true, result: { dispatched: true, action, display: env.DISPLAY } };
  } catch (error) {
    return {
      ok: false,
      error:
        error instanceof Error && /^[a-z_]+$/.test(error.message)
          ? error.message
          : "desktop_adapter_failed",
    };
  } finally {
    x11?.close();
  }
}
