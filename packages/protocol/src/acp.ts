import { check, object, type RpcMessage } from "./index";

export const ACP_MESSAGE_LIMIT = 2 * 1024 * 1024;
export const ACP_QUEUE_LIMIT = 4 * 1024 * 1024;

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
