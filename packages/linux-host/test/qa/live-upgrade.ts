/** Real systemd handover and installer acceptance on owned units, sockets and state. */
import {
  appendFileSync,
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { array, check, type ObjectValue, object, parse } from "@kite/protocol";
import { atomic, sleep } from "@kite/runtime";
import { type Command, command } from "../../src/command";
import { install } from "../../src/install";
import { Bridge } from "./bridge";

const { values } = parseArgs({
  options: { binary: { type: "string" }, evidence: { type: "string" } },
  strict: true,
});
check(
  values.binary && process.env.XDG_RUNTIME_DIR,
  "--binary and a real user-systemd manager are required",
);
const systemctl = async (...args: string[]) =>
  (await command(["systemctl", "--user", ...args], { timeout: 15000 })).stdout.trim();
await systemctl("show-environment");
const unit = `kite-qa-upgrade-${crypto.randomUUID().replaceAll("-", "")}`,
  unitRoot = join(process.env.XDG_RUNTIME_DIR, "systemd/user"),
  servicePath = join(unitRoot, `${unit}.service`),
  socketPath = join(unitRoot, `${unit}.socket`),
  root = mkdtempSync(join(tmpdir(), "kite-live-upgrade-")),
  binary = join(root, "test-node"),
  nodeDir = join(root, "node"),
  address = join(nodeDir, "host.sock"),
  herdr = join(root, "herdr"),
  manager = join(root, "manager"),
  interpreter = join(root, "interpreter"),
  facts = join(root, "facts.json"),
  home = join(root, "user"),
  packageRoot = join(root, "package");
mkdirSync(unitRoot, { recursive: true });
mkdirSync(nodeDir, { mode: 0o700 });
mkdirSync(manager);
mkdirSync(packageRoot);
copyFileSync(resolve(values.binary), binary);
chmodSync(binary, 0o700);
function executable(path: string, source: string): void {
  writeFileSync(path, `#!${process.execPath}\n${source}\n`);
  chmodSync(path, 0o700);
}
executable(
  join(root, "owner"),
  `import {writeFileSync,renameSync} from 'node:fs';import {join} from 'node:path';let n=0;for(;;){const p=join(import.meta.dir,'heartbeat');writeFileSync(p+'.pending',String(++n));renameSync(p+'.pending',p);await Bun.sleep(20);}`,
);
executable(
  herdr,
  `import {existsSync,writeFileSync,appendFileSync} from 'node:fs';import {join} from 'node:path';const root=import.meta.dir;if(process.argv[2]!=='remote-api-bridge')process.exit(2);if(!existsSync(join(root,'owner.pid'))){const child=Bun.spawn([${JSON.stringify(process.execPath)},join(root,'owner')],{stdin:'ignore',stdout:'ignore',stderr:'ignore',detached:true});child.unref();writeFileSync(join(root,'owner.pid'),String(child.pid));}for await(const line of console){if(!line)continue;if(line.startsWith('MUTATE')){appendFileSync(join(root,'mutations'),line+'\\n');while(!existsSync(join(root,'release')))await Bun.sleep(10);process.stdout.write('APPLIED\\n');}else process.stdout.write('PONG '+line.slice(5)+'\\n');}`,
);
symlinkSync("/usr/bin/systemctl", join(manager, "systemctl"));
executable(
  join(manager, "systemd-notify"),
  `import {existsSync,unlinkSync} from 'node:fs';import {join} from 'node:path';const flag=join(import.meta.dir,'../fail-notify'),fail=existsSync(flag);if(fail)unlinkSync(flag);const result=Bun.spawnSync(['/usr/bin/systemd-notify',...process.argv.slice(2)],{stdin:'inherit',stdout:'inherit',stderr:'inherit'});process.exit(fail?1:result.exitCode);`,
);
const fact = {
  session_id: "owned-session",
  title: "Owned session",
  source: "owned-source",
  status: "working",
  revision: 1,
  thread: "owned-thread",
  controls: "",
  controls_known: true,
  lifecycle: false,
};
atomic(facts, { facts: [fact] });
executable(
  interpreter,
  `import {readFileSync,existsSync,writeFileSync,appendFileSync} from 'node:fs';import {join} from 'node:path';const root=import.meta.dir;if(process.argv[3]!=='notification-facts')process.exit(2);const facts=JSON.parse(readFileSync(join(root,'facts.json'),'utf8'));const log=phase=>appendFileSync(join(root,'scans'),JSON.stringify({pid:process.ppid,phase})+'\\n');log('start');if(existsSync(join(root,'hold-scan'))){writeFileSync(join(root,'scan-inflight'),'');await Bun.sleep(2000);}log('end');console.log(JSON.stringify(facts));`,
);
const config = join(root, "node.json");
atomic(config, {
  node_id: "owned-upgrade-probe",
  machine: "owned-fixture",
  socket: address,
  backend: join(root, "adapters/host.js"),
  herdr,
  bun: interpreter,
  systemctl: join(manager, "systemctl"),
  services: [],
  environment: {},
});
copyFileSync(binary, join(packageRoot, "kite-node"));
chmodSync(join(packageRoot, "kite-node"), 0o700);
writeFileSync(join(packageRoot, "kite-desktop"), "controlled native GUI");
copyFileSync(resolve("desktop/linux/strings.json"), join(packageRoot, "strings.json"));
cpSync(resolve("desktop/linux/icons"), join(packageRoot, "icons"), { recursive: true });
cpSync(resolve("skills"), join(packageRoot, "skills"), { recursive: true });
mkdirSync(join(home, ".config/kite"), { recursive: true });
copyFileSync(config, join(home, ".config/kite/node.json"));
const managerCalls: string[][] = [];
let upgrading = false;
const installRun: Command = async (argv, options = {}) => {
  if (argv.includes("upgrade")) upgrading = true;
  if (argv[0] === "systemctl") {
    if (argv.includes("daemon-reload"))
      for (const kind of ["service", "socket"])
        writeFileSync(
          join(unitRoot, `${unit}.${kind}`),
          readFileSync(join(home, `.config/systemd/user/kite-node.${kind}`), "utf8").replaceAll(
            "kite-node.socket",
            `${unit}.socket`,
          ),
        );
    argv = argv.map((arg) =>
      arg
        .replaceAll("kite-node.service", `${unit}.service`)
        .replaceAll("kite-node.socket", `${unit}.socket`),
    );
    if (argv.includes("enable"))
      argv = argv.filter((arg) => arg !== "--now").map((arg) => (arg === "enable" ? "start" : arg));
    managerCalls.push(argv);
  }
  return command(argv, options);
};
const installOwned = () =>
  install(
    { packageRoot, home, enable: true, skillDirs: [join(home, ".agents/skills")] },
    { run: installRun, refreshGui: async () => {} },
  );
async function waitFor(
  predicate: () => boolean | Promise<boolean>,
  label: string,
  timeout = 5000,
): Promise<void> {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await sleep(20);
  }
  throw new Error(`owned upgrade probe timed out: ${label}`);
}
const status = async () =>
  object(parse((await command([binary, "--socket", address, "status"], { timeout: 6000 })).stdout));
const register = async (token: string) => {
  const response = await command([binary, "--socket", address, "push-register"], {
    input: `${JSON.stringify({
      device_id: "owned-device",
      host_id: "owned-host",
      token,
      project_id: "owned-project",
    })}\n`,
    timeout: 15000,
  });
  check(object(parse(response.stdout)).registered, "owned registration failed");
};
let stop = false,
  pump: Promise<void> | undefined,
  ownerPid: number | undefined;
const streams: Bridge[] = [],
  errors: string[] = [],
  samples: number[] = [];
try {
  await installOwned();
  const oldPid = Number((await status()).pid),
    installedSkill = join(home, ".agents/skills/kite/SKILL.md"),
    skill = readFileSync(installedSkill);
  writeFileSync(installedSkill, Buffer.concat([skill, Buffer.from("\nOwned custom skill edit\n")]));
  try {
    await installOwned();
    throw new Error("edited skill was accepted");
  } catch (error) {
    check(
      error instanceof Error && error.message.includes("local changes"),
      "edited skill was overwritten",
    );
  }
  check(
    readFileSync(installedSkill, "utf8").endsWith("Owned custom skill edit\n") &&
      Number((await status()).pid) === oldPid,
    "edited skill or running node changed",
  );
  writeFileSync(installedSkill, skill);
  await waitFor(
    async () => Boolean((await status()).notificationLastScanMs),
    "notification bootstrap",
  );
  await register("owned-token-before");
  const live = await Bridge.connect(address, herdr);
  streams.push(live);
  pump = (async () => {
    try {
      let sequence = 0;
      while (!stop) {
        const sent = performance.now();
        live.send(`PING ${sequence}\n`);
        check((await live.line()) === `PONG ${sequence}`, "continuous bridge reply changed");
        samples.push(performance.now() - sent);
        sequence++;
        await sleep(10);
      }
    } catch (error) {
      errors.push(String(error));
    }
  })();
  await waitFor(() => samples.length >= 20 || errors.length > 0, "continuous frames");
  check(!errors.length, errors.join("; "));
  ownerPid = Number(readFileSync(join(root, "owner.pid"), "utf8"));
  const heartbeat = Number(readFileSync(join(root, "heartbeat"), "utf8"));
  const once = await Bridge.connect(address, herdr);
  streams.push(once);
  once.send("MUTATE once\n");
  await waitFor(() => existsSync(join(root, "mutations")), "in-flight mutation");
  for (const [name, metadata] of [
    ["incompatible", { api: 999 }],
    ["broken", { api: 1, backendProtocol: 5, liveUpgrade: 1, generationLaunch: 1 }],
  ] as const) {
    const bad = join(root, name);
    executable(
      bad,
      `if(process.argv[2]==='version')console.log(${JSON.stringify(JSON.stringify(metadata))});else process.exit(3);`,
    );
    const failed = await command([binary, "--socket", address, "upgrade", "--binary", bad], {
      allowFailure: true,
      timeout: 15000,
    });
    check(
      failed.code !== 0 && Number((await status()).pid) === oldPid,
      "bad candidate replaced the serving node",
    );
  }
  writeFileSync(join(root, "fail-notify"), "");
  const failedNotify = await command([binary, "--socket", address, "upgrade", "--binary", binary], {
    allowFailure: true,
    timeout: 30000,
  });
  check(
    failedNotify.code !== 0 &&
      Number((await status()).pid) === oldPid &&
      Number(await systemctl("show", `${unit}.service`, "--property=MainPID", "--value")) ===
        oldPid,
    "notifier failure did not restore old ownership",
  );
  const registry = join(nodeDir, "push-devices.json"),
    saved = readFileSync(registry);
  writeFileSync(registry, "{invalid-owned-fixture");
  const failedState = await command([binary, "--socket", address, "upgrade", "--binary", binary], {
    allowFailure: true,
    timeout: 15000,
  });
  check(
    failedState.code !== 0 && Number((await status()).pid) === oldPid,
    "corrupt candidate state replaced the old node",
  );
  writeFileSync(registry, saved);
  writeFileSync(join(root, "hold-scan"), "");
  await waitFor(() => existsSync(join(root, "scan-inflight")), "in-flight notification scan");
  const registrationErrors: string[] = [],
    rotation = (async () => {
      try {
        await waitFor(() => upgrading, "upgrade start");
        await sleep(50);
        await register("owned-token-after");
      } catch (error) {
        registrationErrors.push(String(error));
      }
    })();
  const upgradeStarted = performance.now();
  appendFileSync(join(packageRoot, "kite-node"), "owned-new-generation");
  await installOwned();
  await rotation;
  check(!registrationErrors.length, registrationErrors.join("; "));
  const upgradeSeconds = (performance.now() - upgradeStarted) / 1000;
  let newPid = Number((await status()).pid);
  check(
    newPid !== oldPid &&
      !managerCalls.some((argv) => argv.includes("restart") || argv.includes("stop")),
    "installer restarted an existing service",
  );
  check(
    Number(await systemctl("show", `${unit}.service`, "--property=MainPID", "--value")) === newPid,
    "manager did not adopt the new generation",
  );
  unlinkSync(join(root, "hold-scan"));
  fact.status = "idle";
  fact.revision = 2;
  atomic(facts, { facts: [fact] });
  let page: ObjectValue = {};
  await waitFor(
    async () => {
      page = object(
        parse(
          (
            await command(
              [binary, "--socket", address, "notifications", "--after", "owned-upgrade-probe:0"],
              { timeout: 6000 },
            )
          ).stdout,
        ),
      );
      return array(page.events).length > 0;
    },
    "completion after handover",
    7000,
  );
  check(array(page.events).length === 1, "completion notification duplicated");
  check(
    object(object(object(parse(readFileSync(registry, "utf8"))).devices)["owned-device/owned-host"])
      .token === "owned-token-after",
    "concurrent registration lost",
  );
  const scans = readFileSync(join(root, "scans"), "utf8")
      .trim()
      .split("\n")
      .map((line) => object(parse(line))),
    oldEnd = scans.findLastIndex((scan) => scan.pid === oldPid && scan.phase === "end"),
    newStart = scans.findIndex((scan) => scan.pid === newPid && scan.phase === "start");
  check(oldEnd < newStart, "notification writers overlapped");
  check(existsSync(`/proc/${oldPid}`), "old generation dropped live streams");
  const parent = (pid: number) =>
    readFileSync(`/proc/${pid}/stat`, "utf8").split(") ").at(-1)?.split(/\s+/)[1];
  check(parent(newPid) === parent(oldPid), "replacement was not adopted by the user manager");
  const crashed = newPid;
  process.kill(crashed, "SIGKILL");
  await waitFor(
    async () => {
      const pid = Number(
        await systemctl("show", `${unit}.service`, "--property=MainPID", "--value"),
      );
      if (!pid || pid === crashed) return false;
      try {
        if (Number((await status()).pid) === pid) {
          newPid = pid;
          return true;
        }
      } catch {
        /* restart is still preparing */
      }
      return false;
    },
    "supervisor crash recovery",
    8000,
  );
  check(existsSync(`/proc/${oldPid}`), "crash recovery dropped old-generation streams");
  const after = await Bridge.connect(address, herdr);
  after.send("PING new\n");
  check((await after.line()) === "PONG new", "new connection did not reach replacement");
  await after.close();
  writeFileSync(join(root, "release"), "");
  check(
    (await once.line()) === "APPLIED" &&
      readFileSync(join(root, "mutations"), "utf8") === "MUTATE once\n",
    "accepted mutation was lost or repeated",
  );
  check(
    Number(readFileSync(join(root, "owner.pid"), "utf8")) === ownerPid &&
      Number(readFileSync(join(root, "heartbeat"), "utf8")) > heartbeat,
    "owned agent did not survive handover",
  );
  process.kill(ownerPid, 0);
  await sleep(200);
  check(!errors.length, errors.join("; "));
  stop = true;
  await pump;
  for (const stream of streams) await stream.close();
  streams.length = 0;
  await waitFor(() => !existsSync(`/proc/${oldPid}`), "old generation drain");
  check(Number((await status()).pid) === newPid, "serving generation changed unexpectedly");
  const result = {
    status: "PASS",
    realUserSystemd: true,
    continuousFrames: samples.length,
    frameErrors: errors,
    maxRoundTripMs: Math.round(Math.max(...samples) * 100) / 100,
    upgradeSeconds: Math.round(upgradeSeconds * 1000) / 1000,
    inFlightMutationExactlyOnce: true,
    sameOwnedAgentPid: true,
    newConnectionsUseNewGeneration: true,
    incompatibleAndBrokenCandidatesRetainOld: true,
    oldGenerationDrained: true,
    notifierFailureRestoresOldMainPid: true,
    realInstallerWithoutRestart: true,
    corruptCandidateStateRetainsOld: true,
    notificationWriterHandoverWithoutDuplicate: true,
    concurrentPushRegistrationRetained: true,
    replacementAdoptedByUserManager: true,
    newGenerationCrashAutomaticallyRecovered: true,
    customizedSkillPreserved: true,
  };
  console.log(JSON.stringify(result));
  if (values.evidence) {
    mkdirSync(dirname(resolve(values.evidence)), { recursive: true });
    atomic(values.evidence, result);
  }
} finally {
  stop = true;
  if (pump) await pump;
  for (const stream of streams) stream.socket.destroy();
  if (!ownerPid && existsSync(join(root, "owner.pid")))
    ownerPid = Number(readFileSync(join(root, "owner.pid"), "utf8"));
  if (ownerPid) {
    try {
      process.kill(ownerPid, "SIGTERM");
    } catch {
      /* already exited */
    }
  }
  try {
    await command(["systemctl", "--user", "stop", `${unit}.service`, `${unit}.socket`], {
      timeout: 15000,
      allowFailure: true,
    });
  } finally {
    rmSync(servicePath, { force: true });
    rmSync(socketPath, { force: true });
    await systemctl("daemon-reload");
    await command(["systemctl", "--user", "reset-failed", `${unit}.service`, `${unit}.socket`], {
      timeout: 5000,
      allowFailure: true,
    });
    if (process.env.KITE_KEEP_FIXTURES) console.error(`Owned fixture retained: ${root}`);
    else rmSync(root, { recursive: true, force: true });
  }
}
