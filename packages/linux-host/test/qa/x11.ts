import { dlopen, ptr, FFIType as T, toArrayBuffer } from "bun:ffi";
import { check } from "@kite/protocol";
import { sha256 } from "@kite/runtime";

export const x11 = dlopen("libX11.so.6", {
  XOpenDisplay: { args: [T.ptr], returns: T.ptr },
  XCloseDisplay: { args: [T.ptr], returns: T.i32 },
  XGetInputFocus: { args: [T.ptr, T.ptr, T.ptr], returns: T.i32 },
  XSetInputFocus: { args: [T.ptr, T.u64, T.i32, T.u64], returns: T.i32 },
  XSync: { args: [T.ptr, T.i32], returns: T.i32 },
  XDisplayKeycodes: { args: [T.ptr, T.ptr, T.ptr], returns: T.i32 },
  XGetKeyboardMapping: { args: [T.ptr, T.u8, T.i32, T.ptr], returns: T.ptr },
  XFree: { args: [T.ptr], returns: T.i32 },
});
export function display() {
  const result = x11.symbols.XOpenDisplay(null);
  check(result, "owned X11 display unavailable");
  return result;
}
export function inputFocus(handle: ReturnType<typeof display>): { window: number; revert: number } {
  const window = new BigUint64Array(1),
    revert = new Int32Array(1);
  x11.symbols.XGetInputFocus(handle, ptr(window), ptr(revert));
  return { window: Number(window[0]), revert: revert[0] || 0 };
}
export function keyboardMapHash(): string {
  const handle = display(),
    low = new Int32Array(1),
    high = new Int32Array(1),
    width = new Int32Array(1);
  let values: ReturnType<typeof x11.symbols.XGetKeyboardMapping> = null;
  try {
    x11.symbols.XDisplayKeycodes(handle, ptr(low), ptr(high));
    const count = (high[0] || 0) - (low[0] || 0) + 1;
    values = x11.symbols.XGetKeyboardMapping(handle, low[0] || 0, count, ptr(width));
    check(values, "owned keyboard map unavailable");
    return sha256(new Uint8Array(toArrayBuffer(values, 0, count * (width[0] || 0) * 8)));
  } finally {
    if (values) x11.symbols.XFree(values);
    x11.symbols.XCloseDisplay(handle);
  }
}
