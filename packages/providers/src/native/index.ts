import type { ManagedProvider, NativeAdapter, NativeOptions } from "@kite/protocol";
import { Dsh } from "./dsh";
import { Ply } from "./ply";
export const connectNative = (
  provider: ManagedProvider,
  options: NativeOptions,
): Promise<NativeAdapter> =>
  provider === "dsh" ? new Dsh(options).initialize() : new Ply(options).initialize();
