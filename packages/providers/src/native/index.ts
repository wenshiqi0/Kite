import type { ManagedProvider, NativeAdapter, NativeOptions } from "@kite/protocol";
import { Antigravity } from "./antigravity";
import { Dsh } from "./dsh";
import { Ply } from "./ply";
export function connectNative(
  provider: ManagedProvider,
  options: NativeOptions,
): Promise<NativeAdapter> {
  switch (provider) {
    case "dsh":
      return new Dsh(options).initialize();
    case "ply":
      return new Ply(options).initialize();
    case "antigravity":
      return new Antigravity(options).initialize();
  }
}
