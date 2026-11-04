/** Disposable owner for inherited SessionStart identity regression checks. */
import { appendFileSync, chmodSync } from "node:fs";
import { check, object, parse, string } from "@kite/protocol";

const socket = string(process.argv[2]),
  thread = string(process.argv[3]),
  requests = string(process.argv[4]),
  child = crypto.randomUUID();
Bun.serve({
  unix: socket,
  fetch(request, server) {
    if (server.upgrade(request)) return;
    return new Response("upgrade required", { status: 400 });
  },
  websocket: {
    message(ws, body) {
      const request = object(parse(String(body)));
      if (request.id === undefined) return;
      check(
        ["initialize", "thread/loaded/list", "thread/list"].includes(String(request.method)),
        "unexpected mutation",
      );
      appendFileSync(requests, `${request.method}\n`);
      ws.send(
        JSON.stringify({
          id: request.id,
          result:
            request.method === "initialize"
              ? {}
              : request.method === "thread/loaded/list"
                ? { data: [thread] }
                : {
                    data: object(request.params).archived
                      ? []
                      : [
                          {
                            id: child,
                            parentThreadId: thread,
                            agentNickname: "Worker",
                            status: { type: "active", activeFlags: [] },
                            updatedAt: 1793800000,
                          },
                        ],
                    nextCursor: null,
                  },
        }),
      );
    },
  },
});
chmodSync(socket, 0o600);
console.log("ready");
