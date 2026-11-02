import { afterAll, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gunzipSync } from "node:zlib";
import { command } from "../src/command";
import { packageHost } from "../src/package";

const root = mkdtempSync(join(tmpdir(), "kite-package-test-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
test("produces identical archives with only distribution files and a standalone Bun installer", async () => {
  const binary = join(root, "kite-node"),
    desktop = join(root, "kite-desktop"),
    a = join(root, "a.tar.gz"),
    b = join(root, "b.tar.gz"),
    prefix = "kite-host-0.2.0-alpha.56-linux-x86_64";
  writeFileSync(binary, "controlled binary");
  writeFileSync(desktop, "controlled native desktop");
  for (const output of [a, b])
    await packageHost({ binary, output, version: "0.2.0-alpha.56", commit: "a".repeat(40) });
  expect(readFileSync(a)).toEqual(readFileSync(b));
  const names = (await command(["tar", "-tzf", a])).stdout
    .trim()
    .split("\n")
    .map((name) => name.slice(prefix.length + 1));
  expect(names.sort()).toEqual(
    [
      "kite-node",
      "kite-desktop",
      "host-manifest.json",
      "install.js",
      "strings.json",
      "LICENSE",
      "skills/kite/SKILL.md",
      ...["ready", "degraded", "offline", "busy"].map((state) => `icons/kite-${state}.png`),
    ].sort(),
  );
  const manifest = JSON.parse(
    (await command(["tar", "-xzOf", a, `${prefix}/host-manifest.json`])).stdout,
  );
  expect(manifest.sourceCommit).toBe("a".repeat(40));
  expect(manifest.runtime).toBe("bun");
  expect(manifest.installer).toBe("install.js");
  expect(manifest.nodeApi).toBe(1);
  const headers = gunzipSync(readFileSync(a));
  for (let offset = 0; headers[offset]; ) {
    const number = (start: number, length: number) =>
      Number.parseInt(
        headers
          .subarray(offset + start, offset + start + length)
          .toString()
          .replaceAll("\0", "")
          .trim(),
        8,
      );
    expect(number(108, 8)).toBe(0);
    expect(number(116, 8)).toBe(0);
    expect(number(136, 12)).toBe(0);
    offset += 512 + Math.ceil(number(124, 12) / 512) * 512;
  }
  expect((await command(["tar", "-xzOf", a, `${prefix}/install.js`])).stdout).not.toContain(
    "python3",
  );
});
