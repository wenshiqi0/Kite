import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { object, parse, string } from "@kite/protocol";
import { sha256 } from "@kite/runtime";
import { historyProjection } from "../src/history";
import { appendContent, projectContent } from "../src/media";
import { Dsh } from "../src/native/dsh";
import { Ply } from "../src/native/ply";
import { attachmentRead, uploadedFile } from "../src/uploads";

let home = "";
let previous: string | undefined;
beforeEach(() => {
  previous = process.env.HOME;
  home = mkdtempSync(join(tmpdir(), "kite-media-test-"));
  process.env.HOME = home;
});
afterEach(() => {
  if (previous === undefined) delete process.env.HOME;
  else process.env.HOME = previous;
  rmSync(home, { force: true, recursive: true });
});
const image = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 0]);
const inline = { type: "image", mimeType: "image/png", data: image.toString("base64") };
test("Ply and DSH forward actual ordered ACP media chunks, including media-only turns", async () => {
  for (const Adapter of [Ply, Dsh]) {
    const events: import("@kite/protocol").ObjectValue[] = [];
    const adapter = new Adapter({
      command: [
        process.execPath,
        resolve("packages/host/test/fixtures/native.ts"),
        Adapter === Ply ? "ply" : "dsh",
      ],
      cwd: home,
      model: "fixture-model",
      route: "fixture",
      event: (value) => events.push(value),
      permission: () => {
        throw new Error("unexpected permission");
      },
      persist: () => {},
    });
    try {
      await adapter.initialize();
      for (const suffix of ["mixed", "media-only"]) {
        events.length = 0;
        adapter.prepare();
        await adapter.run(
          [{ type: "text", text: `owned-multimodal ${suffix}` }],
          adapter.currentModel,
          "high",
        );
        const messages = events.filter((event) => event.type === "assistant/message");
        const content = object(object(messages.at(-1)?.data).message).content;
        expect(Array.isArray(content) && content.map((row) => object(row).type)).toEqual(
          suffix === "media-only" ? ["image"] : ["text", "image", "audio", "text"],
        );
        expect(new Set(messages.map((event) => event.seq)).size).toBe(1);
      }
    } finally {
      adapter.close();
    }
  }
});
test("human and agent content retains ordered media with immutable readable bytes", () => {
  const content = [
    { type: "text", text: "**Before**" },
    inline,
    {
      type: "audio",
      mimeType: "audio/wav",
      data: Buffer.from("RIFF-owned-audio").toString("base64"),
    },
    { type: "text", text: "After" },
  ];
  const projected = projectContent(content);
  expect(projected.map((row) => row.type)).toEqual([
    "text",
    "kite_attachment",
    "kite_attachment",
    "text",
  ]);
  const file = object(projected[1]?.file);
  expect(file.mime).toBe("image/png");
  expect(uploadedFile(string(file.id), string(file.name))).toEqual(image);
  expect(projectContent(content)).toEqual(projected);
  expect(JSON.stringify(projected)).not.toContain(inline.data);
});
test("nested image URLs and Anthropic image sources share the attachment contract", () => {
  for (const content of [
    { type: "input_image", image_url: `data:image/png;base64,${inline.data}` },
    { type: "image_url", image_url: { url: `data:image/png;base64,${inline.data}` } },
    { type: "image", source: { type: "base64", media_type: "image/png", data: inline.data } },
  ]) {
    const file = object(projectContent([content])[0]?.file);
    expect(uploadedFile(string(file.id), string(file.name))).toEqual(image);
  }
});
test("Markdown images and file links render in position while code and web links stay literal", () => {
  const path = join(home, "image with spaces.png");
  writeFileSync(path, image);
  const text = `Before ![preview](<${path}>) after. [Guide](https://example.test/guide)\n\n\`![code](${path})\`\n\n\`\`\`md\n![code](${path})\n\`\`\``;
  const projected = projectContent(text);
  expect(projected.map((row) => row.type)).toEqual(["text", "kite_attachment", "text"]);
  expect(projected[0]?.text).toBe("Before ");
  expect(projected[2]?.text).toContain("[Guide](https://example.test/guide)");
  expect(projected[2]?.text).toContain("![code]");
  const doc = join(home, "report.pdf");
  writeFileSync(doc, "%PDF-owned");
  expect(object(projectContent(`[Report](${doc})`)[0]?.file).mime).toBe("application/pdf");
});
test("embedded resources and tool results retain media without exposing their binary encoding", () => {
  const result = projectContent([{ type: "tool_result", tool_use_id: "call", content: [inline] }]);
  const content = object(result[0]).content;
  expect(Array.isArray(content) && object(content[0]).type).toBe("kite_attachment");
  const resource = projectContent([
    {
      type: "resource",
      resource: {
        uri: "file:///owned.wav",
        mimeType: "audio/wav",
        blob: Buffer.from("owned").toString("base64"),
      },
    },
  ]);
  expect(object(resource[0]?.file).mime).toBe("audio/wav");
});
test("Markdown web pages stay navigable while local Markdown remains a readable attachment", () => {
  const text =
    "[AGENTS.md](https://github.com/owned/private/blob/main/AGENTS.md)\n" +
    "[Guide](https://docs.example.test/guide.md?view=source#setup)";
  expect(projectContent(text)).toEqual([{ type: "text", text }]);
  const path = join(home, "AGENTS.md");
  const bytes = Buffer.from("# 本地文档\n\n- 可预览的 Markdown\n");
  writeFileSync(path, bytes);
  const file = object(projectContent(`[AGENTS.md](${path})`)[0]?.file);
  expect(file.mime).toBe("text/markdown");
  expect(uploadedFile(string(file.id), string(file.name))).toEqual(bytes);
});
test("malformed native attachments show a placeholder and missing Markdown files retain the link", () => {
  expect(
    projectContent([{ type: "audio", data: "not-base64!", mimeType: "audio/wav" }])[0]?.text,
  ).toContain("暂不可用");
  const text = "![missing](/does-not-exist/owned.png)";
  expect(projectContent(text)).toEqual([{ type: "text", text }]);
});
test("ACP text chunks merge around, and never across, media boundaries", () => {
  let content = appendContent([], { type: "text", text: "Be" });
  content = appendContent(content, { type: "text", text: "fore" });
  content = appendContent(content, inline);
  content = appendContent(content, { type: "text", text: "After" });
  expect(content).toEqual([
    { type: "text", text: "Before" },
    inline,
    { type: "text", text: "After" },
  ]);
});
test("explicit remote audio and Markdown fetch only on viewing, then serve the same cached bytes", async () => {
  let requests = 0;
  const bytes = Buffer.from("owned-audio");
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch() {
      requests++;
      return new Response(bytes);
    },
  });
  try {
    for (const [mimeType, name] of [
      ["audio/wav", "clip.wav"],
      ["text/markdown", "AGENTS.md"],
    ]) {
      requests = 0;
      const projected = projectContent([
        {
          type: "resource_link",
          uri: `${server.url}${name}`,
          mimeType,
          name,
        },
      ]);
      const file = object(projected[0]?.file);
      expect(requests).toBe(0);
      expect(file.size).toBe(0);
      const body = await attachmentRead(string(file.id), string(file.name));
      expect(body.subarray(body.indexOf(10) + 1)).toEqual(bytes);
      expect(requests).toBe(1);
      expect(await attachmentRead(string(file.id), string(file.name))).toEqual(body);
      expect(requests).toBe(1);
    }
  } finally {
    server.stop(true);
  }
});
test("provider projection preserves both authors, strips inline bytes and resolves relative files", async () => {
  writeFileSync(join(home, "photo.png"), image);
  for (const provider of ["codex", "claude"]) {
    const file = join(home, `${provider}.jsonl`);
    const records = [JSON.stringify({ type: "session_meta", payload: { cwd: home } })];
    for (const role of ["user", "assistant"]) {
      const content = [
        { type: "text", text: "Before ![photo](./photo.png)" },
        inline,
        { type: "text", text: "After" },
      ];
      records.push(
        JSON.stringify(
          provider === "claude"
            ? { type: role, uuid: role, message: { content } }
            : { type: "response_item", payload: { type: "message", role, content } },
        ),
      );
    }
    const source = `${records.join("\n")}\n`;
    writeFileSync(file, source);
    const projected = (await historyProjection(file, undefined, undefined, provider)).toString();
    expect(projected).not.toContain(inline.data);
    const first = projected.indexOf("\n"),
      second = projected.indexOf("\n", first + 1);
    const display = projected.slice(second + 1 + Number(projected.slice(first + 1, second)));
    const rows = display
      .split("\n")
      .filter((row) => row.startsWith("{"))
      .map((row) => object(parse(row)))
      .filter((row) => row.type !== "kite_history_page");
    expect(rows).toHaveLength(2);
    const content = object(provider === "claude" ? rows[1]?.message : rows[1]?.payload).content;
    expect(Array.isArray(content) && content.map((row) => object(row).type)).toEqual([
      "text",
      "kite_attachment",
      "kite_attachment",
      "text",
    ]);
    expect(readFileSync(file, "utf8")).toBe(source);
  }
});

test("history presentation retains the pre-existing sorted-key source identity", async () => {
  const path = join(home, "stable.jsonl");
  writeFileSync(
    path,
    `${JSON.stringify({
      type: "response_item",
      payload: {
        type: "message",
        role: "user",
        content: [
          { type: "input_text", text: "Look" },
          { type: "input_image", image_url: `data:image/png;base64,${inline.data}` },
        ],
      },
    })}\n`,
  );
  const output = (await historyProjection(path)).toString();
  expect(output).toContain(
    sha256(
      '{"payload":{"content":[{"text":"Look","type":"input_text"},{"image_url":"","type":"input_image"}],"role":"user","type":"message"},"type":"response_item"}',
    ),
  );
});
