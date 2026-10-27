import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { syncGuides } from "../../scripts/sync-guides.mjs";

test("offline sync preserves exact compiled bytes and rejects mismatches before writing", async () => {
  const root = await mkdtemp(join(tmpdir(), "swapper-guide-sync-"));
  try {
    const source = join(root, "dist"), destination = join(root, "bundled");
    await mkdir(join(source, "champions"), { recursive: true });
    const bytes = Buffer.from('{"schema":1,"champion":103,"generatedAt":"2026-10-27","guides":[{"champion":103}]}\r\n');
    const index = { schema: 1, generatedAt: "2026-10-27", champions: { "103": { sha256: createHash("sha256").update(bytes).digest("hex"), guides: 1 } } };
    const indexPath = join(source, "index.json");
    await writeFile(indexPath, JSON.stringify(index));
    await writeFile(join(source, "champions", "103.json"), bytes);
    assert.equal(await syncGuides(source, destination), 1);
    assert.deepEqual(await readFile(join(destination, "champions", "103.json")), bytes);
    const originalIndex = await readFile(join(destination, "index.json"));
    index.champions["103"].sha256 = "0".repeat(64);
    await writeFile(indexPath, JSON.stringify(index));
    await assert.rejects(syncGuides(source, destination), /SHA-256/);
    assert.deepEqual(await readFile(join(destination, "index.json")), originalIndex);
    index.champions = { "../private": { sha256: "0".repeat(64), guides: 1 } };
    await writeFile(indexPath, JSON.stringify(index));
    await assert.rejects(syncGuides(source, destination), /Invalid champion/);
    index.schema = 2;
    await writeFile(indexPath, JSON.stringify(index));
    await assert.rejects(syncGuides(source, destination), /Invalid guide index/);
  } finally {
    await rm(root, { recursive: true });
  }
});
