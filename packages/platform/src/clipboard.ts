import { existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { check, type Json } from "@kite/protocol";
import { home, run } from "@kite/runtime";
export const CLIPBOARD_LIMIT = 256 * 1024;
export async function clipboardEnvironment(
  port: number,
): Promise<{ env: NodeJS.ProcessEnv; platform: string }> {
  if (process.platform === "darwin") return { env: { ...process.env }, platform: "macos" };
  check(process.platform === "linux", "clipboard platform unsupported");
  const result = await run(["ss", "-H", "-lntp", `sport = :${port}`], { timeout: 2000 }),
    pids = new Set([...result.stdout.toString().matchAll(/pid=(\d+)/g)].map((m) => m[1]));
  check(pids.size === 1, "captured desktop process unavailable");
  const pid = [...pids][0],
    root = `/proc/${pid}`;
  check(
    statSync(root).uid === process.getuid?.() &&
      readFileSync(`${root}/comm`, "utf8").trim() === "sunshine",
    "captured desktop process identity mismatch",
  );
  const allowed = [
      "DISPLAY",
      "XAUTHORITY",
      "WAYLAND_DISPLAY",
      "XDG_RUNTIME_DIR",
      "DBUS_SESSION_BUS_ADDRESS",
    ],
    values = Object.fromEntries(
      readFileSync(`${root}/environ`)
        .toString()
        .split("\0")
        .filter((s) => s.includes("="))
        .map((s) => {
          const at = s.indexOf("=");
          return [s.slice(0, at), s.slice(at + 1)];
        }),
    );
  check(values.DISPLAY || values.WAYLAND_DISPLAY, "captured desktop environment unavailable");
  const env = { ...process.env };
  for (const name of allowed) {
    delete env[name];
    if (values[name] !== undefined) env[name] = values[name];
  }
  return { env, platform: "linux" };
}
function executable(name: string): string {
  const local = join(home(), ".local/lib/kite/bin", name),
    path = existsSync(local) ? local : Bun.which(name);
  check(path, `clipboard tool unavailable: ${name}`);
  return path;
}
export async function transfer(
  mode: string,
  data: Buffer,
  env: NodeJS.ProcessEnv,
  platform: string,
): Promise<string> {
  check(
    ["read", "write"].includes(mode) && data.length <= CLIPBOARD_LIMIT,
    "invalid clipboard operation or size",
  );
  const args =
    platform === "macos"
      ? [mode === "read" ? "/usr/bin/pbpaste" : "/usr/bin/pbcopy"]
      : env.WAYLAND_DISPLAY
        ? mode === "read"
          ? [executable("wl-paste"), "--no-newline", "--type", "text"]
          : [executable("wl-copy"), "--type", "text/plain;charset=utf-8"]
        : [
            executable("xclip"),
            "-selection",
            "clipboard",
            mode === "read" ? "-out" : "-in",
            "-target",
            "UTF8_STRING",
          ];
  const result = await run(args, {
    env,
    ...(mode === "write" ? { input: data } : {}),
    timeout: 3000,
    limit: CLIPBOARD_LIMIT,
    allowFailure: true,
  });
  if (result.code !== 0) {
    if (
      mode === "read" &&
      platform === "linux" &&
      !env.WAYLAND_DISPLAY &&
      result.stderr.toString().trim() === "Error: target UTF8_STRING not available"
    )
      return "";
    throw new Error("clipboard operation failed");
  }
  return new TextDecoder("utf-8", { fatal: true }).decode(result.stdout);
}
export async function clipboard(mode: string, port: number, data: Buffer): Promise<Json> {
  check(Number.isSafeInteger(port) && port > 0 && port <= 65535, "invalid desktop port");
  const { env, platform } = await clipboardEnvironment(port),
    text = await transfer(mode, data, env, platform);
  return { platform, text };
}
