#!/usr/bin/env bun
/** User-owned installation and compatible handover; external services keep their owners. */
import {
  chmodSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  symlinkSync,
  unlinkSync,
} from "node:fs";
import { homedir, hostname } from "node:os";
import { dirname, join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { array, check, type ObjectValue, object, parse, string } from "@kite/protocol";
import { atomicBytes, sha256, sleep } from "@kite/runtime";
import { type Command, command } from "./command";

export const APP_ID = "io.github.wenshiqi0.Kite.Host";
export const digest = (path: string): string => sha256(readFileSync(path));
export interface InstallOptions {
  packageRoot: string;
  home?: string;
  binary?: string;
  desktopBinary?: string;
  enable?: boolean;
  migrateNode?: boolean;
  skillDirs?: string[];
}
export interface InstallDependencies {
  run?: Command;
  which?: (name: string) => string | null;
  runningNodeMatches?: (expected: string) => Promise<boolean>;
  waitForNode?: (argv: string[], version: string, expected: string) => Promise<void>;
  refreshGui?: (environment: NodeJS.ProcessEnv) => Promise<void>;
  environment?: NodeJS.ProcessEnv;
}
const expand = (path: string, home: string) =>
  resolve(path === "~" ? home : path.startsWith("~/") ? join(home, path.slice(2)) : path);
const exists = (path: string) => {
  try {
    lstatSync(path);
    return true;
  } catch {
    return false;
  }
};
function link(path: string, target: string): void {
  mkdirSync(dirname(path), { recursive: true });
  const pending = `${path}.new`;
  if (exists(pending)) unlinkSync(pending);
  symlinkSync(target, pending);
  renameSync(pending, path);
}
export async function runningNodeMatches(
  expected: string,
  run: Command = command,
): Promise<boolean> {
  const result = await run(
    ["systemctl", "--user", "show", "kite-node.service", "--property=MainPID", "--value"],
    { timeout: 5000 },
  );
  const pid = Number(result.stdout.trim());
  try {
    return Number.isSafeInteger(pid) && pid > 0 && digest(`/proc/${pid}/exe`) === expected;
  } catch {
    return false;
  }
}
export async function waitForNode(
  argv: string[],
  version: string,
  expected: string,
  run: Command = command,
): Promise<void> {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    try {
      const result = object(
        parse(
          (await run([...argv, "api-version"], { timeout: Math.min(6000, deadline - Date.now()) }))
            .stdout,
        ),
      );
      if (
        result.version === version &&
        result.api === 1 &&
        digest(`/proc/${Number(result.pid)}/exe`) === expected
      )
        return;
    } catch {
      /* inspect again without replaying the upgrade */
    }
    await sleep(100);
  }
  throw new Error("matching Kite node listener did not become ready");
}
export async function refreshGui(
  environment: NodeJS.ProcessEnv,
  run: Command = command,
): Promise<void> {
  const env = { ...process.env, ...environment };
  if (!env.DBUS_SESSION_BUS_ADDRESS) return;
  await run(
    [
      "gdbus",
      "call",
      "--session",
      "--dest",
      APP_ID,
      "--object-path",
      "/io/github/wenshiqi0/Kite/Host",
      "--method",
      "org.gtk.Actions.Activate",
      "upgrade",
      "[]",
      "{}",
    ],
    { env, timeout: 5000, allowFailure: true },
  );
}
export function configuration(
  home: string,
  which = Bun.which,
  environment: NodeJS.ProcessEnv = process.env,
): ObjectValue {
  const path = join(home, ".config/kite/node.json"),
    bun = which("bun");
  check(bun, "install Bun 1.3.14 or newer before installing the host");
  if (existsSync(path)) {
    const value = object(parse(readFileSync(path, "utf8")));
    delete value.python;
    value.bun ??= realpathSync(bun);
    value.backend = join(home, ".local/lib/kite/host.js");
    return value;
  }
  const herdr = which("herdr") || join(home, ".local/bin/herdr");
  check(existsSync(herdr), "install Herdr first");
  const env: Record<string, string> = {
    PATH: `${dirname(bun)}:${home}/.local/bin:${home}/.cargo/bin:/usr/local/bin:/usr/bin:/bin`,
    XDG_RUNTIME_DIR: environment.XDG_RUNTIME_DIR || `/run/user/${process.getuid?.()}`,
  };
  for (const proc of readdirSync("/proc")) {
    try {
      if (!/^\d+$/.test(proc) || readFileSync(`/proc/${proc}/comm`, "utf8").trim() !== "sunshine")
        continue;
      for (const item of readFileSync(`/proc/${proc}/environ`, "utf8").split("\0")) {
        const at = item.indexOf("="),
          key = item.slice(0, at);
        if (["DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS"].includes(key))
          env[key] = item.slice(at + 1);
      }
      break;
    } catch {
      /* a process may exit during inspection */
    }
  }
  return {
    node_id: crypto.randomUUID(),
    machine: hostname(),
    socket: join(home, ".local/state/kite/node/host.sock"),
    backend: join(home, ".local/lib/kite/host.js"),
    herdr,
    bun: realpathSync(bun),
    systemctl: which("systemctl") || "/usr/bin/systemctl",
    environment: env,
    services: [
      { name: "Herdr", unit: "herdr.service", external: true },
      { name: "Sunshine", unit: "app-dev.lizardbyte.app.Sunshine.service", external: true },
    ],
  };
}
export async function install(
  options: InstallOptions,
  dependencies: InstallDependencies = {},
): Promise<ObjectValue> {
  const root = resolve(options.packageRoot),
    home = resolve(options.home || homedir()),
    run = dependencies.run || command,
    environment = dependencies.environment || process.env;
  const binary = realpathSync(options.binary || join(root, "kite-node")),
    expected = digest(binary),
    manifestPath = join(root, "host-manifest.json");
  const manifest = existsSync(manifestPath)
    ? object(parse(readFileSync(manifestPath, "utf8")))
    : null;
  if (manifest) check(expected === manifest.binarySha256, "host binary checksum mismatch");
  const version = string(
    object(parse((await run([binary, "version"], { timeout: 5000 })).stdout)).version,
  );
  check(/^[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(version), "invalid release version");
  const config = configuration(home, dependencies.which || Bun.which, environment),
    services = array(config.services).map(object);
  check(
    services.every((s) => /^[A-Za-z0-9@_.][A-Za-z0-9@_.-]*\.service$/.test(string(s.unit))),
    "invalid service unit",
  );
  const socket = string(config.socket),
    nodeLink = join(home, ".local/bin/kite-node"),
    argv = [nodeLink, "--socket", socket];
  let migration = false;
  if (options.enable) {
    const active = await run(
      ["systemctl", "--user", "show", "kite-node.service", "--property=ActiveState", "--value"],
      { timeout: 5000, allowFailure: true },
    );
    if (active.stdout.trim() === "active") {
      try {
        migration = !object(parse((await run([...argv, "api-version"], { timeout: 6000 })).stdout))
          .liveUpgrade;
      } catch {
        migration = true;
      }
    }
    check(
      !migration || options.migrateNode,
      "running node has no live handover; use --migrate-node for an intentional maintenance migration",
    );
  }
  const files = new Map<string, Uint8Array>([
    ["kite-desktop", readFileSync(options.desktopBinary || join(root, "kite-desktop"))],
    ["strings.json", readFileSync(join(root, "strings.json"))],
  ]);
  if (manifest)
    check(
      sha256(files.get("kite-desktop") || new Uint8Array()) === manifest.desktopSha256,
      "native desktop binary checksum mismatch",
    );
  const skill = readFileSync(join(root, "skills/kite/SKILL.md")),
    skillDigest = sha256(skill),
    targets = (options.skillDirs || []).map((path) => join(expand(path, home), "kite"));
  files.set("skills/kite/SKILL.md", skill);
  for (const target of targets) {
    const existing = join(target, "SKILL.md"),
      marker = join(target, ".kite-source.sha256");
    if (existsSync(existing) && !readFileSync(existing).equals(skill))
      check(
        existsSync(marker) && digest(existing) === readFileSync(marker, "utf8").trim(),
        `existing Kite skill has local changes: ${existing}`,
      );
  }
  const alias = join(home, ".local/bin/kite");
  if (exists(alias))
    check(
      lstatSync(alias).isSymbolicLink() && realpathSync(alias) === realpathSync(nodeLink),
      "the kite command belongs to another installation",
    );
  for (const state of ["ready", "degraded", "offline", "busy"])
    files.set(`icons/kite-${state}.png`, readFileSync(join(root, `icons/kite-${state}.png`)));
  const release = join(home, ".local/lib/kite-desktop", version);
  mkdirSync(release, { recursive: true, mode: 0o700 });
  chmodSync(release, 0o700);
  atomicBytes(join(release, "kite-node"), readFileSync(binary));
  chmodSync(join(release, "kite-node"), 0o700);
  for (const [name, bytes] of files) {
    mkdirSync(dirname(join(release, name)), { recursive: true });
    atomicBytes(join(release, name), bytes);
    chmodSync(join(release, name), name === "kite-desktop" ? 0o700 : 0o644);
  }
  link(nodeLink, join(release, "kite-node"));
  link(alias, "kite-node");
  for (const target of targets) {
    mkdirSync(target, { recursive: true });
    atomicBytes(join(target, "SKILL.md"), skill);
    chmodSync(join(target, "SKILL.md"), 0o644);
    atomicBytes(join(target, ".kite-source.sha256"), Buffer.from(`${skillDigest}\n`));
    chmodSync(join(target, ".kite-source.sha256"), 0o644);
  }
  const gui = join(home, ".local/bin/kite-desktop");
  link(gui, join(release, "kite-desktop"));
  const configPath = join(home, ".config/kite/node.json");
  mkdirSync(dirname(configPath), { recursive: true });
  atomicBytes(configPath, Buffer.from(`${JSON.stringify(config, null, 2)}\n`));
  const unit = join(home, ".config/systemd/user/kite-node.service"),
    socketUnit = join(home, ".config/systemd/user/kite-node.socket"),
    units = services.map((s) => string(s.unit)).join(" ");
  const quote = (path: string) =>
    `"${path.replaceAll("\\", "\\\\").replaceAll('"', '\\"').replaceAll("%", "%%")}"`;
  const contents = `[Unit]\nDescription=Kite desktop host node\nRequires=kite-node.socket\nAfter=kite-node.socket network.target ${units}\nWants=${units}\n\n[Service]\nEnvironmentFile=-%h/.config/kite/harness.env\nType=notify\nNotifyAccess=all\nSockets=kite-node.socket\nStandardInput=socket\nStandardOutput=journal\nStandardError=journal\nExecStart=${quote(nodeLink)} serve --config ${quote(configPath)} --activated\nRestart=on-failure\nRestartSec=2\nUMask=0077\nKillMode=process\nTimeoutStartSec=30\n\n[Install]\nWantedBy=default.target\n`;
  const socketContents = `[Unit]\nDescription=Kite private host listener\n\n[Socket]\nListenStream=${socket.replaceAll("\\", "\\\\").replaceAll("%", "%%")}\nSocketMode=0600\nDirectoryMode=0700\nRemoveOnStop=true\n\n[Install]\nWantedBy=sockets.target\n`;
  mkdirSync(dirname(unit), { recursive: true });
  atomicBytes(unit, Buffer.from(contents));
  atomicBytes(socketUnit, Buffer.from(socketContents));
  const desktopQuote = (path: string) =>
    `"${path.replace(/[\\"`$]/g, "\\$&").replaceAll("%", "%%")}"`;
  const desktopEntry = `[Desktop Entry]\nType=Application\nName=Kite\nComment=Kite Linux host services\nExec=${desktopQuote(gui)}\nIcon=${release}/icons/kite-ready.png\nTerminal=false\nCategories=Network;RemoteAccess;\n`;
  const entry = join(home, ".local/share/applications/io.github.wenshiqi0.Kite.desktop");
  mkdirSync(dirname(entry), { recursive: true });
  atomicBytes(entry, Buffer.from(desktopEntry));
  chmodSync(entry, 0o644);
  const autostart = join(home, ".config/autostart/io.github.wenshiqi0.Kite.desktop");
  if (!existsSync(autostart)) {
    mkdirSync(dirname(autostart), { recursive: true });
    atomicBytes(
      autostart,
      Buffer.from(
        desktopEntry.replace(`Exec=${desktopQuote(gui)}`, `Exec=${desktopQuote(gui)} --background`),
      ),
    );
    chmodSync(autostart, 0o644);
  }
  if (options.enable && migration)
    await run(["systemctl", "--user", "stop", "kite-node.service"], { timeout: 30000 });
  await run(["systemctl", "--user", "daemon-reload"], { timeout: 10000 });
  if (options.enable) {
    await run(["systemctl", "--user", "enable", "--now", "kite-node.socket"], { timeout: 10000 });
    await run(["systemctl", "--user", "enable", "--now", "kite-node.service"], { timeout: 30000 });
    if (
      !(await (dependencies.runningNodeMatches || ((hash) => runningNodeMatches(hash, run)))(
        expected,
      ))
    )
      await run([...argv, "upgrade", "--binary", join(release, "kite-node")], { timeout: 65000 });
    await (dependencies.waitForNode || ((cmd, value, hash) => waitForNode(cmd, value, hash, run)))(
      argv,
      version,
      expected,
    );
    await run([...argv, "ensure", "all"], { timeout: 35000 });
    const env = config.environment
      ? Object.fromEntries(
          Object.entries(object(config.environment)).map(([key, value]) => [key, string(value)]),
        )
      : {};
    await (dependencies.refreshGui || ((value) => refreshGui(value, run)))(env);
  }
  return { version, node: nodeLink, desktop: gui, service: unit };
}
if (import.meta.main) {
  try {
    const { values } = parseArgs({
      options: {
        binary: { type: "string" },
        "desktop-binary": { type: "string" },
        "package-root": { type: "string" },
        enable: { type: "boolean" },
        "migrate-node": { type: "boolean" },
        "skill-dir": { type: "string", multiple: true },
      },
      strict: true,
    });
    console.log(
      JSON.stringify(
        await install({
          packageRoot: values["package-root"] || dirname(resolve(process.argv[1] || "")),
          ...(values.binary ? { binary: values.binary } : {}),
          ...(values["desktop-binary"] ? { desktopBinary: values["desktop-binary"] } : {}),
          enable: values.enable || false,
          migrateNode: values["migrate-node"] || false,
          skillDirs: values["skill-dir"] || [],
        }),
      ),
    );
  } catch (error) {
    console.error(error instanceof Error ? error.message : "host installation failed");
    process.exitCode = 1;
  }
}
