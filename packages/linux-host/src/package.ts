#!/usr/bin/env bun
/** Reproducible allowlisted distribution; user files never enter the archive. */
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { gzipSync } from "node:zlib";
import { check } from "@kite/protocol";
import { sha256 } from "@kite/runtime";
import { command } from "./command";

const REPOSITORY = resolve(import.meta.dir, "../../..");
export interface PackageOptions {
  binary: string;
  output: string;
  version: string;
  commit: string;
  desktopBinary?: string;
  repository?: string;
}
function tar(files: Map<string, { bytes: Uint8Array; mode: number }>, prefix: string): Buffer {
  const parts: Uint8Array[] = [];
  for (const [path, { bytes, mode }] of [...files].sort(([a], [b]) =>
    a < b ? -1 : a > b ? 1 : 0,
  )) {
    const name = `${prefix}/${path}`,
      header = Buffer.alloc(512);
    check(
      Buffer.byteLength(name) <= 100 && !name.includes("\0"),
      "distribution path exceeds USTAR limit",
    );
    header.write(name, 0, 100);
    const octal = (value: number, at: number, width: number) =>
      header.write(`${value.toString(8).padStart(width - 1, "0")}\0`, at, width);
    octal(mode, 100, 8);
    octal(0, 108, 8);
    octal(0, 116, 8);
    octal(bytes.length, 124, 12);
    octal(0, 136, 12);
    header.fill(32, 148, 156);
    header[156] = 48;
    header.write("ustar\0", 257);
    header.write("00", 263);
    const checksum = header.reduce((total, byte) => total + byte, 0);
    header.write(`${checksum.toString(8).padStart(6, "0")}\0 `, 148, 8);
    parts.push(header, bytes, Buffer.alloc((512 - (bytes.length % 512)) % 512));
  }
  return Buffer.concat([...parts, Buffer.alloc(1024)]);
}
export async function packageHost(
  options: PackageOptions,
): Promise<Record<string, string | number>> {
  check(/^[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(options.version), "invalid distribution version");
  const repository = resolve(options.repository || REPOSITORY),
    assets = join(repository, "desktop/linux"),
    binary = resolve(options.binary),
    desktop = options.desktopBinary || join(dirname(binary), "kite-desktop");
  const nodeBytes = readFileSync(binary),
    desktopBytes = readFileSync(desktop);
  const scratch = mkdtempSync(join(tmpdir(), "kite-installer-bundle-"));
  let installer: Uint8Array;
  try {
    const output = join(scratch, "install.js");
    await command(
      [
        process.execPath,
        "build",
        "--target=bun",
        "--format=esm",
        "--packages=bundle",
        join(repository, "packages/linux-host/src/install.ts"),
        "--outfile",
        output,
      ],
      { cwd: repository, timeout: 30000 },
    );
    installer = readFileSync(output);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
  const manifest = {
    runtime: "bun",
    minimumBunVersion: "1.3.14",
    installer: "install.js",
    desktopSha256: sha256(desktopBytes),
    version: options.version,
    sourceCommit: options.commit,
    target: "x86_64-unknown-linux-gnu",
    nodeApi: 1,
    backendProtocol: 5,
    harnessApi: 2,
    docsApi: 1,
    desktopApi: 1,
    resourceApi: 1,
    binarySha256: sha256(nodeBytes),
  };
  const files = new Map<string, { bytes: Uint8Array; mode: number }>([
    ["kite-node", { bytes: nodeBytes, mode: 0o755 }],
    ["kite-desktop", { bytes: desktopBytes, mode: 0o755 }],
    ["install.js", { bytes: installer, mode: 0o755 }],
    [
      "host-manifest.json",
      { bytes: Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`), mode: 0o644 },
    ],
    ["LICENSE", { bytes: readFileSync(join(repository, "LICENSE")), mode: 0o644 }],
    ["strings.json", { bytes: readFileSync(join(assets, "strings.json")), mode: 0o644 }],
    [
      "skills/kite/SKILL.md",
      { bytes: readFileSync(join(repository, "skills/kite/SKILL.md")), mode: 0o644 },
    ],
  ]);
  for (const state of ["ready", "degraded", "offline", "busy"]) {
    const name = `icons/kite-${state}.png`;
    files.set(name, { bytes: readFileSync(join(assets, name)), mode: 0o644 });
  }
  mkdirSync(dirname(resolve(options.output)), { recursive: true });
  writeFileSync(
    options.output,
    gzipSync(tar(files, `kite-host-${options.version}-linux-x86_64`), { level: 9 }),
  );
  return manifest;
}
if (import.meta.main) {
  try {
    const { values } = parseArgs({
      options: {
        binary: { type: "string" },
        output: { type: "string" },
        version: { type: "string" },
        commit: { type: "string" },
      },
      strict: true,
    });
    check(
      values.binary && values.output && values.version && values.commit,
      "binary, output, version and commit are required",
    );
    console.log(
      JSON.stringify(
        await packageHost({
          binary: values.binary,
          output: values.output,
          version: values.version,
          commit: values.commit,
        }),
      ),
    );
  } catch (error) {
    console.error(error instanceof Error ? error.message : "host packaging failed");
    process.exitCode = 1;
  }
}
