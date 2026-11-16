/** Disposable, read-only app-server peer for archived-owner lifecycle regressions. */
import { chmodSync, readFileSync } from "node:fs";
import { array, check, object, parse, string } from "@kite/protocol";

const socket = string(process.argv[2]),
  state = string(process.argv[3]);
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
        ["initialize", "thread/loaded/list", "thread/read", "thread/list"].includes(
          String(request.method),
        ),
        "unexpected mutation",
      );
      const current = object(parse(readFileSync(state, "utf8"))),
        threads = array(current.threads);
      ws.send(
        JSON.stringify({
          id: request.id,
          result:
            request.method === "initialize"
              ? {}
              : request.method === "thread/loaded/list"
                ? { data: threads }
                : request.method === "thread/read"
                  ? {
                      thread: {
                        id: object(request.params).threadId,
                        status: {
                          type:
                            object(current.background ?? {})[
                              string(object(request.params).threadId)
                            ] ?? current.status,
                        },
                      },
                    }
                  : { data: [], nextCursor: null },
        }),
      );
    },
  },
});
chmodSync(socket, 0o600);
console.log("ready");
