/** Owned X11/D-Bus acceptance for the native Linux panel. Never uses a real node. */
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { check } from "@kite/protocol";
import { bounded, run, sleep } from "@kite/runtime";

check(process.env.DISPLAY, "run under xvfb-run and dbus-run-session");
const root = mkdtempSync(join(tmpdir(), "kite-native-probe-")),
  binary = resolve("target/debug/kite-desktop"),
  node = join(root, "node.ts"),
  calls = join(root, "calls.jsonl"),
  shot = join(root, "panel.png");
writeFileSync(
  node,
  `#!${process.execPath}\nimport {appendFileSync} from 'node:fs';appendFileSync(${JSON.stringify(calls)},JSON.stringify(process.argv.slice(2))+'\\n');console.log(JSON.stringify(process.argv[2]==='sessions'?{sessions:[{id:'owned',title:'Owned fixture session'}]}:{health:'ready',machine:'owned-native-fixture',version:'fixture',services:[{name:'Owned service',active:true}]}));\n`,
);
chmodSync(node, 0o700);
const env = {
  ...process.env,
  HOME: root,
  XDG_RUNTIME_DIR: join(root, "runtime"),
  GIO_USE_VFS: "local",
  GTK_USE_PORTAL: "0",
  NO_AT_BRIDGE: "1",
  KITE_NODE_BINARY: node,
  KITE_DESKTOP_ASSETS: resolve("desktop/linux"),
  KITE_DESKTOP_LAUNCHER: binary,
};
const { mkdirSync } = await import("node:fs");
mkdirSync(join(root, "runtime"), { mode: 0o700 });
const app = Bun.spawn([binary, "--snapshot", shot, "--close-after", "12"], {
  env,
  stdin: "ignore",
  stdout: "pipe",
  stderr: "pipe",
});
try {
  for (let i = 0; i < 200 && !existsSync(shot); i++) {
    check(app.exitCode === null, "native panel exited before screenshot");
    await sleep(25);
  }
  check(existsSync(shot), "native panel screenshot missing");
  check(
    readFileSync(shot)
      .subarray(0, 8)
      .equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])),
    "invalid native screenshot",
  );
  const duplicate = await run([binary, "--background"], { env, timeout: 3000 });
  check(duplicate.code === 0, "native single-instance invocation failed");
  for (const action of ["refresh", "show", "upgrade"]) {
    await run(
      [
        "gdbus",
        "call",
        "--session",
        "--dest",
        "io.github.wenshiqi0.Kite.Host",
        "--object-path",
        "/io/github/wenshiqi0/Kite/Host",
        "--method",
        "org.gtk.Actions.Activate",
        action,
        "[]",
        "{}",
      ],
      { env },
    );
  }
  await sleep(200);
  check(readFileSync(calls, "utf8").includes("status"), "native panel did not request host status");
  check(!readFileSync(calls, "utf8").includes("stop"), "native panel tried to stop services");
  const output = resolve("target/qa/native-panel.png");
  await Bun.write(output, readFileSync(shot));
  await run([binary, "--quit"], { env, timeout: 3000 });
  check((await app.exited) === 0, "native panel did not exit cleanly");
  console.log(
    JSON.stringify({ native: true, singleInstance: true, actions: true, screenshot: output }),
  );
} catch (error) {
  app.kill();
  const stderr = await bounded(app.stderr, 65536);
  throw new Error(`${error instanceof Error ? error.message : String(error)}\n${stderr}`);
} finally {
  if (app.exitCode === null) app.kill();
  rmSync(root, { recursive: true, force: true });
}
