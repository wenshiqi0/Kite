/** Kernel verification on a unique transient slice and disposable native child. */
import { readFileSync } from "node:fs";
import { proc, scopeName } from "@kite/platform";
import { check, object, parse } from "@kite/protocol";
import { bounded, run, sleep } from "@kite/runtime";

const unit = `kitequotaqa${crypto.randomUUID().replaceAll("-", "").slice(0, 12)}.slice`,
  limit = 128 * 1024 * 1024;
const child = Bun.spawn(
  [
    process.execPath,
    "-e",
    "console.log('ready');for await(const _ of Bun.stdin.stream()){const bytes=new Uint8Array(256*1024*1024);for(let i=0;i<bytes.length;i+=4096)bytes[i]=1;console.log(bytes.length);await Bun.sleep(1000);}",
  ],
  { stdin: "pipe", stdout: "pipe", stderr: "ignore", detached: true },
);
let scope = "";
const call = (...args: string[]) => run(["systemctl", "--user", ...args]);
try {
  const reader = child.stdout.getReader();
  const ready = await reader.read();
  check(
    !ready.done && Buffer.from(ready.value).toString().includes("ready"),
    "owned allocation peer unavailable",
  );
  reader.releaseLock();
  const fact = proc(child.pid);
  scope = scopeName("qa:p1", child.pid, fact.started);
  await call(
    "set-property",
    "--runtime",
    unit,
    "MemoryAccounting=yes",
    `MemoryMax=${limit}`,
    `MemoryHigh=${limit}`,
    "MemorySwapMax=0",
  );
  await call("start", unit);
  await run([
    "busctl",
    "--user",
    "--timeout=5",
    "call",
    "org.freedesktop.systemd1",
    "/org/freedesktop/systemd1",
    "org.freedesktop.systemd1.Manager",
    "StartTransientUnit",
    "ssa(sv)a(sa(sv))",
    scope,
    "fail",
    "4",
    "Description",
    "s",
    "Kite owned quota QA",
    "Slice",
    "s",
    unit,
    "CollectMode",
    "s",
    "inactive-or-failed",
    "PIDs",
    "au",
    "1",
    String(child.pid),
    "0",
  ]);
  for (let i = 0; i < 100 && !proc(child.pid).cgroup.endsWith(`/${scope}`); i++) await sleep(20);
  const cgroup = proc(child.pid).cgroup;
  check(
    cgroup.includes(`/${unit}/`) && cgroup.endsWith(`/${scope}`),
    "owned process adoption failed",
  );
  const parent = `/sys/fs/cgroup${cgroup.slice(0, cgroup.lastIndexOf("/"))}`;
  check(
    readFileSync(`${parent}/memory.max`, "utf8").trim() === String(limit),
    "kernel memory limit differs",
  );
  child.stdin.write("allocate\n");
  child.stdin.end();
  const timeout = setTimeout(() => child.kill("SIGKILL"), 8000);
  const code = await child.exited;
  clearTimeout(timeout);
  await bounded(child.stdout, 65536);
  const events = Object.fromEntries(
    readFileSync(`${parent}/memory.events`, "utf8")
      .trim()
      .split("\n")
      .map((s) => s.split(" ")),
  );
  check(
    code !== 0 && Number(events.oom_kill) > 0,
    "disposable allocation was not bounded by the kernel",
  );
  console.log(
    JSON.stringify(
      object(
        parse(
          JSON.stringify({
            kernel_enforced: true,
            oom_kills: Number(events.oom_kill),
            limit_bytes: limit,
          }),
        ),
      ),
    ),
  );
} finally {
  if (child.exitCode === null) child.kill("SIGKILL");
  if (scope) await call("stop", scope).catch(() => {});
  await call("stop", unit).catch(() => {});
  await call("reset-failed", unit).catch(() => {});
  await call("revert", unit).catch(() => {});
}
