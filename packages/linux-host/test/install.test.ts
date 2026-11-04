import { afterEach, expect, test } from "bun:test";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import type { Command } from "../src/command";
import { digest, install, refreshGui, runningNodeMatches } from "../src/install";

const temporary: string[] = [];
afterEach(() => {
  for (const path of temporary.splice(0)) rmSync(path, { recursive: true, force: true });
});
function fixture() {
  const root = mkdtempSync(join(tmpdir(), "kite-install-test-"));
  temporary.push(root);
  const home = join(root, "user"),
    assets = join(root, "package"),
    link = join(home, ".local/bin/kite-node"),
    config = join(home, ".config/kite/node.json"),
    autostart = join(home, ".config/autostart/io.github.wenshiqi0.Kite.desktop");
  for (const path of [assets, dirname(link), dirname(config), dirname(autostart)])
    mkdirSync(path, { recursive: true });
  writeFileSync(join(assets, "kite-node"), "new binary");
  writeFileSync(join(assets, "kite-desktop"), "new native desktop");
  writeFileSync(link, "old binary");
  writeFileSync(
    config,
    JSON.stringify({ socket: "/private/custom.sock", services: [], environment: {} }),
  );
  writeFileSync(autostart, "[Desktop Entry]\nHidden=true\n");
  for (const name of [
    "strings.json",
    ...["ready", "degraded", "offline", "busy"].map((state) => `icons/kite-${state}.png`),
  ]) {
    mkdirSync(dirname(join(assets, name)), { recursive: true });
    copyFileSync(resolve("desktop/linux", name), join(assets, name));
  }
  mkdirSync(join(assets, "skills/kite"), { recursive: true });
  copyFileSync(resolve("skills/kite/SKILL.md"), join(assets, "skills/kite/SKILL.md"));
  return { root, home, assets, link, config, autostart };
}
test("an interrupted installation retries against the running binary and keeps user preferences", async () => {
  const f = fixture(),
    calls: string[][] = [];
  let interrupt = true,
    waits = 0,
    refreshes = 0;
  const run: Command = async (argv) => {
    calls.push(argv);
    if (interrupt && argv.includes("daemon-reload")) throw new Error("interrupted daemon-reload");
    return {
      code: 0,
      stderr: "",
      stdout: argv.includes("version")
        ? '{"version":"test-alpha.1"}'
        : argv.includes("api-version")
          ? '{"liveUpgrade":true}'
          : argv.includes("--property=ActiveState")
            ? "active"
            : "",
    };
  };
  const options = { packageRoot: f.assets, home: f.home, enable: true },
    dependencies = {
      run,
      which: () => process.execPath,
      runningNodeMatches: async () => false,
      waitForNode: async () => {
        waits++;
      },
      refreshGui: async () => {
        refreshes++;
      },
    };
  await expect(install(options, dependencies)).rejects.toThrow("interrupted");
  expect(readFileSync(f.link, "utf8")).toBe("new binary");
  interrupt = false;
  calls.length = 0;
  await install(options, dependencies);
  expect(calls).toContainEqual([
    f.link,
    "--socket",
    "/private/custom.sock",
    "upgrade",
    "--binary",
    join(f.home, ".local/lib/kite-desktop/test-alpha.1/kite-node"),
  ]);
  expect(calls.some((argv) => argv.includes("restart") || argv.includes("stop"))).toBe(false);
  expect(calls).toContainEqual([f.link, "--socket", "/private/custom.sock", "ensure", "all"]);
  expect(waits).toBe(1);
  expect(refreshes).toBe(1);
  expect(JSON.parse(readFileSync(f.config, "utf8")).socket).toBe("/private/custom.sock");
  expect(readFileSync(f.autostart, "utf8")).toContain("Hidden=true");
});
test.skipIf(process.platform !== "linux")(
  "reads the running executable through proc rather than trusting an installed symlink",
  async () => {
    const run: Command = async () => ({ code: 0, stdout: String(process.pid), stderr: "" });
    expect(await runningNodeMatches(digest("/proc/self/exe"), run)).toBe(true);
    expect(await runningNodeMatches("0".repeat(64), run)).toBe(false);
  },
);
test("a GUI update only sends the upgrade action", async () => {
  const calls: string[][] = [],
    run: Command = async (argv) => {
      calls.push(argv);
      return { code: 0, stdout: "", stderr: "" };
    };
  await refreshGui({ DBUS_SESSION_BUS_ADDRESS: "unix:path=/owned/bus" }, run);
  expect(calls).toHaveLength(1);
  expect(calls[0]?.[0]).toBe("gdbus");
  expect(calls[0]).toContain("upgrade");
});
test("a pre-handover node is refused before changing installed files", async () => {
  const f = fixture(),
    original = readFileSync(f.config);
  const run: Command = async (argv) => ({
    code: 0,
    stdout: argv.includes("version")
      ? '{"version":"test-alpha.1"}'
      : argv[0] === "systemctl"
        ? "active"
        : '{"api":1}',
    stderr: "",
  });
  await expect(
    install(
      { packageRoot: f.assets, home: f.home, enable: true },
      { run, which: () => process.execPath },
    ),
  ).rejects.toThrow("--migrate-node");
  expect(readFileSync(f.link, "utf8")).toBe("old binary");
  expect(readFileSync(f.config)).toEqual(original);
  expect(existsSync(join(f.home, ".config/systemd"))).toBe(false);
});
test("a user-edited skill and mismatching package digest are preserved", async () => {
  const f = fixture(),
    skills = join(f.home, ".agents/skills"),
    existing = join(skills, "kite/SKILL.md");
  mkdirSync(dirname(existing), { recursive: true });
  writeFileSync(existing, "owned custom skill");
  const run: Command = async () => ({ code: 0, stdout: '{"version":"test-alpha.1"}', stderr: "" });
  await expect(
    install(
      { packageRoot: f.assets, home: f.home, skillDirs: [skills] },
      { run, which: () => process.execPath },
    ),
  ).rejects.toThrow("local changes");
  expect(readFileSync(existing, "utf8")).toBe("owned custom skill");
  expect(readFileSync(f.link, "utf8")).toBe("old binary");
  writeFileSync(
    join(f.assets, "host-manifest.json"),
    JSON.stringify({ binarySha256: "0".repeat(64) }),
  );
  await expect(
    install({ packageRoot: f.assets, home: f.home }, { run, which: () => process.execPath }),
  ).rejects.toThrow("checksum mismatch");
  expect(readFileSync(f.link, "utf8")).toBe("old binary");
});
