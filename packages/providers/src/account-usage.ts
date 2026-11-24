import { type Json, json, type ObjectValue, object, optionalString } from "@kite/protocol";
import { home } from "@kite/runtime";
import { RpcProcess } from "@kite/runtime/rpc";
import { optionalObject } from "@kite/runtime/socket";
import { configuration } from "./harness";
import { readAntigravityUsage } from "./native/antigravity";

async function codexUsage(bin: string): Promise<ObjectValue> {
  const rpc = new RpcProcess(
    [bin, "-c", "check_for_update_on_startup=false", "app-server", "--listen", "stdio://"],
    home(),
  );
  try {
    await rpc.call(
      "initialize",
      { clientInfo: { name: "kite-quota", version: "1" }, capabilities: { experimentalApi: true } },
      4_000,
    );
    rpc.write({ method: "initialized" });
    const response = object(await rpc.call("account/read", { refreshToken: false }, 4_000));
    const account = optionalObject(response.account);
    const type = optionalString(account.type);
    const planType = optionalString(account.planType);
    const limits =
      type === "chatgpt" ? await rpc.call("account/rateLimits/read", {}, 10_000) : undefined;
    return {
      account: { account: type ? { type, ...(planType ? { planType } : {}) } : null },
      limits,
    };
  } finally {
    rpc.close();
  }
}

export async function readHostAccountUsage(): Promise<Json[]> {
  const collect = async (provider: string, read: () => Promise<ObjectValue>): Promise<Json> => {
    try {
      return json({ provider, ...(await read()) });
    } catch {
      return { provider };
    }
  };
  const requests: Promise<Json>[] = [];
  const codex = Bun.which("codex");
  if (codex) requests.push(collect("codex", () => codexUsage(codex)));
  if (process.env.KITE_ENABLE_ANTIGRAVITY === "1") {
    requests.push(
      collect("antigravity", async () => ({
        account: { account: { type: "antigravity" } },
        limits: await readAntigravityUsage(configuration("antigravity").command, home()),
      })),
    );
  }
  return Promise.all(requests);
}
