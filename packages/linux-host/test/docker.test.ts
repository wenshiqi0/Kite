import { expect, test } from "bun:test";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Command } from "../src/command";
import { prepareContainer } from "../src/container";
import { deployDocker } from "../src/docker";

test("container setup installs only server commands and preserves node identity and settings", () => {
  const home = mkdtempSync(join(tmpdir(), "kite-container-test-"));
  try {
    const first = prepareContainer(home);
    expect(readdirSync(join(home, ".local/bin")).sort()).toEqual(["kite", "kite-node"]);
    expect(existsSync(join(home, ".config/autostart"))).toBe(false);
    const config = join(home, ".config/kite/node.json");
    writeFileSync(config, JSON.stringify({ ...first, agent_memory_limit_bytes: 1073741824 }));
    const second = prepareContainer(home);
    expect(second.node_id).toBe(first.node_id);
    expect(second.agent_memory_limit_bytes).toBe(1073741824);
    writeFileSync(config, "broken retained configuration");
    expect(() => prepareContainer(home)).toThrow();
    expect(readFileSync(config, "utf8")).toBe("broken retained configuration");
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

test("container setup does not replace an unrelated user executable", () => {
  const home = mkdtempSync(join(tmpdir(), "kite-container-command-test-"));
  try {
    mkdirSync(join(home, ".local/bin"), { recursive: true });
    writeFileSync(join(home, ".local/bin/kite"), "owned elsewhere");
    expect(() => prepareContainer(home)).toThrow("another installation");
    expect(readFileSync(join(home, ".local/bin/kite"), "utf8")).toBe("owned elsewhere");
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

test("remote deployment copies public keys to the daemon and waits for health", async () => {
  const root = mkdtempSync(join(tmpdir(), "kite-docker-test-")),
    path = join(root, "phone.pub"),
    calls: { argv: string[]; env?: NodeJS.ProcessEnv }[] = [];
  writeFileSync(path, "ssh-ed25519 AAAA phone\n");
  let transfer: string | undefined;
  const run: Command = async (argv, options = {}) => {
    calls.push({ argv, ...(options.env ? { env: options.env } : {}) });
    let stdout = "";
    if (argv.includes("info"))
      stdout = JSON.stringify({ OSType: "linux", CgroupVersion: "2", SecurityOptions: [] });
    if (argv.includes("ps")) stdout = "a".repeat(64);
    if (argv.includes("{{json .State}}"))
      stdout = JSON.stringify({ Running: true, Health: { Status: "healthy" } });
    if (argv.includes("cp")) {
      transfer = argv[argv.indexOf("cp") + 1];
      expect(transfer && existsSync(transfer)).toBe(true);
      expect(argv.at(-1)).toEndWith(":/var/lib/kite-container/authorized_keys.import");
    }
    if (argv.includes("ssh-keygen")) stdout = "256 SHA256:public kite (ED25519)\n";
    return { code: argv.includes("image") && argv.includes("inspect") ? 1 : 0, stdout, stderr: "" };
  };
  try {
    const result = await deployDocker(
      { image: "registry.test/kite@sha256:1234", context: "my-server", authorizedKeys: path },
      run,
    );
    expect(result.fingerprint).toContain("SHA256:public");
    expect(
      calls.every(({ argv }) => argv.slice(0, 3).join(" ") === "docker --context my-server"),
    ).toBe(true);
    expect(calls.some(({ argv }) => argv.includes("pull"))).toBe(true);
    const start = calls.find(({ argv }) => argv.includes("up"));
    expect(start?.argv).toContain("--wait");
    expect(start?.env?.KITE_IMAGE).toBe("registry.test/kite@sha256:1234");
    expect(calls.some(({ argv }) => argv.includes("--volumes"))).toBe(false);
    expect(transfer && existsSync(transfer)).toBe(false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("unsupported daemons and invalid ports fail before container changes", async () => {
  let mutations = 0;
  const run: Command = async (argv) => {
    if (!argv.includes("info")) mutations++;
    return {
      code: 0,
      stdout: JSON.stringify({
        OSType: "linux",
        CgroupVersion: "2",
        SecurityOptions: ["rootless"],
      }),
      stderr: "",
    };
  };
  await expect(deployDocker({ image: "kite:qa" }, run)).rejects.toThrow("rootful");
  await expect(deployDocker({ image: "kite:qa", sunshinePort: 65535 }, run)).rejects.toThrow(
    "base port",
  );
  await expect(deployDocker({ image: "kite:qa", sshPort: 48090 }, run)).rejects.toThrow("overlap");
  expect(mutations).toBe(0);
});

test("a failed start is reported without deleting persistent state", async () => {
  const calls: string[][] = [];
  const run: Command = async (argv) => {
    calls.push(argv);
    if (argv.includes("up")) throw new Error("unhealthy");
    return {
      code: 0,
      stdout: argv.includes("info")
        ? JSON.stringify({ OSType: "linux", CgroupVersion: "2", SecurityOptions: [] })
        : argv.includes("ps")
          ? "b".repeat(64)
          : "",
      stderr: "",
    };
  };
  await expect(deployDocker({ image: "kite:qa" }, run)).rejects.toThrow("unhealthy");
  expect(calls.some((argv) => argv.includes("down") || argv.includes("rm"))).toBe(false);
});
