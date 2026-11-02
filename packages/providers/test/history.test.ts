import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import {
  appendFileSync,
  closeSync,
  mkdtempSync,
  openSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { object, parse, string } from "@kite/protocol";
import { cursorPosition, historyProjection, questionRecords, sourceLine } from "../src/history";
import { readGeneratedImage } from "../src/uploads";

let home = "",
  previous: string | undefined;
beforeEach(() => {
  home = mkdtempSync(join(tmpdir(), "kite-history-test-"));
  previous = process.env.HOME;
  process.env.HOME = home;
});
afterEach(() => {
  if (previous === undefined) delete process.env.HOME;
  else process.env.HOME = previous;
  rmSync(home, { recursive: true, force: true });
});
const message = (text: string) =>
  `${JSON.stringify({
    type: "response_item",
    payload: { type: "message", role: "assistant", content: [{ type: "text", text }] },
  })}\n`;
function projection(body: Buffer): { control: string; rows: string[] } {
  const text = body.toString(),
    first = text.indexOf("\n"),
    second = text.indexOf("\n", first + 1),
    size = Number(text.slice(first + 1, second)),
    offset = second + 1;
  return {
    control: text.slice(offset, offset + size),
    rows: text
      .slice(offset + size)
      .trim()
      .split("\n"),
  };
}
describe("append-safe complete history projection", () => {
  test("pages backwards and forwards without dropping or duplicating messages", async () => {
    const file = join(home, "rollout.jsonl");
    writeFileSync(
      file,
      Array.from({ length: 750 }, (_, i) => message(`${i}:${"x".repeat(4096)}`)).join(""),
    );
    let body = projection(await historyProjection(file)),
      page = object(parse(string(body.rows[0])));
    expect(page.has_older).toBe(true);
    const offsets = new Set<number>();
    let count = 0;
    while (true) {
      for (const line of body.rows.slice(1)) {
        const r = object(parse(line));
        const offset = Number(r._kite_offset);
        expect(offsets.has(offset)).toBe(false);
        offsets.add(offset);
        count++;
      }
      if (!page.has_older) break;
      body = projection(await historyProjection(file, string(page.start)));
      page = object(parse(string(body.rows[0])));
    }
    expect(count).toBe(750);
    const last = projection(await historyProjection(file)),
      cursor = string(object(parse(string(last.rows[0]))).end);
    appendFileSync(file, message("appended"));
    const next = projection(await historyProjection(file, undefined, cursor));
    expect(next.rows).toHaveLength(2);
    expect(string(next.rows[1])).toContain("appended");
    writeFileSync(file, message("truncated"));
    expect(historyProjection(file, undefined, cursor)).rejects.toThrow("source changed");
  });
  test("indexes old questions across the whole log and retries partial records", async () => {
    const file = join(home, "questions.jsonl"),
      call = {
        type: "response_item",
        payload: {
          type: "function_call",
          name: "request_user_input_async",
          call_id: "owned-question",
          arguments: '{"questions":[]}',
        },
      };
    writeFileSync(file, `${JSON.stringify(call)}\n${message("ordinary")}`);
    expect((await questionRecords(file)).toString()).toContain("owned-question");
    appendFileSync(
      file,
      '{"type":"response_item","payload":{"type":"function_call_output","call_id":"owned-question","output":"accepted"}}',
    );
    expect((await questionRecords(file)).toString()).not.toContain("accepted");
    appendFileSync(file, "\n");
    expect((await questionRecords(file)).toString()).toContain("accepted");
    const result = projection(await historyProjection(file));
    expect(result.control).toContain("owned-question");
    expect(result.control).not.toContain("ordinary");
  });
  test("strips inline images using bounded chunks, including escaped quotes", () => {
    const file = join(home, "images.jsonl");
    writeFileSync(
      file,
      `${JSON.stringify({
        type: "response_item",
        payload: {
          type: "message",
          role: "user",
          content: [
            {
              type: "input_image",
              image_url: `data:image/png;base64,${"a".repeat(5 * 1024 * 1024)}\\"tail`,
            },
            { type: "input_text", text: "kept" },
          ],
        },
      })}\n`,
    );
    const fd = openSync(file, "r");
    try {
      const line = sourceLine(fd, 0, 2048);
      expect(line.complete).toBe(true);
      expect(line.text).toContain('"image_url":""');
      expect(line.text).toContain("kept");
      expect(line.text?.length).toBeLessThan(2048);
    } finally {
      closeSync(fd);
    }
  });
  test("projects generated image bytes to immutable attachments", async () => {
    const file = join(home, "generated.jsonl"),
      image = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 0]);
    writeFileSync(
      file,
      `${JSON.stringify({ type: "response_item", payload: { type: "image_generation_call", id: "owned", result: image.toString("base64"), encrypted_content: "private-reasoning", status: "completed" } })}\n`,
    );
    const body = projection(await historyProjection(file));
    const row = object(parse(string(body.rows[1]))),
      payload = object(row.payload);
    expect(payload.result).toBeUndefined();
    expect(payload.encrypted_content).toBeUndefined();
    const images = payload._kite_images;
    expect(Array.isArray(images)).toBe(true);
    if (!Array.isArray(images)) throw new Error("image metadata missing");
    expect(readGeneratedImage(string(object(images[0]).id))).toEqual(image);
    expect(readFileSync(file, "utf8")).toContain("private-reasoning");
  });
  test("rejects malformed cursor and direction conflicts", async () => {
    expect(() => cursorPosition("../escape", { dev: 1, ino: 2, size: 10 })).toThrow();
    expect(() => cursorPosition("1:3:0:10", { dev: 1, ino: 2, size: 10 })).toThrow();
    expect(historyProjection(join(home, "unused"), "before", "after")).rejects.toThrow(
      "one history direction",
    );
  });
});
