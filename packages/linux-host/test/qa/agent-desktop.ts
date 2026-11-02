/** Real CLI/MCP/X11 acceptance against an owned Bun/GTK fixture and a private Node. */
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { array, check, type Json, object, parse, string } from "@kite/protocol";
import { atomic, sha256, sleep } from "@kite/runtime";
import { RpcError, RpcProcess } from "@kite/runtime/rpc";
import { command } from "../../src/command";
import { keyboardMapHash } from "./x11";

const { values } = parseArgs({
  options: { binary: { type: "string" }, evidence: { type: "string" } },
  strict: true,
});
check(values.binary && process.env.DISPLAY, "owned X11 display and --binary are required");
const root = mkdtempSync(join(tmpdir(), "kite-desktop-api-")),
  binary = join(root, "test-node"),
  statePath = join(root, "fixture.json"),
  socket = join(root, "node/host.sock"),
  config = join(root, "node.json"),
  interpreter = join(root, "interpreter");
copyFileSync(resolve(values.binary), binary);
chmodSync(binary, 0o700);
mkdirSync(join(root, "node"), { mode: 0o700 });
writeFileSync(
  interpreter,
  `#!${process.execPath}\nif(process.argv[3]==="notification-facts")console.log('{"facts":[]}');else {const r=Bun.spawnSync([${JSON.stringify(process.execPath)},...process.argv.slice(2)],{stdin:"inherit",stdout:"inherit",stderr:"inherit"});process.exit(r.exitCode);}\n`,
);
chmodSync(interpreter, 0o700);
const environment = Object.fromEntries(
  ["DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR", "PATH"].flatMap((key) =>
    process.env[key] ? [[key, string(process.env[key])]] : [],
  ),
);
atomic(config, {
  node_id: "owned-desktop-api",
  machine: "owned-fixture",
  socket,
  backend: join(root, "host.js"),
  herdr: "/usr/bin/true",
  bun: interpreter,
  systemctl: "/usr/bin/false",
  services: [],
  environment,
});
const node = Bun.spawn([binary, "serve", "--config", config], {
  env: {
    ...process.env,
    NOTIFY_SOCKET: "/owned-not-a-manager",
    LISTEN_FDS: "99",
    CODEX_THREAD_ID: "owned-not-a-real-thread",
    HERDR_ENV: "1",
  },
  stdin: "ignore",
  stdout: "ignore",
  stderr: "pipe",
});
let rpc: RpcProcess | undefined, fixturePid: number | undefined;
const cli = async (args: string[], input?: string) =>
  object(
    parse(
      (
        await command([binary, "--socket", socket, "desktop", ...args], {
          timeout: 30000,
          ...(input === undefined ? {} : { input }),
        })
      ).stdout,
    ),
  );
const fixture = () => object(parse(readFileSync(statePath, "utf8")));
async function waitFor(predicate: () => boolean, label: string): Promise<void> {
  const deadline = Date.now() + 6000;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await sleep(30);
  }
  const state = existsSync(statePath) ? fixture() : {};
  throw new Error(`owned fixture did not reach ${label}: ${JSON.stringify(state)}`);
}
async function ownFocus(): Promise<void> {
  check(
    Number((await command(["xdotool", "getwindowfocus"], { env: environment })).stdout) ===
      fixture().window,
    "focus moved away from the owned fixture; refusing input",
  );
}
async function tool(name: string, args: Json) {
  check(rpc, "MCP transport unavailable");
  const result = object(await rpc.call("tools/call", { name, arguments: args }));
  check(!result.isError, "owned MCP tool failed");
  return result;
}
try {
  await waitFor(() => existsSync(socket), "private listener");
  check((await cli(["status"])).available, "owned X11 unavailable");
  const launched = await cli([
    "launch",
    "--",
    process.execPath,
    resolve("packages/linux-host/test/qa/desktop-fixture.ts"),
    "--state",
    statePath,
  ]);
  fixturePid = Number(launched.pid);
  await waitFor(
    () => existsSync(statePath) && Boolean(fixture().tile) && Boolean(fixture().input_ready),
    "window geometry and focus",
  );
  check(fixture().manager_environment_cleared, "launch environment was not sanitized");
  const before = keyboardMapHash(),
    entry = object(fixture().entry);
  await ownFocus();
  await cli([
    "click",
    String(Number(entry.x) + 10),
    String(Number(entry.y) + Math.floor(Number(entry.height) / 2)),
  ]);
  await ownFocus();
  await cli(["key", "ctrl+a"]);
  await cli(["type", "--stdin"], "Kite 你好 19");
  await waitFor(() => Boolean(fixture().text_matched), "UTF-8 input");
  for (const body of [
    "你好".repeat(256),
    Array.from({ length: 512 }, (_, i) => String.fromCharCode(0x4e00 + i)).join(""),
  ]) {
    await ownFocus();
    await cli(["key", "ctrl+a"]);
    await cli(["type", "--stdin"], body);
    await waitFor(
      () => fixture().text_sha256 === sha256(body),
      `Unicode batch length=${[...body].length} sha256=${sha256(body)}`,
    );
  }
  check(keyboardMapHash() === before, "temporary keyboard mappings were not restored");
  await ownFocus();
  await cli(["key", "F8"]);
  await waitFor(() => Boolean(fixture().f8), "F8 event");
  const target = object(fixture().target),
    x = String(Number(target.x) + Math.floor(Number(target.width) / 2)),
    y = String(Number(target.y) + Math.floor(Number(target.height) / 2));
  await ownFocus();
  await cli(["move", x, y]);
  await cli(["click", x, y]);
  await cli(["click", x, y, "right"]);
  await cli(["scroll", "0", "2", "--at", x, y]);
  await waitFor(
    () =>
      Number(fixture().left) > 0 && Number(fixture().right) > 0 && Number(fixture().scroll) >= 2,
    "pointer events",
  );
  rpc = new RpcProcess([binary, "--socket", socket, "mcp"], root);
  await rpc.call("initialize", {
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "neutral-contract-probe", version: "1" },
  });
  const tools = array(object(await rpc.call("tools/list", {})).tools)
    .map(object)
    .map((row) => string(row.name))
    .sort();
  check(
    JSON.stringify(tools) ===
      JSON.stringify(
        ["kite_host_status", "kite_sessions_list", "kite_docs", "kite_desktop"].sort(),
      ),
    "standard MCP tools changed",
  );
  const first = await tool("kite_docs", { topics: ["desktop"] }),
    text = string(object(array(first.content)[0]).text),
    revision = text.split("revision=")[1]?.split("]")[0];
  check(revision, "documentation revision missing");
  const cached = await tool("kite_docs", { topics: ["desktop"], known: { desktop: revision } });
  check(
    string(object(array(cached.content)[0]).text).length < text.length / 5,
    "conditional docs repeated the body",
  );
  const region = object(fixture().tile),
    crop = { x: Number(region.x) + 5, y: Number(region.y) + 5, width: 40, height: 20 },
    shot = await tool("kite_desktop", { action: "screenshot", params: { region: crop } }),
    metadata = object(shot.structuredContent),
    images = array(shot.content)
      .map(object)
      .filter((row) => row.type === "image");
  check(images.length === 1, "MCP screenshot pixels missing");
  const png = Buffer.from(string(images[0]?.data), "base64");
  check(
    png.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])) &&
      png.readUInt32BE(16) === 40 &&
      png.readUInt32BE(20) === 20,
    "crop PNG invalid",
  );
  check(
    object(metadata.origin).x === crop.x &&
      object(metadata.origin).y === crop.y &&
      metadata.sha256 === sha256(png),
    "crop metadata differs from pixels",
  );
  const mode = (await import("node:fs")).statSync(string(metadata.path)).mode;
  check((mode & 0o777) === 0o600, "screenshot artifact is not private");
  const same = await tool("kite_desktop", {
    action: "screenshot",
    params: { region: crop, known_sha256: metadata.sha256 },
  });
  check(
    object(same.structuredContent).unchanged &&
      !array(same.content)
        .map(object)
        .some((row) => row.type === "image"),
    "unchanged image was repeated",
  );
  try {
    await rpc.call("tools/call", {
      name: "kite_desktop",
      arguments: { action: "status", params: { action: "launch", argv: ["bad"] } },
    });
    throw new Error("action override accepted");
  } catch (error) {
    check(error instanceof RpcError && error.code === -32602, "action override validation changed");
  }
  check(Array.isArray((await cli(["windows"])).windows), "window enumeration invalid");
  const result = {
    status: "PASS",
    actualX11: true,
    utf8Text: true,
    maxLengthUtf8Text: true,
    distinctUnicodeBatches: true,
    keyboardMapRestored: true,
    keyChordAndF8: true,
    leftAndRightClicks: true,
    scrollEvents: true,
    configuredTarget: true,
    launchEnvironmentSanitized: true,
    standardMcpImage: true,
    cropOriginAndPixels: true,
    privateArtifacts: true,
    unchangedImageOmitted: true,
    docsConditionalBody: true,
    actionOverrideRejected: true,
  };
  console.log(JSON.stringify(result));
  if (values.evidence) {
    mkdirSync(values.evidence, { recursive: true });
    atomic(join(values.evidence, "native-validation.json"), result);
    writeFileSync(join(values.evidence, "owned-crop.png"), png);
  }
} finally {
  rpc?.close();
  if (fixturePid) {
    try {
      process.kill(fixturePid, "SIGTERM");
    } catch {
      /* already exited */
    }
  }
  node.kill();
  await node.exited;
  rmSync(root, { recursive: true, force: true });
}
