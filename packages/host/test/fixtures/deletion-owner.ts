/** Disposable app-server protocol peer and stubborn local tool for deletion acceptance. */
import { chmodSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { array, check, object, parse, string } from "@kite/protocol";

const socket = string(process.argv[2]),
  threads = array(parse(string(process.argv[3])));
const tool = Bun.spawn(
  [
    process.execPath,
    "-e",
    'process.on("SIGTERM",()=>{});console.log("ready");setInterval(()=>{},1000)',
  ],
  { stdin: "ignore", stdout: "pipe", stderr: "ignore" },
);
const reader = tool.stdout.getReader();
check(!(await reader.read()).done, "tool did not start");
reader.releaseLock();
writeFileSync(join(string(process.argv[4]), "tool.json"), JSON.stringify({ pid: tool.pid }));
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
        ["initialize", "thread/loaded/list"].includes(String(request.method)),
        "unexpected mutation",
      );
      ws.send(
        JSON.stringify({
          id: request.id,
          result: request.method === "initialize" ? {} : { data: threads },
        }),
      );
    },
  },
});
chmodSync(socket, 0o600);
console.log("ready");
