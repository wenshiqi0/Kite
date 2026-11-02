import { expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { MIN_CGROUP, prepareMemoryLimit, reclaimableMemory } from "./resources";

const MIB = 1024 * 1024;

test("resident usage excludes unmapped cache regardless of active-file status", () => {
  expect(
    reclaimableMemory(128 * MIB, {
      file: 96 * MIB,
      inactive_file: 16 * MIB,
      active_file: 80 * MIB,
      slab_reclaimable: 4 * MIB,
    }),
  ).toBe(100 * MIB);
});

test("mapped files, tmpfs and pending writes remain charged", () => {
  expect(
    reclaimableMemory(128 * MIB, {
      file: 96 * MIB,
      shmem: 16 * MIB,
      file_mapped: 8 * MIB,
      file_dirty: 4 * MIB,
      file_writeback: 2 * MIB,
      slab_reclaimable: 2 * MIB,
    }),
  ).toBe(68 * MIB);
  expect(reclaimableMemory(128 * MIB, {})).toBe(0);
  expect(reclaimableMemory(32 * MIB, { file: 128 * MIB })).toBe(32 * MIB);
  expect(
    reclaimableMemory(64 * MIB, {
      file: 32 * MIB,
      shmem: 32 * MIB,
      file_mapped: 16 * MIB,
    }),
  ).toBe(0);
});

function fixture(current: number, maximum: number, file: number): string {
  const path = mkdtempSync(join(tmpdir(), "kite-memory-limit-"));
  writeFileSync(join(path, "memory.max"), String(maximum));
  writeFileSync(join(path, "memory.current"), String(current));
  writeFileSync(join(path, "memory.stat"), `file ${file}\n`);
  return path;
}

test("reasserting or raising a full kernel limit does not invalidate enforcement", async () => {
  const path = fixture(128 * MIB, 128 * MIB, 96 * MIB);
  try {
    await prepareMemoryLimit(path, 128 * MIB);
    await prepareMemoryLimit(path, 256 * MIB);
    expect(existsSync(join(path, "memory.reclaim"))).toBe(false);
  } finally {
    rmSync(path, { recursive: true });
  }
});

test("unsafe shrink is refused before reclaim or changing the kernel limit", async () => {
  const path = fixture(224 * MIB, 256 * MIB, 16 * MIB);
  try {
    await expect(prepareMemoryLimit(path, 128 * MIB)).rejects.toThrow("resident use");
    expect(existsSync(join(path, "memory.reclaim"))).toBe(false);
    expect(readFileSync(join(path, "memory.max"), "utf8")).toBe(String(256 * MIB));
  } finally {
    rmSync(path, { recursive: true });
  }
});

test("failed cache reclaim cannot apply a lower kernel limit", async () => {
  const path = fixture(224 * MIB, 256 * MIB, 192 * MIB);
  try {
    await expect(prepareMemoryLimit(path, 128 * MIB)).rejects.toThrow("could not be reclaimed");
    expect(readFileSync(join(path, "memory.reclaim"), "utf8")).toBe(
      `${96 * MIB + MIN_CGROUP} swappiness=0\n`,
    );
    expect(readFileSync(join(path, "memory.max"), "utf8")).toBe(String(256 * MIB));
  } finally {
    rmSync(path, { recursive: true });
  }
});
