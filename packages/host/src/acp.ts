import { closeSync, existsSync, openSync, writeFileSync } from "node:fs";
import { createConnection, createServer } from "node:net";
import { hostname } from "node:os";
import { isAbsolute, join } from "node:path";
import { parseArgs } from "node:util";
import { proc, userManagerCgroup } from "@kite/platform/resources";
import {
  array,
  boundedString,
  CHAT_PROVIDERS,
  check,
  DELEGATION_POLICIES,
  isChatProvider,
  type Json,
  json,
  type ObjectValue,
  object,
  optionalString,
  parse,
  REMOTE_SESSION_STATES,
  type RpcMessage,
  string,
} from "@kite/protocol";
import {
  ACP_MAX_WAIT_SESSIONS,
  ACP_MESSAGE_LIMIT,
  ACP_QUEUE_LIMIT,
  ACP_READ_LIMIT,
  acpLimit,
  acpMessage,
  acpSessionId,
} from "@kite/protocol/acp";
import { kiteAcpConnector } from "@kite/providers/acp";
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
  const line = `${JSON.stringify(value)}\n`;
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
  signal?: AbortSignal,
): Promise<AcpClient> {
  let url = target.url,
    tunnel: Bun.Subprocess<"ignore", "ignore", "ignore"> | undefined;
  try {
    signal?.throwIfAborted();
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
        signal?.throwIfAborted();
        await sleep(50);
      }
      endpoint.hostname = "127.0.0.1";
      endpoint.port = String(localPort);
      url = endpoint.toString();
    }
    const client = await connectAcp(url, acpToken(target.tokenFile), receive, signal),
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
  private disconnect = new AbortController();
  private pending = new Map<
    number,
    { resolve(value: Json): void; reject(error: Error): void; timer: ReturnType<typeof setTimeout> }
  >();
  constructor(
    private receive: (message: RpcMessage, reply: (result: Json) => void) => void = () => {},
  ) {}
  async connect(target: Peer): Promise<void> {
    this.client = await connectPeer(
      target,
      (message) => {
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
      },
      this.disconnect.signal,
    );
    if (this.disconnect.signal.aborted) this.client.close();
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
    this.disconnect.abort();
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

const textField = { type: "string" };
const boolField = { type: "boolean" };
const peerField = { peer: textField };
const targetFields = { ...peerField, sessionId: textField };
const choices = (...values: string[]) => ({ type: "string", enum: values });
const items = (schema: Json, maxItems: number) => ({ type: "array", items: schema, maxItems });
const limit = (maximum: number, minimum = 0) => ({ type: "integer", minimum, maximum });
const schema = (required: string[], properties: ObjectValue) => ({
  type: "object",
  additionalProperties: false,
  required,
  properties,
});
const selectionFields = {
  model: textField,
  effort: textField,
  serviceTier: textField,
  policy: choices(...DELEGATION_POLICIES),
};
const operations = {
  spawn: {
    method: "session/new",
    description: "Create a session and acquire its stable identity.",
    schema: schema(["peer", "cwd", "provider"], {
      ...peerField,
      cwd: textField,
      provider: choices(...CHAT_PROVIDERS),
      title: textField,
      ...selectionFields,
      parent: schema(["machine", "sessionId"], {
        machine: textField,
        sessionId: textField,
        provider: textField,
      }),
      label: textField,
      role: textField,
      writeLock: boolField,
      worktreeIsolated: boolField,
    }),
  },
  send: {
    method: "_kite/session/submit",
    description: "Submit one turn. Query status after uncertain delivery before sending again.",
    schema: schema(["peer", "sessionId", "prompt"], {
      ...targetFields,
      ...selectionFields,
      prompt: textField,
      attachments: items(textField, 63),
      expectedRevision: textField,
      allowCommands: items(textField, 16),
      waitMs: limit(30_000),
    }),
  },
  status: {
    method: "_kite/session/status",
    description: "Query the owner's turn and any pending interaction.",
    schema: schema(["peer", "sessionId"], targetFields),
  },
  read: {
    method: "_kite/session/read",
    description: "Read a bounded public reply and tool summaries.",
    schema: schema(["peer", "sessionId"], {
      ...targetFields,
      turnId: textField,
      maxBytes: limit(ACP_READ_LIMIT, 1),
      includeToolSummaries: boolField,
    }),
  },
  list: {
    method: "session/list",
    description: "List one page of workload sessions and their lineage.",
    schema: schema(["peer"], {
      ...peerField,
      parentSessionId: textField,
      parentMachine: textField,
      label: textField,
      states: items(choices(...REMOTE_SESSION_STATES), 6),
      includeArchived: boolField,
      cursor: textField,
    }),
  },
  wait: {
    method: "_kite/session/wait_any",
    description: "Wait for any target to become ready or need human input.",
    schema: schema(["targets"], {
      targets: items(
        schema(["peer", "sessionId"], { ...targetFields, since: textField }),
        ACP_MAX_WAIT_SESSIONS,
      ),
      timeoutMs: limit(60_000),
    }),
  },
  cancel: {
    method: "session/cancel",
    description: "Explicitly interrupt the target's active turn.",
    schema: schema(["peer", "sessionId"], targetFields),
  },
  release: {
    method: "_kite/session/release",
    description: "Archive and release an owner with durable history.",
    schema: schema(["peer", "sessionId"], { ...targetFields, forceCancel: boolField }),
  },
};
type Operation = keyof typeof operations;
function operation(value: unknown): Operation | undefined {
  return (Object.keys(operations) as Operation[]).find((key) => key === value);
}

async function controlPeer(
  target: Peer,
  signal: AbortSignal,
  method: string,
  params: Json,
  timeout = 40_000,
) {
  const rpc = new AcpRpc();
  const close = () => rpc.close();
  signal.throwIfAborted();
  signal.addEventListener("abort", close, { once: true });
  try {
    await rpc.connect(target);
    const hello = object(
      await rpc.call("initialize", {
        protocolVersion: 1,
        clientInfo: { name: "kite-session-control", version: "2" },
        clientCapabilities: {},
      }),
    );
    check(
      optionalObject(optionalObject(hello._meta).kite).controlPlaneVersion === 2,
      "peer does not advertise Kite session control v2",
    );
    signal.throwIfAborted();
    return await rpc.call(method, params, timeout);
  } finally {
    signal.removeEventListener("abort", close);
    rpc.close();
  }
}

async function waitAny(file: string, args: ObjectValue, signal: AbortSignal) {
  const targets = array(args.targets).map(object);
  check(targets.length > 0 && targets.length <= ACP_MAX_WAIT_SESSIONS, "invalid wait target count");
  const groups = new Map<string, { sessionIds: string[]; since: ObjectValue }>();
  for (const target of targets) {
    const name = string(target.peer),
      id = acpSessionId(target.sessionId);
    const group = groups.get(name) ?? { sessionIds: [], since: {} };
    check(!group.sessionIds.includes(id), "duplicate wait target");
    group.sessionIds.push(id);
    if (target.since !== undefined) group.since[id] = boundedString(target.since, 128);
    groups.set(name, group);
  }
  const timeoutMs = acpLimit(args.timeoutMs, 60_000, 60_000);
  const stop = new AbortController(),
    combined = AbortSignal.any([signal, stop.signal]);
  const jobs = [...groups].map(async ([name, group]) => {
    const result = object(
      await controlPeer(
        peer(file, name),
        combined,
        operations.wait.method,
        {
          ...group,
          timeoutMs,
        },
        timeoutMs + 15_000,
      ),
    );
    return { peer: name, result };
  });
  try {
    // A timeout/error on one peer must not mask an available result on another.
    const first = await Promise.any(
      jobs.map(async (job) => {
        const answer = await job;
        check(answer.result.timedOut === false, "wait timed out");
        return answer;
      }),
    ).catch(() => undefined);
    if (first) return { ...first.result, peer: first.peer };
    signal.throwIfAborted();
    const settled = await Promise.allSettled(jobs);
    const sessions: Json[] = [],
      errors: Json[] = [];
    for (const [index, item] of settled.entries()) {
      if (item.status === "fulfilled") {
        const { peer: name, result } = item.value;
        for (const session of array(result.sessions))
          sessions.push({ ...object(session), peer: name });
      } else {
        const name = [...groups.keys()][index];
        errors.push({
          peer: name,
          message: item.reason instanceof Error ? item.reason.message : "peer wait failed",
        });
      }
    }
    if (errors.length === groups.size) throw new Error(String(object(errors[0]).message));
    return { timedOut: true, sessions, ...(errors.length ? { errors } : {}) };
  } finally {
    stop.abort();
  }
}

async function sessionControl(
  file: string,
  action: Operation,
  args: ObjectValue,
  signal: AbortSignal,
): Promise<Json> {
  const entry = operations[action];
  check(
    Object.keys(args).every((key) => Object.hasOwn(entry.schema.properties, key)),
    "unknown session control argument",
  );
  if (action === "wait") return waitAny(file, args, signal);
  const { peer: name, ...params } = args;
  const target = peer(file, string(name));
  if (action === "spawn") {
    const {
      cwd,
      title,
      provider,
      model,
      effort,
      serviceTier,
      parent,
      role,
      label,
      policy,
      writeLock,
      worktreeIsolated,
    } = params;
    check(isChatProvider(provider), "invalid remote provider");
    return controlPeer(
      target,
      signal,
      entry.method,
      {
        cwd: string(cwd),
        mcpServers: [],
        _meta: {
          kite: {
            controlPlaneVersion: 2,
            provider,
            title,
            model,
            effort,
            serviceTier,
            delegation: {
              policy: policy ?? "strict",
              role,
              parent,
              label,
              writeLock,
              worktreeIsolated,
            },
          },
        },
      },
      90_000,
    );
  }
  if (action === "list") {
    const { cursor, ...filters } = params;
    return controlPeer(target, signal, entry.method, { cursor, _meta: { kite: filters } });
  }
  acpSessionId(params.sessionId);
  if (action === "send") {
    const { prompt, attachments, ...fields } = params;
    const text = string(prompt);
    const links = attachments === undefined ? [] : array(attachments).map(string);
    check((text.trim() || links.length) && links.length <= 63, "invalid session prompt");
    return controlPeer(
      target,
      signal,
      entry.method,
      {
        ...fields,
        prompt: [{ type: "text", text }, ...links.map((uri) => ({ type: "resource_link", uri }))],
      },
      125_000,
    );
  }
  return controlPeer(target, signal, entry.method, params);
}

const toolName = (action: Operation) => `kite_session_${action === "wait" ? "wait_any" : action}`;
const controlTools = (Object.keys(operations) as Operation[]).map((action) => ({
  name: toolName(action),
  description: operations[action].description,
  inputSchema: operations[action].schema,
  annotations: {
    readOnlyHint: ["status", "read", "list", "wait"].includes(action),
    idempotentHint: action !== "spawn" && action !== "send",
    openWorldHint: true,
  },
}));
function mcpOperation(name: unknown) {
  return (Object.keys(operations) as Operation[]).find((key) => toolName(key) === name);
}
function mcpBucket(request: RpcMessage) {
  const action = mcpOperation(optionalObject(request.params).name);
  if (action === "wait") return "wait";
  if (action === "cancel") return "cancel";
  if (action === "status" || action === "read" || action === "list") return "read";
  return request.method === "tools/call" ? "write" : "meta";
}

async function mcp(file: string): Promise<void> {
  let initialized = false,
    closed = false;
  const detach = new AbortController(),
    running = new Map<
      string | number,
      { cancel: AbortController; done: Promise<void>; bucket: string }
    >();
  const output = (id: string | number, result: Json) => {
    if (closed) return;
    const line = `${JSON.stringify({ jsonrpc: "2.0", id, result })}\n`;
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
          ...controlTools,
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
                provider: { type: "string", enum: [...CHAT_PROVIDERS] },
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
    check(request.method === "tools/call", "unknown ACP tool");
    const args = optionalObject(params.arguments);
    const action = mcpOperation(params.name);
    const value = action
      ? await sessionControl(file, action, args, AbortSignal.any([signal, detach.signal]))
      : await (async () => {
          check(params.name === "kite_remote_task", "unknown ACP tool");
          const commands =
            args.allowCommands === undefined ? [] : array(args.allowCommands).map(string);
          check(commands.length <= 16, "too many command authorizations");
          return acpTask({
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
        })();
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
      const bucket = mcpBucket(request);
      if ([...running.values()].filter((job) => job.bucket === bucket).length >= 4) {
        output(id, {
          content: [{ type: "text", text: "Remote session request concurrency limit reached" }],
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
      running.set(id, { cancel, done, bucket });
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
        provider: { type: "string" },
        peers: { type: "string" },
        peer: { type: "string" },
        cwd: { type: "string" },
        session: { type: "string" },
        "expected-revision": { type: "string" },
        "allow-command": { type: "string", multiple: true },
        model: { type: "string" },
        effort: { type: "string" },
        "service-tier": { type: "string" },
        policy: { type: "string" },
        label: { type: "string" },
        role: { type: "string" },
        title: { type: "string" },
        "parent-machine": { type: "string" },
        "parent-session": { type: "string" },
        "parent-provider": { type: "string" },
        "write-lock": { type: "boolean" },
        "worktree-isolated": { type: "boolean" },
        attachment: { type: "string", multiple: true },
        "wait-ms": { type: "string" },
        turn: { type: "string" },
        "max-bytes": { type: "string" },
        "no-tools": { type: "boolean" },
        state: { type: "string", multiple: true },
        "include-archived": { type: "boolean" },
        cursor: { type: "string" },
        target: { type: "string", multiple: true },
        "timeout-ms": { type: "string" },
        "force-cancel": { type: "boolean" },
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
    const defaultProvider = values.provider ?? "codex";
    const machine = process.env.KITE_ACP_MACHINE || hostname();
    check(
      values.workspace?.length && isChatProvider(defaultProvider),
      "ACP serve requires workspace roots and a supported default provider",
    );
    const newSessionProviders = (await providers())
      .map(object)
      .map((row) => optionalString(row.id))
      .filter(isChatProvider);
    check(
      newSessionProviders.includes(defaultProvider),
      "the default ACP provider is not verified on this host",
    );
    const server = serveAcp({
      token: acpToken(values["token-file"] || join(root, "token"), true),
      port,
      connect: kiteAcpConnector({
        machine,
        workspaces: values.workspace,
        herdr: values.herdr || process.env.KITE_ACP_HERDR || "herdr",
        defaultProvider,
        newSessionProviders,
      }),
    });
    process.stdout.write(
      `${JSON.stringify({ machine, url: `ws://127.0.0.1:${server.port}/acp`, pid: process.pid, tokenFile: values["token-file"] || join(root, "token") })}\n`,
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
  const op = operation(action);
  if (op) {
    const args: ObjectValue = { peer: values.peer, sessionId: values.session };
    const selection = {
      model: values.model,
      effort: values.effort,
      serviceTier: values["service-tier"],
      policy: values.policy,
    };
    const numeric = (value: string | undefined) =>
      value === undefined ? undefined : Number(boundedString(value, 8));
    switch (op) {
      case "spawn":
        delete args.sessionId;
        Object.assign(args, selection, {
          cwd: values.cwd,
          provider: values.provider,
          title: values.title,
          label: values.label,
          role: values.role,
          writeLock: values["write-lock"],
          worktreeIsolated: values["worktree-isolated"],
          parent:
            values["parent-machine"] || values["parent-session"] || values["parent-provider"]
              ? {
                  machine: values["parent-machine"],
                  sessionId: values["parent-session"],
                  provider: values["parent-provider"],
                }
              : undefined,
        });
        break;
      case "send":
        Object.assign(args, selection, {
          prompt: (await input(1024 * 1024)).toString(),
          attachments: values.attachment,
          allowCommands: values["allow-command"],
          expectedRevision: values["expected-revision"],
          waitMs: numeric(values["wait-ms"]),
        });
        break;
      case "read":
        Object.assign(args, {
          turnId: values.turn,
          maxBytes: numeric(values["max-bytes"]),
          includeToolSummaries: values["no-tools"] ? false : undefined,
        });
        break;
      case "list":
        delete args.sessionId;
        Object.assign(args, {
          parentMachine: values["parent-machine"],
          parentSessionId: values["parent-session"],
          label: values.label,
          states: values.state,
          includeArchived: values["include-archived"],
          cursor: values.cursor,
        });
        break;
      case "wait":
        delete args.peer;
        delete args.sessionId;
        Object.assign(args, {
          targets: (
            values.target ??
            (values.peer && values.session ? [`${values.peer}/${values.session}`] : [])
          ).map((value) => {
            const [peer, sessionId, since, extra] = value.split("/");
            check(
              peer && sessionId && extra === undefined,
              "--target requires PEER/SESSION[/REVISION]",
            );
            return { peer, sessionId, since };
          }),
          timeoutMs: numeric(values["timeout-ms"]),
        });
        break;
      case "release":
        args.forceCancel = values["force-cancel"];
        break;
    }
    writeWire(
      process.stdout,
      await sessionControl(file, op, object(json(args)), new AbortController().signal),
    );
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
  check(action === "proxy", "unknown ACP action");
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
