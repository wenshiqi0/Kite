/** Named reference token counts for shipped documentation; no model turn. */
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { parseArgs } from "node:util";
import { array, check, object, parse, string } from "@kite/protocol";
import { getEncoding } from "js-tiktoken";
import { command } from "../../src/command";

const { values } = parseArgs({
  options: { binary: { type: "string" }, output: { type: "string" } },
  strict: true,
});
check(values.binary, "--binary is required");
const binary = resolve(values.binary),
  cli = async (...args: string[]) => (await command([binary, ...args])).stdout;
const [index, rawManifest, desktop, skill, advanced] = await Promise.all([
  cli("docs"),
  cli("docs", "--json"),
  cli("docs", "desktop"),
  cli("docs", "skill"),
  cli("docs", "desktop/input"),
]);
const manifest = object(parse(rawManifest)),
  topics = array(manifest.topics).map(object),
  revision = string(topics.find((topic) => topic.id === "desktop")?.revision);
const cached = await cli("docs", "desktop", "--known", `desktop=${revision}`),
  manual =
    index +
    (
      await Promise.all(
        topics
          .filter((topic) => topic.id !== "skill")
          .map((topic) => cli("docs", string(topic.id))),
      )
    ).join("\n");
const catalogue = `kite: ${skill.split("description: ")[1]?.split("\n")[0] || ""}`;
const messages = [
  {
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: {
      protocolVersion: "2025-11-25",
      capabilities: {},
      clientInfo: { name: "token-budget", version: "1" },
    },
  },
  { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
];
const response = await command([binary, "--socket", "/missing/owned-budget.sock", "mcp"], {
    input: `${messages.map((message) => JSON.stringify(message)).join("\n")}\n`,
    timeout: 10000,
  }),
  rows = response.stdout
    .trim()
    .split("\n")
    .map((line) => object(parse(line)));
const tools = object(rows.find((row) => row.id === 2)?.result).tools,
  initial = string(object(rows.find((row) => row.id === 1)?.result).instructions);
const parts = {
  skill_catalogue: catalogue,
  skill_body: skill,
  index,
  desktop_guide: desktop,
  advanced_input_guide: advanced,
  conditional_unchanged: cached,
  mcp_tool_definitions: JSON.stringify(tools),
  mcp_initial_instructions: initial,
  full_manual_once: manual,
};
const encodings: Record<string, unknown> = {};
for (const name of ["cl100k_base", "o200k_base"] as const) {
  const encoder = getEncoding(name),
    counts = Object.fromEntries(
      Object.entries(parts).map(([key, body]) => [key, encoder.encode(body).length]),
    );
  const first =
      (counts.skill_catalogue || 0) + (counts.skill_body || 0) + (counts.desktop_guide || 0),
    count = (key: keyof typeof parts) => counts[key] || 0;
  check(
    count("skill_catalogue") <= 64 && count("skill_body") <= 220 && count("index") <= 350,
    `${name} initial documentation budget exceeded`,
  );
  check(
    count("desktop_guide") <= 700 && count("mcp_tool_definitions") <= 850,
    `${name} desktop/tool budget exceeded`,
  );
  check(
    count("conditional_unchanged") < count("desktop_guide") / 5 &&
      first < count("full_manual_once") * 0.65,
    `${name} progressive loading budget exceeded`,
  );
  encodings[name] = {
    parts: counts,
    first_desktop_cli: first,
    first_desktop_mcp_if_schemas_eager:
      first + count("mcp_tool_definitions") + count("mcp_initial_instructions"),
    desktop_then_advanced: first + count("advanced_input_guide"),
    body_recheck_saving_percent:
      Math.round((1 - count("conditional_unchanged") / count("desktop_guide")) * 1000) / 10,
    first_vs_bulk_once_saving_percent:
      Math.round((1 - first / count("full_manual_once")) * 1000) / 10,
  };
}
const result = {
  status: "PASS",
  scope:
    "Named reference encodings; excludes images, reasoning and unrelated harness context. Native provider accounting may differ.",
  bytes: Object.fromEntries(
    Object.entries(parts).map(([key, text]) => [key, Buffer.byteLength(text)]),
  ),
  encodings,
};
console.log(JSON.stringify(result, null, 2));
if (values.output) {
  mkdirSync(dirname(resolve(values.output)), { recursive: true });
  writeFileSync(values.output, `${JSON.stringify(result, null, 2)}\n`);
}
