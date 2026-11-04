#!/usr/bin/env bun
/** Deploy a complete host through a local or SSH-backed Docker context. */
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { check, object, parse } from "@kite/protocol";
import { type Command, command } from "./command";

export interface DockerOptions {
  image: string;
  context?: string;
  project?: string;
  authorizedKeys?: string;
  sshPort?: number;
  sunshinePort?: number;
  pull?: boolean;
}

export async function deployDocker(options: DockerOptions, run: Command = command) {
  check(options.image && !/\s/.test(options.image), "an image reference is required");
  const project = options.project || "kite",
    sshPort = options.sshPort ?? 2222,
    sunshinePort = options.sunshinePort ?? 48089;
  check(/^[a-z0-9][a-z0-9_-]*$/.test(project), "invalid Compose project name");
  check(Number.isInteger(sshPort) && sshPort >= 1024 && sshPort <= 65535, "invalid SSH port");
  check(
    Number.isInteger(sunshinePort) && sunshinePort >= 1029 && sunshinePort <= 65514,
    "invalid Sunshine base port",
  );
  check(
    ![-5, 0, 1, 21].some((offset) => sunshinePort + offset === sshPort),
    "SSH and Sunshine ports overlap",
  );
  let keys: string | undefined;
  if (options.authorizedKeys) {
    keys = readFileSync(resolve(options.authorizedKeys), "utf8");
    check(
      keys.split("\n").some((line) => /^ssh-(ed25519|rsa) [A-Za-z0-9+/]+={0,3}( |$)/.test(line)),
      "authorized-keys must contain an OpenSSH public key",
    );
    check(!keys.includes("PRIVATE KEY"), "private keys cannot be deployed");
  }
  const docker = ["docker", ...(options.context ? ["--context", options.context] : [])],
    env = {
      ...process.env,
      KITE_IMAGE: options.image,
      KITE_SSH_PORT: String(sshPort),
      KITE_SUNSHINE_PORT: String(sunshinePort),
    },
    compose = [
      ...docker,
      "compose",
      "--project-name",
      project,
      "--file",
      resolve(import.meta.dir, "../../../deploy/docker/compose.yaml"),
    ];
  const info = object(parse((await run([...docker, "info", "--format", "{{json .}}"])).stdout));
  check(info.OSType === "linux" && info.CgroupVersion === "2", "Linux cgroup v2 is required");
  check(!JSON.stringify(info.SecurityOptions).includes("rootless"), "rootful Docker is required");
  await run([...docker, "compose", "version"]);
  const existing = await run([...docker, "image", "inspect", options.image], {
    allowFailure: true,
  });
  if (options.pull || existing.code)
    await run([...docker, "pull", options.image], { timeout: 600000 });
  await run(
    [
      ...docker,
      "run",
      "--rm",
      "--network",
      "none",
      "--entrypoint",
      "/bin/sh",
      "--mount",
      "type=bind,src=/run/udev,dst=/host-udev,readonly",
      "--mount",
      "type=bind,src=/dev/input,dst=/host-input,readonly",
      options.image,
      "-ec",
      'test -d /host-udev/data; test -e /sys/class/misc/uinput/dev && test -e /sys/class/misc/uhid/dev || { echo "load the server uinput and uhid modules before deployment" >&2; exit 1; }; for executable in xfce4-session xfce4-panel xfwm4 fcitx5; do command -v "$executable" >/dev/null || exit 1; done',
    ],
    { timeout: 30000 },
  );
  // Compose bind-mounted secrets refer to the daemon's filesystem. docker cp also
  // works over SSH contexts and transfers only the supplied public-key material.
  await run([...compose, "create", "--no-build", "host"], { env, timeout: 120000 });
  const container = (
    await run([...compose, "ps", "--all", "--quiet", "host"], { env })
  ).stdout.trim();
  check(/^[a-f0-9]{12,64}$/.test(container), "expected exactly one Kite container");
  const scratch = mkdtempSync(join(tmpdir(), "kite-docker-keys-"));
  try {
    if (keys) {
      const path = join(scratch, "authorized_keys");
      writeFileSync(path, keys, { mode: 0o600 });
      await run([
        ...docker,
        "cp",
        path,
        `${container}:/var/lib/kite-container/authorized_keys.import`,
      ]);
    }
    await run([...compose, "up", "--detach", "--no-build", "--wait", "--wait-timeout", "240"], {
      env,
      timeout: 260000,
    });
    const state = object(
      parse((await run([...docker, "inspect", "--format", "{{json .State}}", container])).stdout),
    );
    check(
      state.Running === true && object(state.Health).Status === "healthy",
      "Kite container is not healthy; inspect its journal before retrying",
    );
    if (keys) await run([...docker, "exec", container, "kite-container", "authorize"]);
    const fingerprint = (
      await run([
        ...docker,
        "exec",
        container,
        "ssh-keygen",
        "-lf",
        "/var/lib/kite-container/ssh/ssh_host_ed25519_key.pub",
      ])
    ).stdout.trim();
    return { project, container, user: "kite", sshPort, sunshinePort, fingerprint };
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  try {
    const { values } = parseArgs({
      options: {
        image: { type: "string" },
        context: { type: "string" },
        project: { type: "string" },
        "authorized-keys": { type: "string" },
        "ssh-port": { type: "string" },
        "sunshine-port": { type: "string" },
        pull: { type: "boolean" },
      },
      strict: true,
    });
    check(values.image, "--image is required");
    console.log(
      JSON.stringify(
        await deployDocker({
          image: values.image,
          ...(values.context ? { context: values.context } : {}),
          ...(values.project ? { project: values.project } : {}),
          ...(values["authorized-keys"] ? { authorizedKeys: values["authorized-keys"] } : {}),
          ...(values["ssh-port"] ? { sshPort: Number(values["ssh-port"]) } : {}),
          ...(values["sunshine-port"] ? { sunshinePort: Number(values["sunshine-port"]) } : {}),
          pull: values.pull ?? false,
        }),
      ),
    );
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Docker deployment failed");
    process.exitCode = 1;
  }
}
