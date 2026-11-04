import { closeSync, existsSync, openSync, writeFileSync } from "node:fs";
import { createConnection, createServer } from "node:net";
import { hostname } from "node:os";
import { isAbsolute, join } from "node:path";
import { parseArgs } from "node:util";
import { proc, userManagerCgroup } from "@kite/platform/resources";
import {
  array,
  check,
  type Json,
  object,
  optionalString,
  parse,
  type RpcMessage,
  string,
} from "@kite/protocol";
import { ACP_MESSAGE_LIMIT, ACP_QUEUE_LIMIT, acpMessage } from "@kite/protocol/acp";
import { KiteAcp } from "@kite/providers/acp";
import { providers } from "@kite/providers/harness";
import { home, input, lines, privateDir, requireOwnedFile, sleep } from "@kite/runtime";
import { type AcpClient, connectAcp, serveAcp } from "@kite/runtime/acp";
import { optionalObject } from "@kite/runtime/socket";

interface Peer {
  url: string;
  tokenFile: string;
  sshTarget?: string;
}
function writeWire(stream: NodeJS.WriteStream, value: Json | RpcMessage): void {
  const line = JSON.stringify(value) + "\n";
  check(
    Buffer.byteLength(line) + stream.writableLength <= ACP_QUEUE_LIMIT,
    "ACP stdio output is backpressured",
  );
  stream.write(line);
}
function peer(file: string, name: string): Peer {
  check(/^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,63}$/.test(name), "invalid ACP peer name");
  const row = object(object(object(parse(requireOwnedFile(file, 65536).toString())).peers)[name]),
    tokenFile = string(row.tokenFile),
    url = string(row.url),
    sshTarget = optionalString(row.sshTarget);
  check(isAbsolute(tokenFile), "ACP token file must be absolute");
  if (sshTarget)
    check(
      /^(?:[a-zA-Z0-9_][a-zA-Z0-9_.-]*@)?[a-zA-Z0-9_][a-zA-Z0-9_.-]*$/.test(sshTarget),
      "invalid SSH peer target",
    );
  return { url, tokenFile, ...(sshTarget ? { sshTarget } : {}) };
}
export function acpToken(file: string, create = false): string {
  check(isAbsolute(file), "ACP token path must be absolute");
  if (create && !existsSync(file)) {
    privateDir(join(file, ".."));
    const fd = openSync(file, "wx", 0o600);
    try {
      writeFileSync(
        fd,
        `${crypto.randomUUID().replaceAll("-", "")}${crypto.randomUUID().replaceAll("-", "")}\n`,
      );
    } finally {
      closeSync(fd);
    }
  }
  const value = requireOwnedFile(file, 256).toString().trim();
  check(/^[a-zA-Z0-9_-]{32,128}$/.test(value), "invalid ACP token file");
  return value;
}
async function unusedPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  check(address && typeof address !== "string", "local forward port unavailable");
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
  return address.port;
}
async function listening(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = createConnection({ host: "127.0.0.1", port });
    const timer = setTimeout(() => {
      socket.destroy();
      resolve(false);
    }, 250);
    socket.once("connect", () => {
      clearTimeout(timer);
      socket.destroy();
      resolve(true);
    });
    socket.once("error", () => {
      clearTimeout(timer);
      socket.destroy();
      resolve(false);
    });
  });
}
async function connectPeer(
  target: Peer,
  receive: (message: RpcMessage) => void,
): Promise<AcpClient> {
  let url = target.url,
    tunnel: Bun.Subprocess<"ignore", "ignore", "ignore"> | undefined;
  try {
    if (target.sshTarget) {
      const endpoint = new URL(url);
      check(
        endpoint.protocol === "ws:" &&
          ["127.0.0.1", "localhost", "[::1]"].includes(endpoint.hostname) &&
          endpoint.port,
        "SSH ACP peer must name the remote loopback port",
      );
      const localPort = await unusedPort();
      tunnel = Bun.spawn(
        [
          "ssh",
          "-o",
          "BatchMode=yes",
          "-o",
          "StrictHostKeyChecking=yes",
          "-o",
          "ExitOnForwardFailure=yes",
          "-o",
          "ConnectTimeout=8",
          "-N",
          "-L",
          `127.0.0.1:${localPort}:127.0.0.1:${endpoint.port}`,
          target.sshTarget,
        ],
        { stdin: "ignore", stdout: "ignore", stderr: "ignore" },
      );
      const deadline = Date.now() + 10000;
      while (!(await listening(localPort))) {
        check(
          tunnel.exitCode === null && Date.now() < deadline,
          "SSH ACP forward unavailable; verify existing SSH trust and connectivity",
        );
        await sleep(50);
      }
      endpoint.hostname = "127.0.0.1";
      endpoint.port = String(localPort);
      url = endpoint.toString();
    }
    const client = await connectAcp(url, acpToken(target.tokenFile), receive),
      forward = tunnel;
    void client.closed.finally(() => forward?.kill());
    return {
      ...client,
      close() {
        client.close();
        forward?.kill();
      },
    };
  } catch (error) {
    tunnel?.kill();
    throw error;
  }
}

export class AcpRpc {
  private client?: AcpClient;
  private nextId = 0;
  private pending = new Map<
    number,
    { resolve(value: Json): void; reject(error: Error): void; timer: ReturnType<typeof setTimeout> }
  >();
  constructor(
    private receive: (message: RpcMessage, reply: (result: Json) => void) => void = () => {},
  ) {}
  async connect(target: Peer): Promise<void> {
    this.client = await connectPeer(target, (message) => {
      if (!message.method && typeof message.id === "number") {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        clearTimeout(pending.timer);
        if (message.error) pending.reject(new Error(message.error.message));
        else pending.resolve(message.result ?? {});
      } else {
        const id = message.id;
        this.receive(message, (result) => {
          check(id !== undefined, "cannot reply to a notification");
          this.client?.send({ id, result });
        });
      }
    });
    void this.client.closed.then(() =>
      this.rejectPending("ACP disconnected; submitted work is not replayed"),
    );
  }
  private rejectPending(message: string): void {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error(message));
    }
    this.pending.clear();
  }
  call(method: string, params: Json, timeout = 40000): Promise<Json> {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error("ACP request timed out; inspect the remote session before resubmitting"));
      }, timeout);
      this.pending.set(id, { resolve, reject, timer });
      try {
        check(this.client, "ACP is not connected");
        this.client.send({ id, method, params });
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(error);
      }
    });
  }
  notify(method: string, params: Json): void {
    this.client?.send({ method, params });
  }
  close(): void {
    this.client?.close();
    this.rejectPending("ACP client closed");
  }
}

export interface AcpTaskOptions {
  peer: Peer;
  cwd: string;
  prompt: string;
  sessionId?: string | undefined;
  provider?: string | undefined;
  expectedRevision?: string | undefined;
  allowCommands?: string[];
  signal?: AbortSignal;
  detachSignal?: AbortSignal;
  event?(message: RpcMessage): void;
}
/** A task always names its target. Only exact, explicitly supplied commands get one-use approval. */
export async function acpTask(options: AcpTaskOptions): Promise<Json> {
  let replyBytes = 0,
    submitted = false,
    activeSession = "";
  let cancelTimer: ReturnType<typeof setTimeout> | undefined;
  const replies: string[] = [],
    rpc = new AcpRpc((message, reply) => {
      options.event?.(message);
      if (message.method === "session/update") {
        const update = optionalObject(optionalObject(message.params).update);
        if (update.sessionUpdate === "agent_message_chunk") {
          const content = optionalObject(update.content);
          if (content.type !== "text") return;
          const text = string(content.text);
          replyBytes += Buffer.byteLength(text);
          check(replyBytes <= ACP_MESSAGE_LIMIT, "remote reply exceeds task result limit");
          replies.push(text);
        }
      } else if (message.method === "session/request_permission") {
        if (!submitted) return;
        const params = optionalObject(message.params),
          tool = optionalObject(params.toolCall),
          raw = optionalObject(tool.rawInput),
          approved =
            !options.signal?.aborted &&
            options.allowCommands?.includes(optionalString(raw.command)) === true &&
            raw.cwd === options.cwd,
          choices = array(params.options).map(object),
          choice = choices.find((row) => row.kind === (approved ? "allow_once" : "reject_once"));
        reply(
          choice
            ? { outcome: { outcome: "selected", optionId: string(choice.optionId) } }
            : { outcome: { outcome: "cancelled" } },
        );
      }
    });
  const detach = () => rpc.close();
  const cancel = () => {
    try {
      if (submitted && activeSession) rpc.notify("session/cancel", { sessionId: activeSession });
      else rpc.close();
    } catch {
      rpc.close();
    }
    cancelTimer = setTimeout(() => rpc.close(), 5000);
  };
  check(
    !options.signal?.aborted && !options.detachSignal?.aborted,
    "ACP task was cancelled before connection",
  );
  options.signal?.addEventListener("abort", cancel, { once: true });
  options.detachSignal?.addEventListener("abort", detach, { once: true });
  try {
    await rpc.connect(options.peer);
    check(
      !options.signal?.aborted && !options.detachSignal?.aborted,
      "ACP task cancelled before initialization",
    );
    await rpc.call("initialize", {
      protocolVersion: 1,
      clientInfo: { name: "kite-task", version: "1" },
      clientCapabilities: {},
    });
    const session = object(
        await rpc.call(
          options.sessionId ? "session/resume" : "session/new",
          {
            ...(options.sessionId ? { sessionId: options.sessionId } : {}),
            cwd: options.cwd,
            mcpServers: [],
            _meta: { kite: { provider: options.provider || "codex" } },
          },
          90_000,
        ),
      ),
      sessionId = string(session.sessionId);
    check(
      !options.signal?.aborted && !options.detachSignal?.aborted,
      "ACP task cancelled before submission",
    );
    activeSession = sessionId;
    options.event?.({ method: "_kite/session-attached", params: { sessionId } });
    submitted = true;
    const result = await rpc.call(
      "session/prompt",
      {
        sessionId,
        prompt: [{ type: "text", text: options.prompt }],
        ...(options.expectedRevision
          ? { _meta: { kite: { expectedRevision: options.expectedRevision } } }
          : {}),
      },
      10 * 60_000 + 5000,
    );
    return { sessionId, result, text: replies.join("") };
  } catch (error) {
    if (activeSession)
      throw new Error(
        `${error instanceof Error ? error.message : "ACP task failed"}; remote session: ${activeSession}`,
      );
    throw error;
  } finally {
    if (cancelTimer) clearTimeout(cancelTimer);
    options.signal?.removeEventListener("abort", cancel);
    options.detachSignal?.removeEventListener("abort", detach);
    rpc.close();
  }
}

async function mcp(file: string): Promise<void> {
  let initialized = false,
    closed = false;
  const detach = new AbortController(),
    running = new Map<string | number, { cancel: AbortController; done: Promise<void> }>();
  const output = (id: string | number, result: Json) => {
    if (closed) return;
    const line = JSON.stringify({ jsonrpc: "2.0", id, result }) + "\n";
    check(
      Buffer.byteLength(line) + process.stdout.writableLength <= ACP_QUEUE_LIMIT,
      "MCP output is backpressured",
    );
    process.stdout.write(line);
  };
  const handle = async (request: RpcMessage, signal: AbortSignal): Promise<Json> => {
    const params = optionalObject(request.params);
    if (request.method === "ping") return {};
    if (request.method === "initialize") {
      check(!initialized, "MCP connection already initialized");
      initialized = true;
      return {
        protocolVersion: "2024-11-05",
        serverInfo: { name: "kite-acp", version: "1" },
        capabilities: { tools: {} },
      };
    }
    check(initialized, "initialize MCP first");
    if (request.method === "tools/list")
      return {
        tools: [
          {
            name: "kite_remote_task",
            description:
              "Submit an explicit task to a named remote Kite session over authenticated ACP. The target keeps its workspace, credentials and owner; requests are never replayed.",
            inputSchema: {
              type: "object",
              additionalProperties: false,
              required: ["peer", "cwd", "prompt"],
              properties: {
                peer: { type: "string" },
                cwd: { type: "string" },
                prompt: { type: "string" },
                sessionId: { type: "string" },
                provider: { type: "string", enum: ["codex", "ply", "dsh"] },
                expectedRevision: { type: "string" },
                allowCommands: {
                  type: "array",
                  maxItems: 16,
                  items: { type: "string" },
                  description:
                    "Exact commands explicitly authorized for this task; all other permissions are rejected.",
                },
              },
            },
            annotations: {
              readOnlyHint: false,
              destructiveHint: true,
              idempotentHint: false,
              openWorldHint: true,
            },
          },
        ],
      };
    check(
      request.method === "tools/call" && params.name === "kite_remote_task",
      "unknown ACP tool",
    );
    const args = optionalObject(params.arguments),
      commands = args.allowCommands === undefined ? [] : array(args.allowCommands).map(string);
    check(commands.length <= 16, "too many command authorizations");
    const value = await acpTask({
      peer: peer(file, string(args.peer)),
      cwd: string(args.cwd),
      prompt: string(args.prompt),
      sessionId: optionalString(args.sessionId) || undefined,
      provider: optionalString(args.provider) || "codex",
      expectedRevision: optionalString(args.expectedRevision) || undefined,
      allowCommands: commands,
      signal,
      detachSignal: detach.signal,
    });
    return {
      content: [{ type: "text", text: JSON.stringify(value) }],
      structuredContent: value,
      isError: false,
    };
  };
  try {
    for await (const line of lines(Bun.stdin.stream(), ACP_MESSAGE_LIMIT)) {
      const request = acpMessage(parse(line)),
        params = optionalObject(request.params),
        id = request.id;
      if (id === undefined || id === null) {
        if (request.method === "notifications/cancelled") {
          const target = params.requestId;
          if (typeof target === "string" || typeof target === "number")
            running.get(target)?.cancel.abort();
        }
        continue;
      }
      if (running.size >= 4) {
        output(id, {
          content: [{ type: "text", text: "Remote task concurrency limit reached" }],
          isError: true,
        });
        continue;
      }
      check(!running.has(id), "duplicate MCP request identity");
      const cancel = new AbortController(),
        done = handle(request, cancel.signal)
          .then((result) => output(id, result))
          .catch((error) =>
            output(id, {
              content: [
                {
                  type: "text",
                  text: error instanceof Error ? error.message : "Remote task failed",
                },
              ],
              isError: true,
            }),
          )
          .finally(() => running.delete(id));
      running.set(id, { cancel, done });
    }
  } finally {
    closed = true;
    detach.abort();
    await Promise.allSettled([...running.values()].map((job) => job.done));
  }
}

export async function acpMain(argv: string[]): Promise<void> {
  const action = argv[0],
    { values, positionals } = parseArgs({
      args: argv.slice(1),
      strict: true,
      allowPositionals: false,
      options: {
        port: { type: "string", default: "0" },
        workspace: { type: "string", multiple: true },
        "token-file": { type: "string" },
        herdr: { type: "string" },
        provider: { type: "string", default: "codex" },
        peers: { type: "string" },
        peer: { type: "string" },
        cwd: { type: "string" },
        session: { type: "string" },
        "expected-revision": { type: "string" },
        "allow-command": { type: "string", multiple: true },
      },
    });
  check(!positionals.length, "unexpected ACP arguments");
  const root = join(home(), ".config/kite/acp"),
    file = values.peers || join(root, "peers.json");
  if (action === "serve") {
    if (process.platform === "linux" && process.env.KITE_AGENT_RESOURCE_CONTROL === "1") {
      try {
        const current = proc(process.pid).cgroup,
          manager = (await userManagerCgroup(current)).trim();
        check(
          manager && (current === manager || current.startsWith(`${manager}/`)),
          "ACP resource hierarchy unavailable",
        );
      } catch {
        throw new Error(
          "Run the ACP gateway as a systemd user service so workers retain the configured resource budget (systemd-run --user --collect kite acp serve ...)",
        );
      }
    }
    const port = Number(values.port);
    check(Number.isInteger(port) && port >= 0 && port <= 65535, "invalid ACP port");
    check(
      values.workspace?.length && ["codex", "ply", "dsh"].includes(values.provider),
      "ACP serve requires workspace roots and a supported default provider",
    );
    const newSessionProviders = (await providers())
      .map(object)
      .map((row) => optionalString(row.id))
      .filter((id) => id === "codex" || id === "ply" || id === "dsh");
    check(
      newSessionProviders.some((provider) => provider === values.provider),
      "the default ACP provider is not verified on this host",
    );
    const server = serveAcp({
      token: acpToken(values["token-file"] || join(root, "token"), true),
      port,
      connect: (send) =>
        new KiteAcp(
          {
            machine: hostname(),
            workspaces: values.workspace || [],
            herdr: values.herdr || process.env.KITE_ACP_HERDR || "herdr",
            defaultProvider:
              values.provider === "dsh" ? "dsh" : values.provider === "ply" ? "ply" : "codex",
            newSessionProviders,
          },
          send,
        ),
    });
    process.stdout.write(
      `${JSON.stringify({ machine: hostname(), url: `ws://127.0.0.1:${server.port}/acp`, pid: process.pid, tokenFile: values["token-file"] || join(root, "token") })}\n`,
    );
    const stop = () => {
      server.stop(true);
      process.exit(0);
    };
    process.once("SIGTERM", stop);
    process.once("SIGINT", stop);
    return;
  }
  if (action === "mcp") {
    await mcp(file);
    return;
  }
  const target = peer(file, string(values.peer));
  if (action === "task") {
    const prompt = (await input(1024 * 1024)).toString();
    check(prompt.trim(), "ACP task prompt is empty");
    const cancel = new AbortController(),
      interrupt = () => cancel.abort();
    process.once("SIGINT", interrupt);
    process.once("SIGTERM", interrupt);
    try {
      const result = await acpTask({
        peer: target,
        cwd: string(values.cwd),
        prompt,
        sessionId: values.session,
        provider: values.provider,
        expectedRevision: values["expected-revision"],
        allowCommands: values["allow-command"] || [],
        signal: cancel.signal,
        event: (message) => writeWire(process.stderr, message),
      });
      writeWire(process.stdout, result);
    } finally {
      process.removeListener("SIGINT", interrupt);
      process.removeListener("SIGTERM", interrupt);
    }
    return;
  }
  check(action === "proxy", "ACP action must be serve, proxy, task or mcp");
  const client = await connectPeer(target, (message) => writeWire(process.stdout, message));
  const write = (async () => {
    for await (const line of lines(Bun.stdin.stream(), ACP_MESSAGE_LIMIT))
      client.send(acpMessage(parse(line)));
  })();
  try {
    await Promise.race([client.closed, write]);
  } finally {
    client.close();
  }
}
