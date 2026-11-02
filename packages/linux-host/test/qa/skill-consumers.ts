/** Optional installed skill-loader acceptance. No model turn or global config mutation. */
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { array, check, object, string } from "@kite/protocol";
import { RpcProcess } from "@kite/runtime/rpc";
import { command } from "../../src/command";

const { values } = parseArgs({
  options: {
    binary: { type: "string" },
    codex: { type: "string" },
    "dsh-source": { type: "string" },
    output: { type: "string" },
  },
  strict: true,
});
check(values.binary, "--binary is required");
const body = (await command([resolve(values.binary), "docs", "skill"])).stdout;
check(body.startsWith("---\nname: kite\n"), "standard skill frontmatter missing");
const result: {
    standardSkill: boolean;
    modelTurns: number;
    consumers: Record<string, unknown>;
    claude: string;
    status: string;
  } = {
    standardSkill: true,
    modelTurns: 0,
    consumers: {},
    claude: "Standard Agent Skills frontmatter and MCP contract; no live model invocation.",
    status: "PASS",
  },
  root = mkdtempSync(join(tmpdir(), "kite-skill-consumers-"));
try {
  const project = join(root, "project"),
    skills = join(project, ".agents/skills"),
    env = Object.fromEntries(
      Object.entries(process.env).filter(
        ([key]) => !key.startsWith("HERDR_") && key !== "CODEX_THREAD_ID",
      ),
    );
  mkdirSync(join(project, ".git"), { recursive: true });
  mkdirSync(join(skills, "kite"), { recursive: true });
  writeFileSync(join(skills, "kite/SKILL.md"), body);
  const codex = values.codex || Bun.which("codex");
  if (codex) {
    const rpc = new RpcProcess(
      [codex, "app-server", "--listen", "stdio://"],
      project,
      () => {},
      () => {},
      env,
    );
    try {
      await rpc.call("initialize", {
        clientInfo: { name: "kite-skill-contract", version: "1" },
        capabilities: { experimentalApi: true },
      });
      rpc.write({ method: "initialized" });
      const catalogue = object(
          await rpc.call("skills/list", { cwds: [project], forceReload: true }),
        ),
        found = array(catalogue.data)
          .map(object)
          .flatMap((row) => array(row.skills || []).map(object))
          .filter((skill) => skill.name === "kite" && string(skill.path).startsWith(skills));
      check(found.length === 1, "Codex did not discover the project standard skill");
      result.consumers.codex = { skillDiscovered: true, modelInvoked: false };
    } finally {
      rpc.close();
      await rpc.child.exited;
    }
  }
  if (values["dsh-source"]) {
    const script = `import { Context } from '@deepseek-ai/cordis';import SkillRegistry from '@deepseek-ai/dsh-skill';import * as SkillFileSystem from '@deepseek-ai/dsh-skill-filesystem';const ctx=new Context();await ctx.plugin(SkillRegistry);await ctx.plugin(SkillFileSystem,{includeDefaultRoots:false,customSkillDirs:[process.env.KITE_SKILL_ROOT],watch:false});const listed=await ctx.skills.list({cwd:process.env.KITE_PROJECT});const loaded=await ctx.skills.get('kite',{cwd:process.env.KITE_PROJECT});if(listed.length!==1||!loaded||!JSON.stringify(loaded).includes('kite docs desktop'))throw Error('Portable skill not loaded');console.log(JSON.stringify({skillDiscovered:true,bodyLoaded:true,modelInvoked:false}));`;
    result.consumers.deepseek_harness = JSON.parse(
      (
        await command(["node", "--import", "tsx/esm", "--input-type=module", "-e", script], {
          cwd: values["dsh-source"],
          env: { ...env, KITE_SKILL_ROOT: skills, KITE_PROJECT: project },
          timeout: 25000,
        })
      ).stdout,
    );
  }
  console.log(JSON.stringify(result, null, 2));
  if (values.output) {
    mkdirSync(dirname(resolve(values.output)), { recursive: true });
    writeFileSync(values.output, `${JSON.stringify(result, null, 2)}\n`);
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}
