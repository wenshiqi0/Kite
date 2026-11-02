import { input } from "@kite/runtime";
import { CLIPBOARD_LIMIT, clipboard } from "./clipboard";

try {
  const mode =
    process.argv[process.argv[1] === "read" || process.argv[1] === "write" ? 1 : 2] || "";
  console.log(
    JSON.stringify(
      await clipboard(
        mode,
        Number(process.argv[process.argv[1] === "read" || process.argv[1] === "write" ? 2 : 3]),
        mode === "write" ? await input(CLIPBOARD_LIMIT) : Buffer.alloc(0),
      ),
    ),
  );
} catch {
  process.stderr.write("clipboard operation failed\n");
  process.exitCode = 1;
}
