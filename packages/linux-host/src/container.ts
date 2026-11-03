#!/usr/bin/env bun
/** Prepare server-only state; no desktop application, tray or client installer. */
import { lstatSync, mkdirSync, readFileSync, renameSync, symlinkSync, unlinkSync } from "node:fs";
import { homedir, hostname } from "node:os";
import { join, resolve } from "node:path";
import { check, type ObjectValue, object, parse } from "@kite/protocol";
import { atomicBytes } from "@kite/runtime";

export function prepareContainer(home = homedir()): ObjectValue {
  home = resolve(home);
  const configPath = join(home, ".config/kite/node.json");
  let config: ObjectValue;
  try {
    config = object(parse(readFileSync(configPath, "utf8")));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    config = { node_id: crypto.randomUUID(), machine: hostname() };
  }
  const environment = {
    ...(config.environment ? object(config.environment) : {}),
    PATH: `${home}/.local/bin:/opt/kite-tools/bin:/usr/local/bin:/usr/bin:/bin`,
    XDG_RUNTIME_DIR: "/run/user/1000",
    DISPLAY: ":0",
    XAUTHORITY: "/run/kite/Xauthority",
    DBUS_SESSION_BUS_ADDRESS: "unix:path=/run/user/1000/bus",
  };
  const result = {
    ...config,
    socket: join(home, ".local/state/kite/node/host.sock"),
    backend: join(home, ".local/lib/kite/host.js"),
    herdr: "/usr/local/bin/herdr",
    bun: "/usr/local/bin/bun",
    systemctl: "/usr/bin/systemctl",
    environment,
    services: [
      { name: "Herdr", unit: "herdr.service", external: true },
      { name: "Sunshine", unit: "app-dev.lizardbyte.app.Sunshine.service", external: true },
    ],
  };
  const bin = join(home, ".local/bin");
  mkdirSync(bin, { recursive: true, mode: 0o700 });
  for (const name of ["kite", "kite-node"]) {
    const path = join(bin, name);
    try {
      check(lstatSync(path).isSymbolicLink(), `${name} belongs to another installation`);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
    const pending = `${path}.new`;
    try {
      unlinkSync(pending);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
    symlinkSync("/usr/local/bin/kite-node", pending);
    renameSync(pending, path);
  }
  for (const path of [".config/kite", ".local/lib/kite", ".local/state/kite/node"])
    mkdirSync(join(home, path), { recursive: true, mode: 0o700 });
  atomicBytes(configPath, Buffer.from(`${JSON.stringify(result, null, 2)}\n`));
  return result;
}

if (import.meta.main) {
  try {
    const config = prepareContainer();
    console.log(JSON.stringify({ nodeId: config.node_id, role: "server" }));
  } catch (error) {
    console.error(error instanceof Error ? error.message : "container setup failed");
    process.exitCode = 1;
  }
}
