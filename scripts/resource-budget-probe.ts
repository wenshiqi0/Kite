/** Kernel verification on a unique transient slice and disposable native child. */
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { prepareMemoryLimit, proc, reclaimableMemory, scopeName } from "@kite/platform";
import { check, object, parse } from "@kite/protocol";
import { run, sleep } from "@kite/runtime";

const cacheRoot = join(homedir(), ".cache");
mkdirSync(cacheRoot, { recursive: true });
const unit = `kitequotaqa${crypto.randomUUID().replaceAll("-", "").slice(0, 12)}.slice`,
  limit = 256 * 1024 * 1024,
  residentLimit = 64 * 1024 * 1024,
  root = mkdtempSync(join(cacheRoot, "kite-memory-probe-"));
const child = Bun.spawn(
  [
    "/usr/bin/python3",
    "-u",
    "-c",
    `import os,sys
print('ready',flush=True)
resident=[]
for command in sys.stdin:
 if command.strip()=='cache':
  with open(sys.argv[1],'wb') as f:
   block=b'x'*(1024*1024)
   for _ in range(160): f.write(block)
   f.flush();os.fsync(f.fileno())
   del block
  print('cached',flush=True)
 else:
  size=(32 if command.strip()=='resident' else 256)*1024*1024
  data=bytearray(size)
  for i in range(0,size,4096): data[i]=1
  resident.append(data)
  print('resident',flush=True)
`,
    join(root, "cache"),
  ],
  { stdin: "pipe", stdout: "pipe", stderr: "inherit", detached: true },
);
let scope = "";
const call = (...args: string[]) => run(["systemctl", "--user", ...args]);
try {
  const reader = child.stdout.getReader();
  let buffered = "";
  const line = async (stage: string) => {
    while (!buffered.includes("\n")) {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const value = await Promise.race([
        reader.read(),
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => reject(new Error("owned allocation peer timed out")), 10_000);
        }),
      ]).finally(() => clearTimeout(timer));
      check(!value.done, `owned allocation peer exited during ${stage}`);
      buffered += Buffer.from(value.value).toString();
    }
    const end = buffered.indexOf("\n"),
      result = buffered.slice(0, end);
    buffered = buffered.slice(end + 1);
    return result;
  };
  check((await line("startup")) === "ready", "owned allocation peer unavailable");
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
  const memory = () => {
    const stats = Object.fromEntries(
      readFileSync(`${parent}/memory.stat`, "utf8")
        .trim()
        .split("\n")
        .map((s) => {
          const [key, value] = s.split(" ");
          return [key, Number(value)];
        }),
    );
    const current = Number(readFileSync(`${parent}/memory.current`, "utf8")),
      cache = reclaimableMemory(current, stats);
    return { current, cache, resident: current - cache };
  };
  child.stdin.write("cache\n");
  check((await line("cache")) === "cached", "owned cache fixture failed");
  const cached = memory();
  check(
    cached.current > residentLimit && cached.resident < residentLimit / 2,
    "file cache was charged as resident memory",
  );
  await prepareMemoryLimit(parent, residentLimit);
  await call(
    "set-property",
    "--runtime",
    unit,
    `MemoryMax=${residentLimit}`,
    `MemoryHigh=${residentLimit}`,
  );
  check(
    readFileSync(`${parent}/memory.max`, "utf8").trim() === String(residentLimit),
    "safe cache-backed shrink did not reach the kernel",
  );
  child.stdin.write("resident\n");
  check(
    (await line("resident")) === "resident",
    "cache prevented a resident allocation within budget",
  );
  const allocated = memory();
  check(
    allocated.resident >= 32 * 1024 * 1024 && allocated.resident <= residentLimit,
    "resident allocation was not charged to the real budget",
  );
  child.stdin.write("oom\n");
  child.stdin.end();
  const timeout = setTimeout(() => child.kill("SIGKILL"), 8000);
  const code = await child.exited;
  clearTimeout(timeout);
  reader.releaseLock();
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
            limit_bytes: residentLimit,
            cached_bytes_before_shrink: cached.cache,
            resident_bytes_before_shrink: cached.resident,
            resident_bytes_after_allocation: allocated.resident,
            cache_reclaimed_before_shrink: true,
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
  rmSync(root, { recursive: true });
}
