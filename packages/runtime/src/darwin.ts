/** Darwin process facts from Apple's libproc/sysctl, never from ps text matching. */
import { dlopen, FFIType, ptr } from "bun:ffi";
import { check } from "@kite/protocol";

const library = () =>
  dlopen("/usr/lib/libSystem.B.dylib", {
    proc_pidinfo: {
      args: [FFIType.i32, FFIType.i32, FFIType.u64, FFIType.ptr, FFIType.i32],
      returns: FFIType.i32,
    },
    sysctl: {
      args: [FFIType.ptr, FFIType.u32, FFIType.ptr, FFIType.ptr, FFIType.ptr, FFIType.u64],
      returns: FFIType.i32,
    },
  });
export function darwinProcess(
  pid: number,
): { started: string; status: number; uid: number; pgid: number } | undefined {
  if (!Number.isSafeInteger(pid) || pid <= 0) return undefined;
  const lib = library(),
    bytes = Buffer.alloc(136);
  try {
    // proc_bsdinfo, PROC_PIDTBSDINFO; layout in the macOS SDK sys/proc_info.h.
    if (lib.symbols.proc_pidinfo(pid, 3, 0, ptr(bytes), bytes.length) !== bytes.length)
      return undefined;
    if (bytes.readUInt32LE(12) !== pid) return undefined;
    return {
      started: (bytes.readBigUInt64LE(120) * 1000000n + bytes.readBigUInt64LE(128)).toString(),
      status: bytes.readUInt32LE(4),
      uid: bytes.readUInt32LE(20),
      pgid: bytes.readUInt32LE(100),
    };
  } finally {
    lib.close();
  }
}
export function darwinContext(pid: number): {
  argv: string[];
  environment: Record<string, string>;
  cwd: string;
} {
  const before = darwinProcess(pid);
  check(before && before.uid === process.getuid?.(), "process owner mismatch");
  const lib = library();
  try {
    const mib = new Int32Array([1, 49, pid]),
      size = new BigUint64Array(1);
    check(
      lib.symbols.sysctl(ptr(mib), 3, null, ptr(size), null, 0) === 0,
      "process arguments unavailable",
    );
    const length = Number(size[0]);
    check(length >= 4 && length <= 2 * 1024 * 1024, "invalid process argument size");
    const bytes = Buffer.alloc(length);
    check(
      lib.symbols.sysctl(ptr(mib), 3, ptr(bytes), ptr(size), null, 0) === 0,
      "process arguments unavailable",
    );
    const count = bytes.readInt32LE(0),
      end = Number(size[0]);
    check(count > 0 && count <= 4096 && end <= bytes.length, "invalid process argument count");
    let at = bytes.indexOf(0, 4) + 1;
    check(at > 4, "invalid executable path");
    while (at < end && bytes[at] === 0) at++;
    const word = () => {
      const next = bytes.indexOf(0, at);
      check(next >= at && next < end, "truncated process argument");
      const value = bytes.toString("utf8", at, next);
      at = next + 1;
      return value;
    };
    const argv = Array.from({ length: count }, word),
      environment: Record<string, string> = {};
    while (at < end) {
      if (bytes[at] === 0) {
        at++;
        continue;
      }
      const item = word(),
        equal = item.indexOf("=");
      if (equal > 0) environment[item.slice(0, equal)] = item.slice(equal + 1);
    }
    const vnode = Buffer.alloc(2352);
    check(
      lib.symbols.proc_pidinfo(pid, 9, 0, ptr(vnode), vnode.length) === vnode.length,
      "process directory unavailable",
    );
    const cwd = vnode.toString("utf8", 152, vnode.indexOf(0, 152));
    check(
      cwd.startsWith("/") && darwinProcess(pid)?.started === before.started,
      "process identity changed",
    );
    return { argv, environment, cwd };
  } finally {
    lib.close();
  }
}
