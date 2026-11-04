import { existsSync } from "node:fs";
import { join } from "node:path";
import { type ParseArgsConfig, parseArgs } from "node:util";
import { gzipSync } from "node:zlib";
import { configure, execBudgeted, reapOrphans, sample } from "@kite/platform";
import { capturedDisplay } from "@kite/platform/captured-display";
import { CLIPBOARD_LIMIT, clipboard } from "@kite/platform/clipboard";
import { desktop } from "@kite/platform/desktop";
import { deliver } from "@kite/platform/push";
import {
  check,
  integer,
  type Json,
  json,
  object,
  parse,
  RPC_LIMIT,
  string,
  UUID,
} from "@kite/protocol";
import { catalogRead, catalogWrite } from "@kite/providers/catalog";
import {
  api,
  bind,
  ensure,
  herdrRequest,
  installResourceHooks,
  noteSessionOpen,
  proxy,
  readRegistry,
  sessionOpen,
  upgrade,
} from "@kite/providers/codex";
import { sessionDelete } from "@kite/providers/deletion";
import { harnessAgents, harnessDirectory, providers } from "@kite/providers/harness";
import { historyProjection, historyStamp } from "@kite/providers/history";
import { lifecycleFacts, lifecycleReap, notificationFacts } from "@kite/providers/lifecycle";
import { serveOwner } from "@kite/providers/owner";
import { attachmentRead, readGeneratedImage, upload } from "@kite/providers/uploads";
import { exec, input, lines } from "@kite/runtime";
export async function main(argv: string[]): Promise<void> {
  process.umask(0o077);
  installResourceHooks();
  const action = argv[0];
  check(action, "host action required");
  const output = (v: Json) => {
    process.stdout.write(`${JSON.stringify(v)}\n`);
  };
  if (action === "captured-display") {
    check(argv.length === 2, "captured-display requires a port");
    output(await capturedDisplay(Number(argv[1])));
    return;
  }
  if (action === "harness-owner") {
    await serveOwner(string(argv[1]));
    return;
  }
  if (action === "desktop") {
    output(await desktop(object(parse((await input(32768)).toString()))));
    return;
  }
  if (action === "clipboard") {
    const mode = string(argv[1]),
      port = Number(argv[2]);
    output(
      await clipboard(
        mode,
        port,
        mode === "write" ? await input(CLIPBOARD_LIMIT) : Buffer.alloc(0),
      ),
    );
    return;
  }
  if (action === "push") {
    try {
      output(
        await deliver(
          string(argv[1]),
          string(argv[2]),
          object(parse((await input(65536)).toString())),
        ),
      );
    } catch {
      output({
        results: [],
        problem: "push transport unavailable; check host credentials, project and network",
      });
      process.exitCode = 1;
    }
    return;
  }
  const resource = action === "resources",
    args = resource ? argv.slice(2) : argv.slice(1),
    resourceAction = argv[1];
  const { values, positionals } = parseArgs({
    args,
    strict: true,
    allowPositionals: true,
    options: {
      pane: { type: "string" },
      herdr: { type: "string" },
      thread: { type: "string" },
      file: { type: "string" },
      before: { type: "string" },
      after: { type: "string" },
      id: { type: "string" },
      name: { type: "string" },
      size: { type: "string" },
      sha256: { type: "string" },
      compressed: { type: "boolean" },
    } satisfies ParseArgsConfig["options"],
  });
  const arg = (key: string) => {
    const v = (values as Record<string, string | boolean | undefined>)[key];
    check(typeof v === "string", `missing ${key}`);
    return v;
  };
  if (resource) {
    if (resourceAction === "sample") {
      output(await sample());
      return;
    }
    if (resourceAction === "orphans") {
      await reapOrphans();
      output({});
      return;
    }
    if (resourceAction === "configure") {
      const p = object(parse((await input(65536)).toString()));
      output(await configure(integer(p.limit_bytes), integer(p.high_bytes), arg("herdr")));
      return;
    }
    check(
      resourceAction === "exec-agent" || resourceAction === "exec-harness",
      "invalid resource action",
    );
    let shell: number | undefined;
    if (resourceAction === "exec-agent") {
      const p = object(
        (await api(arg("herdr"), "pane.process_info", { pane_id: arg("pane") })).process_info,
      );
      check(p.foreground_process_group_id === p.shell_pid, "agent pane is not an available shell");
      shell = integer(p.shell_pid);
    }
    await execBudgeted(arg("pane"), positionals, shell);
    return;
  }
  check(!positionals.length, "unexpected arguments");
  switch (action) {
    case "herdr-api":
      for await (const line of lines(Bun.stdin.stream(), RPC_LIMIT)) {
        output(await herdrRequest(arg("herdr"), object(parse(line))));
        break; // Herdr's bridge handles one request per connection.
      }
      break;
    case "catalog-read":
      output(json(await catalogRead()));
      break;
    case "catalog-write":
      output(json(await catalogWrite(parse((await input(1024 * 1024)).toString()))));
      break;
    case "harness-scan":
      output({
        providers: await providers(),
        agents: await harnessAgents((await catalogRead()).sessions),
      });
      break;
    case "harness-history": {
      const file = join(harnessDirectory(arg("thread")), "history.jsonl");
      if (!existsSync(file)) {
        process.stdout.write("kite-history-pending-v1\n");
        break;
      }
      const stamp = historyStamp(file);
      process.stdout.write(`kite-history-v1\n${stamp}\n`);
      if (stamp !== values.name)
        process.stdout.write(
          gzipSync(
            await historyProjection(
              file,
              typeof values.before === "string" ? values.before : undefined,
              typeof values.after === "string" ? values.after : undefined,
            ),
            { level: 1 },
          ),
        );
      break;
    }
    case "history": {
      const body = await historyProjection(
        arg("file"),
        typeof values.before === "string" ? values.before : undefined,
        typeof values.after === "string" ? values.after : undefined,
      );
      process.stdout.write(values.compressed ? gzipSync(body, { level: 1 }) : body);
      break;
    }
    case "proxy":
      await proxy(arg("pane"));
      break;
    case "ensure":
      output(json(await ensure(arg("pane"), arg("herdr"))));
      break;
    case "bind":
      output(await bind(arg("pane"), arg("herdr")));
      break;
    case "identity":
      output(json(readRegistry(arg("pane"))));
      break;
    case "upload":
      output(
        await upload(
          arg("id"),
          arg("name"),
          Number(arg("size")),
          arg("sha256"),
          Bun.stdin.stream(),
        ),
      );
      break;
    case "attachment-read":
      process.stdout.write(attachmentRead(arg("id"), arg("name")));
      break;
    case "image-read":
      process.stdout.write(readGeneratedImage(arg("id")));
      break;
    case "session-open":
      output(await sessionOpen(arg("id"), arg("herdr")));
      break;
    case "session-delete":
      output(json(await sessionDelete(arg("id"), arg("herdr"))));
      break;
    case "lifecycle-facts":
      output(await lifecycleFacts(arg("herdr")));
      break;
    case "lifecycle-reap":
      output(await lifecycleReap(object(parse((await input(65536)).toString())), arg("herdr")));
      break;
    case "notification-facts":
    case "session-facts":
      output(await notificationFacts(arg("herdr"), action === "session-facts"));
      break;
    case "upgrade":
      output(await upgrade(arg("pane"), arg("herdr")));
      break;
    case "tui": {
      const info = await ensure(arg("pane"), arg("herdr"));
      await noteSessionOpen(arg("pane"));
      check(UUID.test(arg("thread")), "invalid thread ID");
      exec([
        "codex",
        "--remote",
        `unix://${info.socket}`,
        "-c",
        "check_for_update_on_startup=false",
        "resume",
        arg("thread"),
      ]);
      break;
    }
    default:
      throw new Error("unsupported host action");
  }
}
if (import.meta.main)
  try {
    await main(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : "host adapter failed"}\n`);
    process.exitCode = 1;
  }
