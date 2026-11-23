import { existsSync, realpathSync } from "node:fs";
import { join } from "node:path";
import {
  array,
  check,
  type DelegationPolicy,
  type ObjectValue,
  object,
  optionalString,
  type RemoteSessionState,
  type SessionRecord,
} from "@kite/protocol";
import { readJson, run, startTicks } from "@kite/runtime";
import { listening, optionalObject } from "@kite/runtime/socket";
import { within } from "./acp-backend";
import { delegatedInteraction, recentTurns } from "./acp-state";
import { backendRpc, readRegistry, registryPath } from "./codex";
import { harnessDirectory, harnessInfo } from "./harness";

export async function ownerPresent(record: SessionRecord): Promise<boolean> {
  if (record.provider === "codex") {
    if (!record.pane_id || !existsSync(registryPath(record.pane_id))) return false;
    const saved = object(readJson(registryPath(record.pane_id)));
    check(
      typeof saved.pid === "number" && typeof saved.start_ticks === "string",
      "invalid Codex owner record",
    );
    if (startTicks(saved.pid) !== saved.start_ticks) {
      check(!(await listening(String(saved.socket))), "Codex owner identity unavailable");
      return false;
    }
    readRegistry(record.pane_id);
    return true;
  }
  if (!record.reference) return false;
  const path = harnessDirectory(record.reference.value),
    statePath = join(path, "state.json");
  check(existsSync(statePath), "managed owner state unavailable");
  const state = object(readJson(statePath));
  const info = harnessInfo(readJson(join(path, "session.json")));
  check(
    info.id === record.id &&
      info.thread === record.reference.value &&
      info.provider === record.provider,
    "managed owner identity changed",
  );
  check(
    typeof state.pid === "number" && typeof state.start_ticks === "string",
    "invalid managed owner record",
  );
  return startTicks(state.pid) === state.start_ticks;
}

export const terminalState = (status: string): RemoteSessionState =>
  status === "failed" ? "failed" : "idle";
export const isActive = (state: RemoteSessionState) =>
  state === "working" || state === "waiting_approval" || state === "waiting_question";

export interface OwnerStatus {
  state: RemoteSessionState;
  activeTurnId?: string;
  lastTurnId?: string;
  lastTurnStatus?: string;
}
export async function inspectOwner(record: SessionRecord): Promise<OwnerStatus> {
  const present = await ownerPresent(record);
  if (!present && record.archived) return { state: "released" };
  const previous = recentTurns(record)[0];
  if (record.reference?.source === "kite:harness") {
    const state = object(readJson(join(harnessDirectory(record.reference.value), "state.json")));
    const id = optionalString(state.turn_id);
    const status = optionalString(state.status);
    const pending = present ? delegatedInteraction(record) : undefined;
    if (present && ["working", "blocked"].includes(status)) {
      check(id, "managed active turn identity unavailable");
      return {
        state:
          pending?.kind === "question"
            ? "waiting_question"
            : status === "blocked"
              ? "waiting_approval"
              : "working",
        activeTurnId: id,
      };
    }
    check(
      ["working", "blocked", "failed", "idle", "done", "cancelled"].includes(status),
      "managed owner status unavailable",
    );
    const last =
      status === "done" ? "completed" : status === "cancelled" ? "interrupted" : "failed";
    if (status !== "idle" && id)
      return { state: terminalState(last), lastTurnId: id, lastTurnStatus: last };
  } else if (record.provider === "codex" && present && record.reference && record.pane_id) {
    const threadId = record.reference.value;
    const thread = object(
      object(
        await backendRpc(
          record.pane_id,
          "thread/read",
          {
            threadId,
            includeTurns: false,
          },
          4000,
        ),
      ).thread,
    );
    check(thread.id === threadId, "Codex owner identity changed");
    const status = object(thread.status);
    const active = status.type === "active";
    check(
      active || ["idle", "systemError", "notLoaded"].includes(String(status.type)),
      "Codex owner status unavailable",
    );
    if (active || !previous) {
      let rows: ObjectValue[] = [];
      try {
        rows = array(
          object(
            await backendRpc(
              record.pane_id,
              "thread/turns/list",
              {
                threadId,
                limit: 1,
                itemsView: "notLoaded",
                sortDirection: "desc",
              },
              4000,
            ),
          ).data,
        ).map(object);
      } catch (error) {
        if (active) throw error;
      }
      const turn = rows[0],
        id = optionalString(turn?.id);
      if (active) {
        check(id && turn?.status === "inProgress", "Codex active turn identity unavailable");
        const flags = Array.isArray(status.activeFlags) ? status.activeFlags : [];
        const pending = delegatedInteraction(record);
        return {
          state:
            flags.includes("waitingOnApproval") || pending?.kind === "approval"
              ? "waiting_approval"
              : flags.includes("waitingOnUserInput") || pending?.kind === "question"
                ? "waiting_question"
                : "working",
          activeTurnId: id,
        };
      }
      if (id && turn?.status !== "inProgress") {
        const last = optionalString(turn?.status);
        return { state: terminalState(last), lastTurnId: id, lastTurnStatus: last };
      }
    }
    if (status.type === "systemError") return { state: "failed" };
  } else if (record.reference || record.pane_id) {
    check(!present && previous, "owner status unavailable");
    // A previously running owner can disappear without producing a terminal event.
    check(record.archived, "owner unavailable; inspect the saved conversation");
  }
  return previous
    ? {
        state: terminalState(previous.lastTurnStatus),
        lastTurnId: previous.turnId,
        lastTurnStatus: previous.lastTurnStatus,
      }
    : { state: "idle" };
}

export async function gitWorkspace(cwd: string, verifyLinked = false) {
  const { code, stdout } = await run(
    [
      "git",
      "-C",
      cwd,
      "rev-parse",
      "--show-toplevel",
      "--absolute-git-dir",
      "--path-format=absolute",
      "--git-common-dir",
      "--abbrev-ref",
      "HEAD",
    ],
    { timeout: 3000, allowFailure: true },
  );
  if (code !== 0) return;
  const [root, admin, common, branch] = stdout.toString().trimEnd().split("\n");
  if (!root || !admin || !common || !branch) return;
  const path = realpathSync(root),
    gitDir = realpathSync(admin),
    commonDir = realpathSync(common);
  let linked = path === cwd && gitDir !== commonDir && within(join(commonDir, "worktrees"), gitDir);
  if (linked && verifyLinked) {
    const worktrees = await run(["git", "-C", cwd, "worktree", "list", "--porcelain", "-z"], {
      timeout: 3000,
    });
    linked = worktrees.stdout.toString().split("\0").includes(`worktree ${cwd}`);
  }
  return { path, branch, linked };
}

export function acpTurnSettings(
  provider: string,
  settings: ObjectValue[] | undefined,
  policy: DelegationPolicy,
): ObjectValue | undefined {
  const isolated = policy === "isolated_auto";
  const mode =
    provider === "antigravity"
      ? (["tool_permission", "default", "turbo"] as const)
      : provider === "ply"
        ? (["approval_mode", "ask", "bypass"] as const)
        : undefined;
  if (!mode) {
    check(!isolated, "provider has not enabled an isolated automatic permission mode");
    return;
  }
  const [id, manual, automatic] = mode;
  const setting = settings?.find((entry) => entry.id === id);
  // Ply without a permission selector uses its existing ask-mode protocol.
  if (!isolated && provider === "ply" && !setting) return;
  const value = isolated ? automatic : manual;
  check(
    setting &&
      (!isolated || setting.currentValue === value) &&
      Array.isArray(setting.options) &&
      setting.options.some((option) => optionalObject(option).value === value),
    isolated
      ? "provider has not enabled an isolated automatic permission mode"
      : "provider permission routing is unavailable",
  );
  return { [id]: value };
}
