import { array, check, type Json, object, type SessionSetting, string } from "@kite/protocol";
import { optionalObject } from "@kite/runtime/socket";

/** Only native permission controls are editable; models/reasoning have their own selectors. */
export function permissionSettings(config: Json[]): SessionSetting[] {
  return config
    .map(object)
    .filter((row) => row.category === "permissions")
    .map((row) => {
      check(row.type === "select", "unsupported permission setting");
      const options = array(row.options).map((entry) => {
        const choice = object(entry);
        return { value: string(choice.value), name: string(choice.name) };
      });
      check(options.length > 0 && options.length <= 32, "invalid permission choices");
      const currentValue = string(row.currentValue);
      check(
        options.some((option) => option.value === currentValue),
        "invalid current permission",
      );
      return { id: string(row.id), name: string(row.name), currentValue, options };
    });
}

export function permissionValues(
  value: Json | undefined,
  settings: SessionSetting[],
): Record<string, string> {
  const values: Record<string, string> = {};
  for (const [id, raw] of Object.entries(optionalObject(value))) {
    const selected = string(raw);
    check(
      settings.some(
        (setting) =>
          setting.id === id && setting.options.some((option) => option.value === selected),
      ),
      "permission choice unavailable",
    );
    values[id] = selected;
  }
  return values;
}

export function approvalFields(tool: Json | undefined, cwd: string) {
  const call = optionalObject(tool),
    raw = call.rawInput,
    input =
      raw && typeof raw === "object" && !Array.isArray(raw)
        ? object(raw)
        : raw === undefined
          ? {}
          : { input: raw };
  return {
    toolName: call.title ?? "tool",
    toolInput: input,
    command: typeof input.command === "string" ? input.command : undefined,
    reason: typeof input.justification === "string" ? input.justification : undefined,
    cwd: typeof input.cwd === "string" ? input.cwd : cwd,
  };
}
