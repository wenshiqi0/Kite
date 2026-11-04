/** The Mac Sunshine capture target, verified against its listening process. */
import { readFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { check, integer, type Json, object, parse } from "@kite/protocol";
import { processContext, run, startTicks } from "@kite/runtime";

export async function capturedDisplay(port: number): Promise<Json> {
  check(
    process.platform === "darwin" && Number.isSafeInteger(port) && port > 0 && port <= 65535,
    "invalid Mac capture endpoint",
  );
  const found = await run(["/usr/sbin/lsof", "-nP", "-a", `-iTCP:${port}`, "-sTCP:LISTEN", "-Fp"], {
    timeout: 3000,
    limit: 65536,
  });
  const pids = new Set(
    [...found.stdout.toString().matchAll(/^p(\d+)$/gm)].map((m) => Number(m[1])),
  );
  check(pids.size === 1, "captured desktop process unavailable");
  const pid = [...pids][0];
  check(pid, "captured desktop process unavailable");
  const started = startTicks(pid),
    context = processContext(pid);
  check(
    started && basename(context.argv[0] || "").toLowerCase() === "sunshine",
    "capture process identity mismatch",
  );
  const argument = context.argv[1],
    file = argument?.startsWith("/")
      ? argument
      : join(context.environment.HOME || "", ".config/sunshine/sunshine.conf");
  check(file.startsWith("/"), "capture configuration unavailable");
  const config = readFileSync(file);
  check(config.length <= 1024 * 1024, "capture configuration exceeds limit");
  const settings: Record<string, string> = {};
  for (const line of [...config.toString().split("\n"), ...context.argv.slice(1)]) {
    const match = /^\s*(output_name|capture)\s*=\s*([^#\r\n]*?)\s*(?:#.*)?$/.exec(line);
    if (match?.[1]) settings[match[1]] = match[2] || "";
  }
  check(
    !settings.output_name || /^\d+$/.test(settings.output_name),
    "invalid Mac capture display ID",
  );
  const result = await run(
    [
      join(dirname(process.execPath), "host-platform"),
      "display",
      "--output",
      settings.output_name || "",
    ],
    { timeout: 3000, limit: 65536 },
  );
  const value = object(parse(result.stdout.toString())),
    width = integer(value.width),
    height = integer(value.height);
  check(
    width >= 320 &&
      width <= 16384 &&
      height >= 240 &&
      height <= 16384 &&
      startTicks(pid) === started,
    "capture process or display changed",
  );
  return { width, height, displayId: integer(value.displayId) };
}
