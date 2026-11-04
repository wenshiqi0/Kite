import { createHmac, timingSafeEqual } from "node:crypto";
import { check, parse, type RpcMessage } from "@kite/protocol";
import { ACP_MESSAGE_LIMIT, ACP_QUEUE_LIMIT, acpMessage } from "@kite/protocol/acp";
import type { ServerWebSocket } from "bun";

export interface AcpConnection {
  receive(message: RpcMessage): Promise<void>;
  close(): void;
}
export interface AcpServerOptions {
  token: string;
  port: number;
  hostname?: string;
  connect(send: (message: RpcMessage) => void): AcpConnection;
}
interface ConnectionState {
  connection?: AcpConnection;
  pending: number;
}
function authenticated(header: string | null, token: string): boolean {
  if (!header) return false;
  const actual = Buffer.from(header),
    expected = Buffer.from(`Bearer ${token}`);
  return actual.length === expected.length && timingSafeEqual(actual, expected);
}
function peerProof(token: string, challenge: string): string {
  return createHmac("sha256", token).update(`kite-acp-peer-v1\0${challenge}`).digest("hex");
}
export function serveAcp(options: AcpServerOptions) {
  const hostname = options.hostname ?? "127.0.0.1";
  check(
    ["127.0.0.1", "::1"].includes(hostname),
    "ACP listens on loopback; use an authenticated SSH forward for remote access",
  );
  check(
    options.token.length >= 32 && !/[\r\n]/.test(options.token),
    "invalid ACP authentication token",
  );
  let connections = 0;
  const server = Bun.serve<ConnectionState>({
    hostname,
    port: options.port,
    maxRequestBodySize: ACP_MESSAGE_LIMIT,
    fetch(request, server) {
      const url = new URL(request.url);
      if (url.pathname !== "/acp" || url.search) return new Response(null, { status: 404 });
      if (request.headers.has("origin")) return new Response(null, { status: 403 });
      const challenge = request.headers.get("x-kite-acp-challenge");
      if (
        request.method === "GET" &&
        challenge &&
        /^[a-f0-9]{32}$/.test(challenge) &&
        !request.headers.has("upgrade")
      )
        return new Response(null, {
          status: 204,
          headers: {
            "x-kite-acp-proof": peerProof(options.token, challenge),
            "cache-control": "no-store",
          },
        });
      if (!authenticated(request.headers.get("authorization"), options.token))
        return new Response(null, { status: 403 });
      if (request.method !== "GET") return new Response(null, { status: 405 });
      if (connections >= 16) return new Response(null, { status: 503 });
      if (server.upgrade(request, { data: { pending: 0 } })) {
        connections++;
        return;
      }
      return new Response(null, { status: 426 });
    },
    websocket: {
      maxPayloadLength: ACP_MESSAGE_LIMIT,
      backpressureLimit: ACP_QUEUE_LIMIT,
      closeOnBackpressureLimit: true,
      idleTimeout: 120,
      sendPings: true,
      open(socket) {
        try {
          socket.data.connection = options.connect((message) => {
            const text = JSON.stringify({ ...message, jsonrpc: "2.0" });
            if (
              Buffer.byteLength(text) > ACP_MESSAGE_LIMIT ||
              socket.getBufferedAmount() + Buffer.byteLength(text) > ACP_QUEUE_LIMIT
            ) {
              socket.close(1009, "ACP output limit exceeded");
              return;
            }
            socket.send(text);
          });
        } catch {
          socket.close(1011, "ACP workspace unavailable");
        }
      },
      message(socket, data) {
        if (typeof data !== "string" || socket.data.pending >= 32) {
          socket.close(1008, "ACP input limit exceeded");
          return;
        }
        let message: RpcMessage;
        try {
          message = acpMessage(parse(data));
        } catch {
          socket.close(1008, "Invalid ACP JSON-RPC message");
          return;
        }
        socket.data.pending++;
        void socket.data.connection
          ?.receive(message)
          .catch(() => {
            socket.close(1011, "ACP connection failed");
          })
          .finally(() => {
            socket.data.pending--;
          });
      },
      close(socket) {
        connections--;
        socket.data.connection?.close();
      },
    },
  });
  return server;
}

export interface AcpClient {
  send(message: RpcMessage): void;
  close(): void;
  closed: Promise<void>;
}
/** Establish once. Neither transport errors nor timeouts replay submitted work. */
export async function connectAcp(
  url: string,
  token: string,
  receive: (message: RpcMessage) => void,
): Promise<AcpClient> {
  const endpoint = new URL(url);
  check(
    ["ws:", "wss:"].includes(endpoint.protocol) &&
      !endpoint.username &&
      !endpoint.password &&
      endpoint.pathname === "/acp" &&
      !endpoint.search &&
      !endpoint.hash,
    "invalid ACP endpoint",
  );
  check(
    endpoint.protocol === "wss:" || ["127.0.0.1", "localhost", "[::1]"].includes(endpoint.hostname),
    "unencrypted ACP requires a loopback SSH forward",
  );
  check(token.length >= 32 && !/[\r\n]/.test(token), "invalid ACP token");
  // Verify knowledge of the endpoint secret before releasing a Bearer token to
  // a selected local forward. This also closes the free-port/bind race.
  const challenge = crypto.randomUUID().replaceAll("-", ""),
    probe = new URL(endpoint);
  probe.protocol = endpoint.protocol === "wss:" ? "https:" : "http:";
  const response = await fetch(probe, {
      headers: { "x-kite-acp-challenge": challenge },
      redirect: "error",
      signal: AbortSignal.timeout(8000),
    }),
    proof = response.headers.get("x-kite-acp-proof") || "";
  check(
    response.status === 204 &&
      /^[a-f0-9]{64}$/.test(proof) &&
      timingSafeEqual(Buffer.from(proof, "hex"), Buffer.from(peerProof(token, challenge), "hex")),
    "ACP handshake failed: endpoint identity mismatch",
  );
  const socket = new WebSocket(url, { headers: { Authorization: `Bearer ${token}` } });
  const closed = new Promise<void>((resolve) => {
    socket.addEventListener("close", () => resolve(), { once: true });
  });
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      socket.close();
      reject(new Error("ACP handshake timed out"));
    }, 8000);
    socket.addEventListener(
      "open",
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
    socket.addEventListener(
      "error",
      () => {
        clearTimeout(timer);
        reject(new Error("ACP handshake failed"));
      },
      { once: true },
    );
  });
  socket.addEventListener("message", (event) => {
    try {
      check(
        typeof event.data === "string" && Buffer.byteLength(event.data) <= ACP_MESSAGE_LIMIT,
        "ACP frame exceeds limit",
      );
      receive(acpMessage(parse(event.data)));
    } catch {
      socket.close(1008, "Invalid ACP message");
    }
  });
  return {
    closed,
    send(message) {
      const text = JSON.stringify({ ...message, jsonrpc: "2.0" });
      check(
        socket.readyState === WebSocket.OPEN &&
          Buffer.byteLength(text) <= ACP_MESSAGE_LIMIT &&
          socket.bufferedAmount + Buffer.byteLength(text) <= ACP_QUEUE_LIMIT,
        "ACP connection is closed or backpressured",
      );
      socket.send(text);
    },
    close() {
      socket.close();
    },
  };
}

// Keep the public socket mechanism typed independently of provider ownership.
export type AcpWebSocket = ServerWebSocket<ConnectionState>;
