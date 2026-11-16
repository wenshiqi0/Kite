import {
  closeSync,
  constants,
  existsSync,
  fsyncSync,
  lstatSync,
  mkdirSync,
  openSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join } from "node:path";
import {
  array,
  check,
  type Json,
  MAX_FILE,
  type Model,
  type NativeBlock,
  object,
  string,
} from "@kite/protocol";
import {
  atomicBytes,
  home,
  privateDir,
  readOwnedAt,
  requireOwnedFile,
  sha256,
} from "@kite/runtime";
import { stateDir } from "./catalog";
export function filename(name: string): void {
  check(
    name.length > 0 &&
      Buffer.byteLength(name) <= 180 &&
      ![".", ".."].includes(name) &&
      !/[/\\\0\r\n]/.test(name),
    "invalid filename",
  );
}
export const uploadRoot = () => privateDir(join(home(), ".local/share/kite/uploads"));
export async function upload(
  id: string,
  name: string,
  size: number,
  digest: string,
  source: ReadableStream<Uint8Array>,
): Promise<Json> {
  check(
    /^[a-f0-9]{32}$/.test(id) &&
      Number.isSafeInteger(size) &&
      size > 0 &&
      size <= MAX_FILE &&
      /^[a-f0-9]{64}$/.test(digest),
    "invalid upload",
  );
  filename(name);
  const dir = join(uploadRoot(), id);
  mkdirSync(dir, { mode: 0o700 });
  const tmp = join(dir, ".part");
  let fd: number | undefined;
  const reader = source.getReader();
  try {
    fd = openSync(tmp, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL, 0o600);
    const hasher = new Bun.CryptoHasher("sha256");
    let count = 0;
    // The SSH client waits for this receipt before closing its input stream.
    // The declared length frames the body; EOF is not part of this protocol.
    while (count < size) {
      const next = await reader.read();
      check(!next.done, "incomplete upload");
      const bytes = next.value;
      count += bytes.length;
      check(count <= size, "upload exceeds declared size");
      hasher.update(bytes);
      writeFileSync(fd, bytes);
    }
    check(count === size, "incomplete upload");
    check(hasher.digest("hex") === digest, "upload checksum mismatch");
    fsyncSync(fd);
    closeSync(fd);
    fd = undefined;
    const path = join(dir, name);
    renameSync(tmp, path);
    return { path, sha256: digest, size };
  } catch (error) {
    if (fd !== undefined) closeSync(fd);
    rmSync(dir, { recursive: true, force: true });
    throw error;
  } finally {
    reader.releaseLock();
  }
}
export function uploadedFile(id: string, name: string): Buffer {
  check(/^[a-f0-9]{32}$/.test(id), "invalid attachment identity");
  filename(name);
  const directory = join(uploadRoot(), id);
  check(!lstatSync(directory).isSymbolicLink(), "invalid upload directory");
  const fd = openSync(directory, constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
  try {
    return readOwnedAt(fd, name, MAX_FILE);
  } finally {
    closeSync(fd);
  }
}
export async function attachmentRead(id: string, name: string): Promise<Buffer> {
  check(/^[a-f0-9]{32}$/.test(id), "invalid attachment identity");
  filename(name);
  if (!existsSync(join(uploadRoot(), id, name))) {
    const { fetchMedia } = await import("./media");
    await fetchMedia(id, name);
  }
  const data = uploadedFile(id, name);
  return Buffer.concat([
    Buffer.from(`${JSON.stringify({ name, size: data.length, sha256: sha256(data) })}\n`),
    data,
  ]);
}
export function mime(data: Uint8Array): string | undefined {
  const b = Buffer.from(data);
  if (b.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) return "image/png";
  if (b.subarray(0, 3).equals(Buffer.from([255, 216, 255]))) return "image/jpeg";
  if (["GIF87a", "GIF89a"].includes(b.subarray(0, 6).toString())) return "image/gif";
  if (b.subarray(0, 4).toString() === "RIFF" && b.subarray(8, 12).toString() === "WEBP")
    return "image/webp";
  return undefined;
}
export function imageStore(): string {
  return privateDir(join(stateDir(), "generated-images"));
}
export function storeGeneratedBytes(data: Buffer): Json {
  check(data.length > 0 && data.length <= MAX_FILE, "invalid generated image size");
  const type = mime(data);
  check(type && type !== "image/gif", "unsupported generated image format");
  const id = sha256(data),
    path = join(imageStore(), id);
  if (!existsSync(path) || sha256(requireOwnedFile(path, MAX_FILE)) !== id) atomicBytes(path, data);
  return {
    id,
    name: `generated-${id.slice(0, 12)}.${type === "image/png" ? "png" : type === "image/jpeg" ? "jpg" : "webp"}`,
    mime: type,
    size: data.length,
    path: `kite-image:${id}`,
  };
}
export function storeGeneratedImage(encoded: string): Json {
  check(encoded.length <= Math.ceil(MAX_FILE / 3) * 4 + 128, "generated image exceeds 25 MiB");
  const text = encoded.startsWith("data:image/")
    ? encoded.slice(encoded.indexOf(",") + 1)
    : encoded;
  check(
    /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(text),
    "invalid base64 image",
  );
  return storeGeneratedBytes(Buffer.from(text, "base64"));
}
export function readGeneratedImage(id: string): Buffer {
  check(/^[a-f0-9]{64}$/.test(id), "invalid generated image identity");
  const data = requireOwnedFile(join(imageStore(), id), MAX_FILE);
  check(sha256(data) === id, "generated image changed");
  return data;
}
export function nativeInput(value: unknown, model: Model): NativeBlock[] {
  const blocks = array(value);
  check(blocks.length > 0 && blocks.length <= 6, "invalid harness input");
  let total = 0;
  const result: NativeBlock[] = blocks.map((value) => {
    const b = object(value);
    if (b.type === "text") {
      const text = string(b.text);
      const size = Buffer.byteLength(text);
      check(size <= 1024 * 1024, "harness text exceeds limit");
      total += size;
      return { type: "text", text };
    }
    check(
      b.type === "localImage" && model.inputModalities.includes("image"),
      "input modality unavailable",
    );
    const path = string(b.path);
    const parent = dirname(path);
    const root = join(home(), ".local/share/kite/uploads");
    check(
      dirname(parent) === root &&
        /^[a-f0-9]{32}$/.test(basename(parent)) &&
        !lstatSync(root).isSymbolicLink(),
      "image must be an owned upload",
    );
    const data = uploadedFile(basename(parent), basename(path));
    const type = mime(data);
    check(type, "unsupported image encoding");
    const encoded = data.toString("base64");
    total += encoded.length;
    return { type: "image", mimeType: type, data: encoded };
  });
  check(
    total <= 36 * 1024 * 1024 && result.some((b) => b.type === "text" && b.text.trim()),
    "invalid harness prompt",
  );
  return result;
}
