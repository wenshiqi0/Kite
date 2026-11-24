import { array, check, integer, object, type RpcMessage, SESSION_ID, string } from "./index";

export const ACP_MESSAGE_LIMIT = 2 * 1024 * 1024;
export const ACP_QUEUE_LIMIT = 4 * 1024 * 1024;
export const ACP_READ_LIMIT = 256 * 1024;
export const ACP_READ_DEFAULT = 64 * 1024;
export const ACP_MAX_CHILDREN_PER_PARENT = 16;
export const ACP_MAX_WAIT_SESSIONS = 16;
export const ACP_MAX_PLACEMENT_PEERS = 16;
export const ACP_PLACEMENT_METHOD = "_kite/host/placement";
export const ACP_CONTROL_METHODS = ["submit", "status", "read", "wait_any", "release"].map(
  (method) => `_kite/session/${method}`,
);

export function acpSessionId(value: unknown): string {
  const id = string(value);
  check(SESSION_ID.test(id), "invalid ACP session ID");
  return id;
}
export function acpLimit(value: unknown, fallback: number, max: number, min = 0): number {
  const limit = value === undefined ? fallback : integer(value);
  check(limit >= min && limit <= max, "ACP numeric limit out of range");
  return limit;
}
export function acpCommands(value: unknown): string[] {
  const commands = value === undefined ? [] : array(value).map(string);
  check(commands.length <= 16, "too many command authorizations");
  check(
    commands.every(
      (command) => command.trim() && command.length <= 8192 && !command.includes("\0"),
    ),
    "invalid command authorization",
  );
  return commands;
}

/** Network ACP preserves the same UTF-8 JSON-RPC envelope as stdio ACP v1. */
export function acpMessage(value: unknown): RpcMessage {
  const row = object(value);
  check(row.jsonrpc === "2.0", "ACP requires JSON-RPC 2.0");
  const id = row.id;
  check(
    id === undefined ||
      id === null ||
      typeof id === "string" ||
      (typeof id === "number" && Number.isSafeInteger(id)),
    "invalid ACP request identity",
  );
  check(typeof id !== "string" || id.length <= 256, "ACP request identity exceeds limit");
  if (row.method !== undefined) {
    check(
      typeof row.method === "string" && row.method.length > 0 && row.method.length <= 128,
      "invalid ACP method",
    );
    check(row.result === undefined && row.error === undefined, "invalid ACP request");
  } else {
    check(
      id !== undefined && Object.hasOwn(row, "result") !== Object.hasOwn(row, "error"),
      "invalid ACP response",
    );
    if (row.error !== undefined) {
      const error = object(row.error);
      check(
        typeof error.code === "number" &&
          Number.isSafeInteger(error.code) &&
          typeof error.message === "string",
        "invalid ACP error",
      );
    }
  }
  return row as RpcMessage;
}
