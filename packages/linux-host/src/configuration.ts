import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { array, check, type ObjectValue, object, string } from "@kite/protocol";

const desktopKeys = [
  "DISPLAY",
  "XAUTHORITY",
  "XDG_RUNTIME_DIR",
  "DBUS_SESSION_BUS_ADDRESS",
  "XDG_SESSION_TYPE",
  "XDG_CURRENT_DESKTOP",
];

export function sunshineSettings(source: string): Record<string, string> {
  const settings: Record<string, string> = {};
  let key: string | undefined;
  for (const line of source.split("\n")) {
    if (/^\s*(#|$)/.test(line)) continue;
    const match = /^\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*(.*)$/.exec(line);
    if (match?.[1]) {
      key = match[1];
      settings[key] = (match[2] || "").replace(/#.*$/, "").trimEnd();
    } else {
      check(key, "invalid Sunshine configuration");
      settings[key] += `\n${line}`;
    }
  }
  delete settings.sunshine_name;
  return settings;
}

/** Adopt existing settings once. Services subsequently read only node.json. */
export function unifyConfiguration(
  configuration: ObjectValue,
  home: string,
  which: (name: string) => string | null,
): ObjectValue {
  if (configuration.configuration_version === 1) return configuration;
  check(
    configuration.configuration_version === undefined || configuration.configuration_version === 0,
    "unsupported host configuration version",
  );
  const environment = object(configuration.environment || {});
  const services = array(configuration.services).map(object);
  const sunshine = services.find((service) => service.name === "Sunshine");
  if (sunshine) {
    let file = join(home, ".config/sunshine/sunshine.conf");
    let args: string[] = [];
    for (const proc of readdirSync("/proc")) {
      try {
        if (!/^\d+$/.test(proc) || readFileSync(`/proc/${proc}/comm`, "utf8").trim() !== "sunshine")
          continue;
        if (
          !readFileSync(`/proc/${proc}/cgroup`, "utf8")
            .split(/[\n/]/)
            .includes(string(sunshine.unit))
        )
          continue;
        const processEnvironment = readFileSync(`/proc/${proc}/environ`, "utf8").split("\0");
        const values = Object.fromEntries(
          processEnvironment.map((value) => {
            const at = value.indexOf("=");
            return [value.slice(0, at), value.slice(at + 1)];
          }),
        );
        if (values.HOME !== home) continue;
        for (const key of desktopKeys) {
          if (environment[key] === undefined && values[key]) environment[key] = values[key];
        }
        const argv = readFileSync(`/proc/${proc}/cmdline`, "utf8").split("\0").filter(Boolean);
        if (argv[1]?.startsWith("/")) file = argv[1];
        args = argv.slice(argv[1]?.startsWith("/") ? 2 : 1);
        break;
      } catch {
        // A process may exit while its existing configuration is being adopted.
      }
    }
    const settings = existsSync(file) ? sunshineSettings(readFileSync(file, "utf8")) : {};
    const overrides = args.filter((arg) => arg.includes("="));
    Object.assign(settings, sunshineSettings(overrides.join("\n")));
    settings.port ??= "47989";
    const executable = which("sunshine") || join(home, ".local/bin/sunshine");
    configuration.sunshine = {
      executable,
      args: args.filter((arg) => !arg.includes("=")),
      settings,
    };
  }
  // The former installer snapshot becomes a live system-hostname default.
  configuration.machine = "";
  configuration.environment = environment;
  configuration.configuration_version = 1;
  return configuration;
}

export function serviceLauncher(
  node: string,
  config: string,
  service: ObjectValue,
): string | undefined {
  const name = string(service.name);
  if (name !== "Herdr" && name !== "Sunshine") return undefined;
  const quote = (value: string) => {
    check(!/[\r\n\0]/.test(value), "invalid service path");
    return `"${value
      .replaceAll("%", "%%")
      .replaceAll("$", () => "$$")
      .replaceAll("\\", "\\\\")
      .replaceAll('"', '\\"')}"`;
  };
  return [
    "[Service]",
    "ExecStart=",
    `ExecStart=${quote(node)} service-exec ${name} --config ${quote(config)}`,
    ...(name === "Sunshine" ? ["ExecStartPre="] : []),
    "",
  ].join("\n");
}
