import { closeSync, existsSync, fstatSync, openSync, readSync } from "node:fs";
import { basename, extname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { check, type Json, MAX_FILE, type ObjectValue, optionalString } from "@kite/protocol";
import { atomic, atomicBytes, privateDir, readJson, sha256 } from "@kite/runtime";
import { optionalObject } from "@kite/runtime/socket";
import { stateDir } from "./catalog";
import { filename, mime as imageMime, uploadRoot } from "./uploads";

const types: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  webp: "image/webp",
  gif: "image/gif",
  svg: "image/svg+xml",
  avif: "image/avif",
  heic: "image/heic",
  bmp: "image/bmp",
  wav: "audio/wav",
  mp3: "audio/mpeg",
  m4a: "audio/mp4",
  aac: "audio/aac",
  ogg: "audio/ogg",
  opus: "audio/opus",
  flac: "audio/flac",
  mp4: "video/mp4",
  webm: "video/webm",
  mov: "video/quicktime",
  pdf: "application/pdf",
  zip: "application/zip",
  json: "application/json",
  txt: "text/plain",
  md: "text/markdown",
  csv: "text/csv",
  html: "text/html",
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  xlsx: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  pptx: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
};
const sources = () => privateDir(join(stateDir(), "media-sources"));
function mediaName(uri: string, type: string, name: string): string {
  const extension = Object.keys(types).find((key) => types[key] === type) || "bin";
  const candidate = name || (uri.startsWith("data:") ? "" : basename(uri.split(/[?#]/)[0] || ""));
  const result = candidate || `attachment.${extension}`;
  filename(result);
  return result;
}
function descriptor(id: string, name: string, mime: string, size: number): ObjectValue {
  return { id, name, mime, size, path: join(uploadRoot(), id, name) };
}
function store(data: Buffer, type: string, name: string): ObjectValue {
  check(data.length > 0 && data.length <= MAX_FILE, "attachment exceeds 25 MiB or is empty");
  const mime = imageMime(data) || type || "application/octet-stream";
  name = mediaName("", mime, name);
  const id = sha256(Buffer.concat([Buffer.from(`${name}\0${mime}\0`), data])).slice(0, 32);
  const file = descriptor(id, name, mime, data.length);
  privateDir(join(uploadRoot(), id));
  if (!existsSync(String(file.path))) atomicBytes(String(file.path), data);
  return file;
}
function attachment(uri: string, type: string, name: string, cwd: string): ObjectValue {
  if (uri.startsWith("data:")) {
    const match = /^data:([^;,]+);base64,([\s\S]*)$/.exec(uri);
    check(
      match?.[2] && match[2].length <= Math.ceil(MAX_FILE / 3) * 4,
      "invalid inline attachment",
    );
    check(
      /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(match[2]),
      "invalid attachment encoding",
    );
    return store(Buffer.from(match[2], "base64"), match[1] || type, name);
  }
  if (/^https?:\/\//i.test(uri)) {
    const url = new URL(uri);
    check(!url.username && !url.password, "attachment URL includes credentials");
    name = mediaName(uri, type, name);
    const mime = type || types[extname(name).slice(1).toLowerCase()] || "application/octet-stream";
    const id = sha256(JSON.stringify([uri, mime, name])).slice(0, 32);
    const source = join(sources(), `${id}.json`);
    if (!existsSync(source)) atomic(source, { uri, name, mime });
    // Network bytes are fetched only when the user views this attachment.
    return descriptor(id, name, mime, 0);
  }
  let path = uri.startsWith("file:") ? fileURLToPath(uri) : uri.replace(/^sandbox:/, "");
  if (!isAbsolute(path)) {
    check(cwd, "relative attachment has no working directory");
    path = resolve(cwd, path);
  }
  const fd = openSync(path, "r");
  try {
    const info = fstatSync(fd);
    check(info.isFile() && info.size > 0 && info.size <= MAX_FILE, "invalid attachment file");
    const bytes = Buffer.alloc(info.size);
    let offset = 0;
    while (offset < bytes.length) {
      const count = readSync(fd, bytes, offset, bytes.length - offset, null);
      check(count > 0, "attachment changed while reading");
      offset += count;
    }
    name = mediaName(path, type, name);
    return store(
      bytes,
      type || types[extname(name).slice(1).toLowerCase()] || "application/octet-stream",
      name,
    );
  } finally {
    closeSync(fd);
  }
}

export async function fetchMedia(id: string, name: string): Promise<void> {
  check(/^[a-f0-9]{32}$/.test(id), "invalid attachment identity");
  filename(name);
  const source = join(sources(), `${id}.json`);
  const value = optionalObject(readJson(source));
  check(value.name === name && typeof value.uri === "string", "attachment source changed");
  const response = await fetch(value.uri, { signal: AbortSignal.timeout(45000) });
  check(response.ok && response.body, "attachment download failed");
  const reader = response.body.getReader();
  const parts: Buffer[] = [];
  let size = 0;
  try {
    while (true) {
      const next = await reader.read();
      if (next.done) break;
      size += next.value.length;
      check(size <= MAX_FILE, "attachment exceeds 25 MiB");
      parts.push(Buffer.from(next.value));
    }
  } finally {
    await reader.cancel();
  }
  check(size > 0, "empty attachment");
  privateDir(join(uploadRoot(), id));
  atomicBytes(join(uploadRoot(), id, name), Buffer.concat(parts));
}

function projectMedia(block: ObjectValue, cwd: string): ObjectValue | undefined {
  const source = optionalObject(block.source),
    resource = optionalObject(block.resource);
  const image = typeof block.image_url === "object" ? optionalObject(block.image_url) : {},
    audio = optionalObject(block.input_audio);
  const video = typeof block.video_url === "object" ? optionalObject(block.video_url) : {};
  const type =
    optionalString(block.mimeType || block.mime_type || source.media_type || resource.mimeType) ||
    (block.type === "input_audio"
      ? `audio/${optionalString(audio.format) || "wav"}`
      : ["image", "input_image", "output_image", "image_url"].includes(optionalString(block.type))
        ? "image/*"
        : ["audio", "output_audio"].includes(optionalString(block.type))
          ? "audio/*"
          : ["video", "video_url"].includes(optionalString(block.type))
            ? "video/*"
            : "");
  const fileData = optionalString(block.file_data);
  const data =
    optionalString(block.data || source.data || resource.blob || audio.data) ||
    (fileData.startsWith("data:") ? "" : fileData);
  const uri =
    [
      block.uri,
      block.url,
      block.image_url,
      block.file_url,
      block.path,
      image.url,
      video.url,
      block.video_url,
      fileData.startsWith("data:") ? fileData : "",
      source.url,
      resource.uri,
    ]
      .map(optionalString)
      .find(Boolean) || "";
  if (!data && !uri) return undefined;
  const file = attachment(
    data ? `data:${type || "application/octet-stream"};base64,${data}` : uri,
    type,
    optionalString(block.name || block.filename || block.title),
    cwd,
  );
  return { type: "kite_attachment", file, native_type: block.type };
}
function markdown(text: string, cwd: string): ObjectValue[] {
  // Code, Skill instructions and the one Rust-owned envelope codec stay literal.
  if (text.includes("<kite_attachments>") || text.includes("<skill>"))
    return [{ type: "text", text }];
  const result: ObjectValue[] = [];
  const pattern =
    /(```[\s\S]*?(?:```|$)|~~~[\s\S]*?(?:~~~|$)|`+[^`\n]*`+)|(!?)\[([^\]\n]*)\]\((<[^>\n]+>|[^\s()]+(?:\([^\n()]*\)[^\s()]*)*)(?:\s+"[^"\n]*")?\)/g;
  let start = 0;
  for (const match of text.matchAll(pattern)) {
    if (match[1]) continue;
    const uri = (match[4] || "").replace(/^<|>$/g, "");
    const type =
      types[
        extname(uri.split(/[?#]/)[0] || "")
          .slice(1)
          .toLowerCase()
      ] || "";
    if (!match[2] && (!type || !/^(?:https?:\/\/|file:|sandbox:|\/|\.\.?\/)/.test(uri))) continue;
    // A Markdown web page (including a private repository's blob page) is a
    // navigable link, not downloadable Markdown bytes. Explicit resource blocks
    // can still identify remote documents as attachments.
    if (!match[2] && type === "text/markdown" && /^https?:\/\//i.test(uri)) continue;
    try {
      const file = attachment(uri, type || (match[2] ? "image/*" : ""), "", cwd);
      if (match.index > start) result.push({ type: "text", text: text.slice(start, match.index) });
      result.push({ type: "kite_attachment", file });
      start = match.index + match[0].length;
    } catch {
      /* A missing Markdown target remains a readable link. */
    }
  }
  if (start < text.length) result.push({ type: "text", text: text.slice(start) });
  return result;
}

/** One content contract for human, agent and tool messages, in source order. */
export function projectContent(value: Json | undefined, cwd = ""): ObjectValue[] {
  if (typeof value === "string") return markdown(value, cwd);
  if (!Array.isArray(value)) return [];
  return value.flatMap((value) => {
    if (!value || typeof value !== "object" || Array.isArray(value)) return [];
    const block = optionalObject(value);
    if (block.type === "tool_result")
      return [{ ...block, content: projectContent(block.content, cwd) }];
    if (["text", "input_text", "output_text"].includes(optionalString(block.type)))
      return markdown(optionalString(block.text), cwd);
    if (
      [
        "image",
        "input_image",
        "output_image",
        "image_url",
        "audio",
        "input_audio",
        "output_audio",
        "video",
        "video_url",
        "file",
        "input_file",
        "output_file",
        "document",
        "resource",
        "resource_link",
      ].includes(optionalString(block.type))
    ) {
      if (block.type === "resource" && typeof optionalObject(block.resource).text === "string")
        return [{ type: "text", text: optionalObject(block.resource).text }];
      try {
        const projected = projectMedia(block, cwd);
        if (projected) return [projected];
      } catch {
        /* Preserve a visible placeholder without leaking inline bytes. */
      }
      return [
        {
          type: "text",
          text: `${optionalString(block.name || block.title) || "附件"}（暂不可用）`,
        },
      ];
    }
    return [block];
  });
}

/** ACP text chunks append to text; media starts a new ordered content block. */
export function appendContent(previous: ObjectValue[], content: Json): ObjectValue[] {
  const block = optionalObject(content);
  const last = previous.at(-1);
  if (block.type === "text" && last?.type === "text")
    return [
      ...previous.slice(0, -1),
      { type: "text", text: optionalString(last.text) + optionalString(block.text) },
    ];
  return [...previous, block];
}

/** Input acknowledgement survives Markdown being projected into media blocks. */
export function contentFingerprint(value: Json | undefined): string | undefined {
  const text =
    typeof value === "string"
      ? value
      : Array.isArray(value)
        ? value
            .filter((value) => value && typeof value === "object" && !Array.isArray(value))
            .map((value) => optionalObject(value))
            .map((block) => optionalString(block.text))
            .filter(
              (text) => text && text !== "</image>" && !text.startsWith("<image name=[Image #"),
            )
            .join("\n")
        : "";
  return text ? sha256(text) : undefined;
}
