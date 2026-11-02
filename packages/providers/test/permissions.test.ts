import { expect, test } from "bun:test";
import { approvalFields, permissionSettings, permissionValues } from "../src/permissions";

test("approval content keeps executable text and file parameters out of JSON prose", () => {
  const command = "df -h\nfree -h";
  expect(
    approvalFields({ title: "bash", rawInput: { command, cwd: "/owned" } }, "/fallback"),
  ).toMatchObject({ command, cwd: "/owned", toolName: "bash", reason: undefined });
  expect(
    approvalFields({ title: "read", rawInput: { path: "/owned/note.txt" } }, "/owned"),
  ).toMatchObject({ command: undefined, toolName: "read", toolInput: { path: "/owned/note.txt" } });
});
test("only native permission categories and exact advertised choices are accepted", () => {
  const settings = permissionSettings([
    { id: "model", currentValue: "fixture" },
    {
      id: "approval_mode",
      name: "Tool approval",
      category: "permissions",
      type: "select",
      currentValue: "ask",
      options: [
        { value: "ask", name: "Ask" },
        { value: "risk", name: "Risk" },
      ],
    },
  ]);
  expect(settings).toHaveLength(1);
  expect(permissionValues({ approval_mode: "risk" }, settings)).toEqual({ approval_mode: "risk" });
  expect(() => permissionValues({ approval_mode: "bypass" }, settings)).toThrow();
  expect(() => permissionValues({ model: "fixture" }, settings)).toThrow();
  expect(() => permissionValues({ approval_mode: true }, settings)).toThrow();
});
