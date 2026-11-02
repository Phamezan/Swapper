import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { MAX_FILE_BYTES, syncGuides } from "../../scripts/sync-guides.mjs";

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
    const generalOnly = Buffer.from('{"schema":1,"champion":103,"generatedAt":"2026-10-27","guides":[],"general":[{"champion":103}]}');
    await writeFile(join(source, "champions", "103.json"), generalOnly);
    index.champions["103"] = { sha256: createHash("sha256").update(generalOnly).digest("hex"), guides: 0, general: 1 };
    await writeFile(indexPath, JSON.stringify(index));
    assert.equal(await syncGuides(source, destination), 1);
    index.champions["103"].general = 0;
    await writeFile(indexPath, JSON.stringify(index));
    await assert.rejects(syncGuides(source, destination), /Invalid champion entry/);
    const vodOnly = Buffer.from(JSON.stringify({ schema: 1, champion: 103, generatedAt: '2026-11-01', guides: [], vods: [{ enemy: 238, lane: 'mid', videos: [{ videoId: 'A_b-cD12345' }] }] }));
    await writeFile(join(source, 'champions', '103.json'), vodOnly);
    index.champions['103'] = { sha256: createHash('sha256').update(vodOnly).digest('hex'), guides: 0, vods: 1 };
    await writeFile(indexPath, JSON.stringify(index));
    assert.equal(await syncGuides(source, destination), 1);
    assert.deepEqual(await readFile(join(destination, 'champions', '103.json')), vodOnly);
    for (const count of [0, -1, 1.5, 2049, 2]) {
      index.champions['103'].vods = count;
      await writeFile(indexPath, JSON.stringify(index));
      await assert.rejects(syncGuides(source, destination), /Invalid champion/);
    }
    const fixtureVod = Buffer.from(JSON.stringify({ schema: 1, champion: 103, generatedAt: '2026-11-01', guides: [], vods: [{ enemy: 238, lane: 'mid', videos: [{ fixture: true }] }] }));
    await writeFile(join(source, 'champions', '103.json'), fixtureVod);
    index.champions['103'] = { sha256: createHash('sha256').update(fixtureVod).digest('hex'), guides: 0, vods: 1 };
    await writeFile(indexPath, JSON.stringify(index));
    await assert.rejects(syncGuides(source, destination), /Invalid champion file/);
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

test("champion files up to 8 MB sync and larger ones are refused", async () => {
  assert.equal(MAX_FILE_BYTES, 8 * 1024 * 1024);
  const root = await mkdtemp(join(tmpdir(), "swapper-guide-size-"));
  try {
    const source = join(root, "dist"), destination = join(root, "bundled");
    await mkdir(join(source, "champions"), { recursive: true });
    const json = '{"schema":1,"champion":103,"generatedAt":"2026-11-02","guides":[{"champion":103}]}';
    for (const [size, ok] of [[6 * 1024 * 1024, true], [MAX_FILE_BYTES, true], [MAX_FILE_BYTES + 1, false]]) {
      const bytes = Buffer.from(json.padEnd(size, " "));
      await writeFile(join(source, "champions", "103.json"), bytes);
      await writeFile(join(source, "index.json"), JSON.stringify({ schema: 1, generatedAt: "2026-11-02", champions: { "103": { sha256: createHash("sha256").update(bytes).digest("hex"), guides: 1 } } }));
      if (ok) assert.equal(await syncGuides(source, destination), 1, `${size} bytes`);
      else await assert.rejects(syncGuides(source, destination), /exceeds 8 MB/);
    }
  } finally {
    await rm(root, { recursive: true });
  }
});
