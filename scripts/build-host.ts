import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

const at = process.argv.indexOf("--out-dir"),
  out = resolve(at >= 0 ? process.argv[at + 1] || "" : "target/bun");
mkdirSync(out, { recursive: true });
for (const [entry, name, target] of [
  ["packages/host/src/cli.ts", "host.js", "bun"],
  ["packages/providers/src/native/dsh-plugin.ts", "dsh-plugin.mjs", "node"],
  ["packages/platform/src/clipboard-cli.ts", "clipboard.js", "bun"],
] as const) {
  const result = await Bun.build({
    entrypoints: [entry],
    target,
    format: "esm",
    minify: false,
    external: [],
    packages: "bundle",
  });
  if (!result.success) throw new AggregateError(result.logs, `failed to build ${name}`);
  const artifact = result.outputs[0];
  if (!artifact) throw new Error(`missing ${name}`);
  await Bun.write(resolve(out, name), artifact);
}
