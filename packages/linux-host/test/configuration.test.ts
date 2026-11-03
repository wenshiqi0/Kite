import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { serviceLauncher, sunshineSettings, unifyConfiguration } from "../src/configuration";

const temporary: string[] = [];
afterEach(() => {
  for (const path of temporary.splice(0)) rmSync(path, { recursive: true, force: true });
});

test("existing capture settings are adopted once and later edits use the central configuration", () => {
  const home = mkdtempSync(join(tmpdir(), "kite-config-test-"));
  temporary.push(home);
  const directory = join(home, ".config/sunshine");
  mkdirSync(directory, { recursive: true });
  const file = join(directory, "sunshine.conf");
  writeFileSync(
    file,
    "sunshine_name = former-host\nport = 48123\ncapture = x11\noutput_name = DP-1\n",
  );
  const configuration = unifyConfiguration(
    {
      node_id: "existing-node",
      machine: "former-host",
      environment: {
        DISPLAY: ":10",
        XAUTHORITY: "/private/authority",
        MIMO_API_KEY: "private-value",
      },
      services: [{ name: "Sunshine", unit: "custom-sunshine.service", external: true }],
    },
    home,
    () => "/custom/sunshine",
  );
  expect(configuration.machine).toBe("");
  expect(configuration.node_id).toBe("existing-node");
  expect(configuration.sunshine).toEqual({
    executable: "/custom/sunshine",
    args: [],
    settings: { port: "48123", capture: "x11", output_name: "DP-1" },
  });
  writeFileSync(file, "port = 49000\n");
  expect(unifyConfiguration(configuration, home, () => null)).toBe(configuration);
  expect(configuration.environment).toEqual({
    DISPLAY: ":10",
    XAUTHORITY: "/private/authority",
    MIMO_API_KEY: "private-value",
  });
});

test("Sunshine lists and comments survive adoption", () => {
  expect(
    sunshineSettings(`
# capture profile
port = 48123 # base port
global_prep_cmd = [
  {"do":"true", "undo":"true"}
]
`),
  ).toEqual({ port: "48123", global_prep_cmd: '[\n  {"do":"true", "undo":"true"}\n]' });
});

test("service units reference one configuration path and carry no copied settings", () => {
  const unit = serviceLauncher("/a path/kite-node", "/a path/node.json", { name: "Sunshine" });
  expect(unit).toContain(
    'ExecStart="/a path/kite-node" service-exec Sunshine --config "/a path/node.json"',
  );
  expect(unit).toContain("ExecStartPre=\n");
  expect(unit).not.toContain("Environment=");
  expect(serviceLauncher("/node", "/config", { name: "Unrelated" })).toBeUndefined();
});
