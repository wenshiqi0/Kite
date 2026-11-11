import {
  closeSync,
  existsSync,
  fstatSync,
  openSync,
  readSync,
  realpathSync,
  statSync,
} from "node:fs";
import { isAbsolute, join } from "node:path";
import { check, MAX_FILE, type ObjectValue, object, optionalString, parse } from "@kite/protocol";
import { atomic, locked, privateDir, readJson, sha256 } from "@kite/runtime";
import { optionalObject } from "@kite/runtime/socket";
import { stateDir } from "./catalog";
import { contentFingerprint, projectContent } from "./media";
import { storeGeneratedBytes, storeGeneratedImage } from "./uploads";

const LIMIT = 2 * 1024 * 1024;
interface SourceLine {
  start: number;
  end: number;
  text: string | null;
  complete: boolean;
}
/** Strip image_url values while scanning, without retaining inline base64 strings. */
export function sourceLine(
  fd: number,
  start: number,
  limit: number,
  generated = false,
): SourceLine {
  const parts: Buffer[] = [];
  let size = 0,
    position = start,
    skip = false,
    escaped = false,
    carry = Buffer.alloc(0),
    complete = false;
  const emit = (b: Buffer) => {
    if (size <= limit) {
      const part = b.subarray(0, Math.max(0, limit + 1 - size));
      parts.push(part);
      size += part.length;
    }
  };
  while (true) {
    const block = Buffer.alloc(65536);
    const n = readSync(fd, block, 0, block.length, position);
    if (!n) break;
    let chunk = block.subarray(0, n);
    const nl = chunk.indexOf(10);
    if (nl >= 0) {
      chunk = chunk.subarray(0, nl + 1);
      complete = true;
    }
    position += chunk.length;
    if (generated) {
      emit(chunk);
    } else {
      let data = Buffer.concat([carry, chunk]);
      carry = Buffer.alloc(0);
      while (data.length) {
        if (skip) {
          let at = -1;
          for (let i = 0; i < data.length; i++) {
            const c = data[i];
            if (c === 34 && !escaped) {
              at = i;
              break;
            }
            escaped = c === 92 ? !escaped : false;
          }
          if (at < 0) {
            data = Buffer.alloc(0);
            continue;
          }
          skip = false;
          escaped = false;
          data = data.subarray(at + 1);
        } else {
          const match = /"image_url"\s*:\s*"/.exec(data.toString("latin1"));
          if (match) {
            emit(Buffer.concat([data.subarray(0, match.index), Buffer.from('"image_url":""')]));
            data = data.subarray(match.index + match[0].length);
            skip = true;
          } else {
            if (data.length > 64) emit(data.subarray(0, -64));
            carry = data.subarray(-64);
            break;
          }
        }
      }
    }
    if (complete) break;
  }
  emit(carry);
  return {
    start,
    end: position,
    text: size <= limit ? Buffer.concat(parts).toString() : null,
    complete,
  };
}
export function displayLine(fd: number, start: number, limit: number, cwd = ""): SourceLine {
  const head = Buffer.alloc(1024);
  const n = readSync(fd, head, 0, 1024, start);
  const prefix = head.subarray(0, n).toString();
  if (
    !/"(?:image_generation_call|image_generation_end|ImageGeneration|image_gen\.generation)"/.test(
      prefix,
    )
  )
    return contentLine(fd, start, limit, cwd);
  const row = sourceLine(fd, start, Math.ceil(MAX_FILE / 3) * 4 + limit, true);
  if (!row.complete) return { ...row, text: null };
  if (row.text === null) {
    const id =
      /"(?:id|call_id)"\s*:\s*"([^"\\]{1,256})"/.exec(prefix)?.[1] || `image-offset-${start}`;
    return {
      ...row,
      text: JSON.stringify({
        type: "response_item",
        payload: {
          type: "image_generation_call",
          id,
          status: "completed",
          _kite_image_error: "generated image exceeds 25 MiB",
        },
      }),
    };
  }
  let record: ObjectValue;
  try {
    record = object(parse(row.text));
  } catch {
    return { ...row, text: "" };
  }
  let payload = object(record.payload);
  if (
    record.type === "event_msg" &&
    ["item_completed", "item_started"].includes(optionalString(payload.type))
  ) {
    const item = object(payload.item);
    if (
      item.type === "ImageGeneration" ||
      (item.type === "Extension" && item.kind === "image_gen.generation")
    ) {
      payload = {
        ...item,
        type: "image_generation_call",
        saved_path: item.savedPath ?? item.saved_path,
      };
      record = { ...record, type: "response_item", payload };
    }
  }
  if (record.type === "event_msg" && payload.type === "image_generation_end") {
    record.type = "response_item";
    payload.type = "image_generation_call";
    payload.id = payload.call_id;
  } else if (record.type !== "response_item" || payload.type !== "image_generation_call")
    return contentLine(fd, start, limit, cwd);
  const encoded = payload.result;
  delete payload.result;
  delete payload.encrypted_content;
  payload._kite_images = [];
  try {
    if (typeof encoded === "string" && encoded)
      payload._kite_images = [storeGeneratedImage(encoded)];
    else if (payload.saved_path && payload.status === "completed") {
      const path = String(payload.saved_path);
      check(isAbsolute(path), "invalid generated image path");
      const file = openSync(path, "r");
      try {
        const info = fstatSync(file);
        check(info.isFile() && info.size <= MAX_FILE, "invalid generated image file");
        const data = Buffer.alloc(info.size);
        check(readSync(file, data) === data.length, "generated image changed");
        payload._kite_images = [storeGeneratedBytes(data)];
      } finally {
        closeSync(file);
      }
    }
  } catch (error) {
    payload._kite_image_error = error instanceof Error ? error.message : "invalid generated image";
  }
  return { ...row, text: JSON.stringify(record) };
}
// Match serde_json's sorted-key record identity before presentation changes it.
function sourceIdentity(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(sourceIdentity).join(",")}]`;
  if (value && typeof value === "object")
    return `{${Object.keys(value)
      .sort()
      .filter((key) => key !== "_kite_offset")
      .map(
        (key) =>
          `${JSON.stringify(key)}:${sourceIdentity(key === "image_url" && typeof (value as Record<string, unknown>)[key] === "string" ? "" : (value as Record<string, unknown>)[key])}`,
      )
      .join(",")}}`;
  return JSON.stringify(value) ?? "null";
}
function contentLine(fd: number, start: number, limit: number, cwd: string): SourceLine {
  const row = sourceLine(fd, start, 40 * 1024 * 1024, true);
  if (!row.complete) return { ...row, text: null };
  // Keep controls/oversized input scanning bounded even when media is too large.
  if (row.text === null) return sourceLine(fd, start, limit);
  try {
    const record = object(parse(row.text));
    record._kite_record_id = sha256(sourceIdentity(record));
    const payload = object(
      record.type === "response_item" ? record.payload : (record.message ?? {}),
    );
    payload._kite_content_fingerprint ||= contentFingerprint(payload.content);
    if (Array.isArray(payload.content) || typeof payload.content === "string")
      payload.content = projectContent(payload.content, cwd);
    if (Array.isArray(payload.output)) payload.output = projectContent(payload.output, cwd);
    const text = JSON.stringify(record);
    return { ...row, text: Buffer.byteLength(text) <= limit ? text : null };
  } catch {
    return { ...row, text: "" };
  }
}
export async function questionRecords(file: string): Promise<Buffer> {
  const cache = join(privateDir(join(stateDir(), "history")), `${sha256(realpathSync(file))}.json`);
  return locked(`${cache}.lock`, () => {
    const fd = openSync(file, "r");
    try {
      const info = fstatSync(fd);
      let offset = 0,
        rows: ObjectValue[] = [];
      if (existsSync(cache)) {
        try {
          const saved = object(readJson(cache));
          if (
            saved.version === 4 &&
            saved.device === info.dev &&
            saved.inode === info.ino &&
            typeof saved.offset === "number" &&
            saved.offset >= 0 &&
            saved.offset <= info.size
          ) {
            offset = saved.offset;
            rows = (Array.isArray(saved.rows) ? saved.rows : []).map(object);
          }
        } catch {
          /* rebuild an invalid cache from its source */
        }
      }
      const calls = new Set(
        rows
          .filter((r) => object(r.payload).type === "function_call")
          .map((r) => object(r.payload).call_id),
      );
      while (offset < info.size) {
        const row = sourceLine(fd, offset, LIMIT);
        if (!row.complete) break;
        offset = row.end;
        if (!row.text) continue;
        try {
          let r = object(parse(row.text));
          if (!["response_item", "event_msg"].includes(String(r.type))) continue;
          const p = object(r.payload);
          let keep = false;
          if (
            r.type === "event_msg" &&
            ["task_complete", "task_aborted"].includes(String(p.type))
          ) {
            r = {
              type: "event_msg",
              payload: {
                type: p.type,
                turn_id: String(p.turn_id || row.start),
                failed: p.error != null,
              },
            };
            keep = true;
          } else if (
            p.type === "function_call" &&
            p.name === "request_user_input_async" &&
            p.call_id
          ) {
            calls.add(p.call_id);
            keep = true;
          } else if (p.type === "function_call_output" && calls.has(p.call_id)) keep = true;
          else if (p.type === "message" && p.role === "user") {
            let text = (Array.isArray(p.content) ? p.content : [])
              .map(object)
              .filter((v) => typeof v.text === "string");
            if (!text.some((v) => String(v.text).includes("<send_user_message_question_reply>"))) {
              const marker = ["<environment_context>", "<turn_aborted>"].find((tag) =>
                String(text[0]?.text || "").startsWith(tag),
              );
              text = marker ? [{ type: "input_text", text: marker }] : [];
            }
            r = {
              type: "response_item",
              payload: { type: "message", role: "user", content: text },
            };
            keep = true;
          }
          if (keep) rows.push(r);
        } catch {
          /* non-control provider record */
        }
      }
      const lastTerminal = rows.findLastIndex((r) => r.type === "event_msg");
      const compact: ObjectValue[] = [];
      for (const [i, r] of rows.entries()) {
        if (r.type === "event_msg" && i !== lastTerminal) continue;
        const p = object(r.payload);
        if (
          p.role === "user" &&
          JSON.stringify(p.content) === "[]" &&
          JSON.stringify(compact.at(-1)) === JSON.stringify(r)
        )
          continue;
        compact.push(r);
      }
      const result = Buffer.from(compact.map((r) => `${JSON.stringify(r)}\n`).join(""));
      check(result.length <= 1024 * 1024, "question history index exceeds transport capacity");
      atomic(cache, { version: 4, device: info.dev, inode: info.ino, offset, rows: compact });
      return result;
    } finally {
      closeSync(fd);
    }
  });
}
function* reverseRanges(fd: number, end: number): Generator<[number, number]> {
  let position = end,
    lineEnd = end;
  while (position) {
    const start = Math.max(0, position - 65536),
      block = Buffer.alloc(position - start);
    readSync(fd, block, 0, block.length, start);
    let index = block.length;
    while (true) {
      index = block.lastIndexOf(10, index - 1);
      if (index < 0) break;
      const lineStart = start + index + 1;
      if (lineStart < lineEnd) yield [lineStart, lineEnd];
      lineEnd = lineStart;
      if (index === 0) break;
    }
    position = start;
  }
  if (lineEnd) yield [0, lineEnd];
}
export function cursorPosition(
  value: string,
  info: { dev: number; ino: number; size: number },
): number {
  check(/^\d+:\d+:\d+:\d+$/.test(value) && value.length <= 128, "invalid history cursor");
  const [d, i, p, s] = value.split(":").map(Number);
  check(
    d === info.dev &&
      i === info.ino &&
      p !== undefined &&
      s !== undefined &&
      p >= 0 &&
      p <= s &&
      s <= info.size,
    "history source changed; reopen the latest page",
  );
  return p;
}
const displayRecord = (r: ObjectValue, provider: string) => {
  if (provider === "claude") return r.type === "user" || r.type === "assistant";
  const payload = object(r.payload);
  if (r.type === "event_msg")
    return (
      ["task_complete", "task_aborted"].includes(String(payload.type)) && payload.error != null
    );
  return (
    r.type === "response_item" &&
    [
      "message",
      "function_call",
      "custom_tool_call",
      "computer_call",
      "function_call_output",
      "custom_tool_call_output",
      "computer_call_output",
      "image_generation_call",
    ].includes(String(payload.type))
  );
};
export async function historyProjection(
  file: string,
  before?: string,
  after?: string,
  provider = "codex",
): Promise<Buffer> {
  check(!(before && after), "choose one history direction");
  const controls = provider === "claude" ? Buffer.alloc(0) : await questionRecords(file),
    transport = 4 * 1024 * 1024 - controls.length - 4096,
    pageBudget = Math.min(1024 * 1024, transport);
  const fd = openSync(file, "r");
  try {
    const info = fstatSync(fd),
      boundary = before || after ? cursorPosition(before || after || "", info) : info.size;
    const header = Buffer.alloc(Math.min(info.size, 65536));
    readSync(fd, header, 0, header.length, 0);
    let cwd = "";
    for (const line of header.toString().split("\n").slice(0, 16)) {
      try {
        const row = object(parse(line));
        cwd ||= optionalString(row.cwd || optionalObject(row.payload).cwd);
      } catch {
        /* Only complete metadata rows can provide a working directory. */
      }
    }
    const rows: { start: number; end: number; text: string }[] = [];
    let total = 0;
    function* forward(): Generator<[number, number]> {
      let pos = boundary;
      while (pos < info.size) {
        const row = displayLine(fd, pos, transport, cwd);
        if (!row.complete || row.end > info.size) break;
        yield [pos, row.end];
        pos = row.end;
      }
    }
    for (const [start, end] of after ? forward() : reverseRanges(fd, boundary)) {
      const row = displayLine(fd, start, transport, cwd);
      if (!row.complete || row.end !== end) continue;
      if (row.text === null) {
        const header = Buffer.alloc(Math.min(1024, end - start));
        readSync(fd, header, 0, header.length, start);
        check(
          !header.includes("response_item"),
          "conversation record exceeds the history transport limit",
        );
        continue;
      }
      let r: ObjectValue;
      try {
        r = object(parse(row.text));
        if (!displayRecord(r, provider)) continue;
      } catch {
        continue;
      }
      const text = `${JSON.stringify({ ...r, _kite_offset: start })}\n`;
      const bytes = Buffer.byteLength(text);
      if (rows.length && (rows.length >= 2048 || total + bytes > pageBudget)) break;
      rows.push({ start, end, text });
      total += bytes;
      if (total >= pageBudget) break;
    }
    if (!after) rows.reverse();
    const start = rows[0]?.start ?? (after ? boundary : 0),
      end = rows.at(-1)?.end ?? (after ? info.size : boundary);
    const cursor = (n: number) => `${info.dev}:${info.ino}:${n}:${info.size}`;
    const page = {
      type: "kite_history_page",
      start: cursor(start),
      end: cursor(end),
      has_older: rows.length > 0 && start > 0,
      has_newer: rows.length > 0 && end < info.size && Boolean(before || after),
    };
    return Buffer.concat([
      Buffer.from(`kite-codex-history-v2\n${controls.length}\n`),
      controls,
      Buffer.from(`${JSON.stringify(page)}\n${rows.map((r) => r.text).join("")}`),
    ]);
  } finally {
    closeSync(fd);
  }
}
export function historyStamp(file: string): string {
  const s = statSync(file, { bigint: true });
  return `${s.dev}:${s.ino}:${s.size}:${s.mtimeNs}:media-v1`;
}
export function historyFiles(root: string, id: string): string[] {
  return Array.from(
    new Bun.Glob(`**/rollout-*-${id}.jsonl`).scanSync({
      cwd: root,
      absolute: true,
      onlyFiles: true,
    }),
  );
}
