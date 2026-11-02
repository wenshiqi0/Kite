import { basename } from "node:path";
import { run } from "@kite/runtime";

export interface CommandOptions {
  input?: string | Uint8Array;
  timeout?: number;
  env?: NodeJS.ProcessEnv;
  cwd?: string;
  allowFailure?: boolean;
}
export interface CommandResult {
  code: number;
  stdout: string;
  stderr: string;
}
export type Command = (argv: string[], options?: CommandOptions) => Promise<CommandResult>;
export const command: Command = async (argv, options = {}) => {
  const result = await run(argv, { ...options, allowFailure: true });
  if (result.code && !options.allowFailure)
    throw new Error(
      `${basename(argv[0] || "command")} exited ${result.code}: ${result.stderr.toString().slice(0, 2048)}`,
    );
  return { code: result.code, stdout: result.stdout.toString(), stderr: result.stderr.toString() };
};
