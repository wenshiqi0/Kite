#!/usr/bin/env bun
/** Opt-in, disposable local-daemon acceptance; never copies a user's login. */
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { parseArgs } from "node:util";
import { array, check, object, parse, string } from "@kite/protocol";
import { command } from "../../src/command";
import { deployDocker } from "../../src/docker";

const { values } = parseArgs({
  options: {
    image: { type: "string" },
    evidence: { type: "string" },
    "stream-probe": { type: "string" },
  },
  strict: true,
});
check(values.image, "--image is required; use an already-built image");
const project = `kite-docker-qa-${process.pid}-${Date.now()}`,
  evidence = resolve(values.evidence || `target/${project}`),
  key = `${evidence}/device`,
  compose = [
    "docker",
    "compose",
    "--project-name",
    project,
    "--file",
    resolve(import.meta.dir, "../../../../deploy/docker/compose.yaml"),
  ],
  sshPort = 23000 + Math.floor(Math.random() * 1000),
  sunshinePort = 33000 + Math.floor(Math.random() * 1000),
  env = {
    ...process.env,
    KITE_IMAGE: values.image,
    KITE_SSH_PORT: String(sshPort),
    KITE_SUNSHINE_PORT: String(sunshinePort),
  };
mkdirSync(evidence, { recursive: true, mode: 0o700 });
check(
  !process.env.DOCKER_HOST || process.env.DOCKER_HOST.startsWith("unix://"),
  "the QA probe refuses a remote DOCKER_HOST",
);
const context = object(array(parse((await command(["docker", "context", "inspect"])).stdout))[0]);
check(
  string(object(object(context.Endpoints).docker).Host).startsWith("unix://"),
  "the QA probe requires a local Unix-socket Docker context",
);
await command(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", key]);
let container = "";
const exec = async (script: string) =>
  (
    await command([
      "docker",
      "exec",
      "--user",
      "kite",
      "--env",
      "XDG_RUNTIME_DIR=/run/user/1000",
      "--env",
      "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus",
      "--env",
      "DISPLAY=:0",
      "--env",
      "XAUTHORITY=/run/kite/Xauthority",
      container,
      "/bin/bash",
      "-euc",
      script,
    ])
  ).stdout.trim();
const ssh = [
  "ssh",
  "-i",
  key,
  "-o",
  "IdentitiesOnly=yes",
  "-o",
  "BatchMode=yes",
  "-o",
  "StrictHostKeyChecking=yes",
  "-o",
  `UserKnownHostsFile=${evidence}/known_hosts`,
  "-p",
  String(sshPort),
  "kite@127.0.0.1",
];
try {
  console.error(`Starting owned project ${project}`);
  const deployed = await deployDocker({
    image: values.image,
    project,
    authorizedKeys: `${key}.pub`,
    sshPort,
    sunshinePort,
  });
  container = deployed.container;
  const hostKey = (
    await command([
      "docker",
      "exec",
      container,
      "cat",
      "/var/lib/kite-container/ssh/ssh_host_ed25519_key.pub",
    ])
  ).stdout;
  writeFileSync(`${evidence}/known_hosts`, `[127.0.0.1]:${sshPort} ${hostKey}`);
  writeFileSync(`${evidence}/host.pub`, hostKey);
  const status = object(
    parse((await command([...ssh, "/home/kite/.local/bin/kite", "status"])).stdout),
  );
  check(status.health === "ready", "Node is not ready through authenticated SSH");
  await exec(
    "if command -v kite-desktop >/dev/null; then exit 1; fi; test ! -e /opt/kite-host/kite-desktop; test ! -e /home/kite/.config/autostart/io.github.wenshiqi0.Kite.desktop",
  );
  await command([...ssh, "/usr/local/bin/herdr", "agent", "list"]);
  const desktop = object(parse(await exec("/home/kite/.local/bin/kite desktop status")));
  check(desktop.available, "X11 desktop is unavailable");
  const shot = object(parse(await exec("/home/kite/.local/bin/kite desktop screenshot")));
  await command(["docker", "cp", `${container}:${string(shot.path)}`, `${evidence}/desktop.png`]);
  const png = readFileSync(`${evidence}/desktop.png`);
  check(
    png.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])) &&
      png.readUInt32BE(16) === 1920 &&
      png.readUInt32BE(20) === 1080,
    "desktop did not produce an actual 1080p PNG",
  );
  await exec("pactl -s unix:/run/user/1000/pulse/native list short sinks | grep kite_audio");
  check((await exec("fc-list :lang=zh family")).includes("Noto"), "Chinese fonts are missing");
  check((await exec("locale")).includes("C.UTF-8"), "the default locale is not English UTF-8");
  const limit = await exec(
    "systemd-run --user --quiet --wait --pipe --collect -p MemoryMax=64M /bin/bash -euc 'path=$(cut -d: -f3 /proc/self/cgroup); cat /sys/fs/cgroup$path/memory.max'",
  );
  check(limit === "67108864", "the real delegated cgroup memory limit was not installed");
  if (values["stream-probe"]) {
    const fixture = await Bun.build({
      entrypoints: [resolve(import.meta.dir, "desktop-fixture.ts")],
      target: "bun",
      format: "esm",
      packages: "bundle",
    });
    check(fixture.success && fixture.outputs[0], "failed to bundle the owned input fixture");
    await Bun.write(`${evidence}/input-fixture.js`, fixture.outputs[0]);
    await exec("mkdir -p /home/kite/.local/state/kite/docker-qa");
    await command(
      [
        "docker",
        "exec",
        "--interactive",
        "--user",
        "kite",
        container,
        "bun",
        "-e",
        "await Bun.write(Bun.argv[1], await Bun.stdin.text());",
        "/home/kite/.local/state/kite/docker-qa/input-fixture.js",
      ],
      { input: await fixture.outputs[0].text() },
    );
    const password = crypto.randomUUID();
    await command([
      "docker",
      "exec",
      "--user",
      "kite",
      container,
      "sunshine",
      "--creds",
      "kite-qa",
      password,
    ]);
    await command([
      "docker",
      "exec",
      "--user",
      "kite",
      container,
      "bun",
      "-e",
      'await Bun.write("/home/kite/.config/sunshine/admin-credentials.json", Bun.argv[1]);',
      JSON.stringify({ username: "kite-qa", password }),
    ]);
    await exec(
      "chmod 600 /home/kite/.config/sunshine/admin-credentials.json; systemctl --user restart app-dev.lizardbyte.app.Sunshine.service",
    );
    await exec(
      "systemd-run --user --quiet --collect --unit=kite-docker-input-fixture --setenv=DISPLAY=:0 --setenv=XAUTHORITY=/run/kite/Xauthority /usr/local/bin/bun /home/kite/.local/state/kite/docker-qa/input-fixture.js --state /home/kite/.local/state/kite/docker-qa/input-state.json",
    );
    const stream = await command(
      [
        resolve(values["stream-probe"]),
        String(sshPort),
        String(sunshinePort),
        key,
        `${evidence}/host.pub`,
      ],
      { timeout: 90000 },
    );
    writeFileSync(`${evidence}/stream.txt`, stream.stdout + stream.stderr);
    check(
      stream.stdout.includes("sunshine_native_mouse_keyboard=PASS"),
      "native streaming/input probe did not pass",
    );
    console.error("Real Sunshine video/audio packets and native mouse/keyboard passed");
  }
  await exec("printf retained > /home/kite/docker-qa-marker");
  console.error("SSH, Herdr, X11, audio, fonts, locale and cgroup passed; recreating container");
  await command([...compose, "down"], { env, timeout: 90000 });
  const replacement = await deployDocker({
    image: values.image,
    project,
    sshPort,
    sunshinePort,
  });
  container = replacement.container;
  const restored = object(
    parse((await command([...ssh, "/home/kite/.local/bin/kite", "status"])).stdout),
  );
  check(restored.nodeId === status.nodeId, "Kite node identity changed on container replacement");
  check(replacement.fingerprint === deployed.fingerprint, "SSH trust changed on replacement");
  check((await exec("cat /home/kite/docker-qa-marker")) === "retained", "workspace file was lost");
  const result = {
    image: values.image,
    project,
    nodeId: status.nodeId,
    ssh: true,
    herdr: true,
    serverOnly: true,
    desktopPng: { width: 1920, height: 1080 },
    virtualAudio: true,
    chineseFonts: true,
    englishLocale: true,
    cgroupMemoryMax: Number(limit),
    replacementPreservedIdentity: true,
    sunshineTransportAndInput: Boolean(values["stream-probe"]),
    modelRequests: 0,
  };
  writeFileSync(`${evidence}/result.json`, `${JSON.stringify(result, null, 2)}\n`);
  console.log(JSON.stringify(result));
} catch (error) {
  if (!container)
    container = (
      await command([...compose, "ps", "--all", "--quiet", "host"], { env })
    ).stdout.trim();
  if (container) {
    const journal = await command(
      ["docker", "exec", container, "journalctl", "-b", "--no-pager", "-n", "250"],
      { allowFailure: true },
    );
    writeFileSync(`${evidence}/failure-journal.txt`, journal.stdout + journal.stderr);
  }
  throw error;
} finally {
  // The prefix is owned by this invocation; normal deployment never removes data.
  check(project.startsWith(`kite-docker-qa-${process.pid}-`), "unexpected cleanup project");
  try {
    await command([...compose, "down", "--volumes", "--remove-orphans"], {
      env,
      timeout: 90000,
    });
  } finally {
    for (const path of [key, `${key}.pub`, `${evidence}/known_hosts`])
      rmSync(path, { force: true });
  }
}
