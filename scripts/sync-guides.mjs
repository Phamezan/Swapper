import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// Mirrors MAX_CHAMPION_BYTES in src-tauri/src/guides/model.rs.
export const MAX_FILE_BYTES = 8 * 1024 * 1024;
const bundled = fileURLToPath(new URL("../src-tauri/resources/guides/", import.meta.url));

export async function syncGuides(source, destination = bundled) {
  const indexBytes = await readFile(resolve(source, "index.json"));
  if (indexBytes.length > 256 * 1024) throw new Error("Guide index exceeds 256 KB.");
  const index = JSON.parse(indexBytes);
  if (index.schema !== 1 || typeof index.generatedAt !== "string" || !index.champions || Array.isArray(index.champions)
    || typeof index.champions !== "object" || Object.keys(index.champions).length > 2048) throw new Error("Invalid guide index.");
  const files = [];
  let blindBytes;
  try { blindBytes = await readFile(resolve(source, "blind-picks.json")); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  if (blindBytes) {
    if (blindBytes.length > 256 * 1024) throw new Error("Blind picks exceed 256 KB.");
    const data = JSON.parse(blindBytes);
    if (data.schema !== 1 || !Array.isArray(data.lists)) throw new Error("Invalid blind picks.");
  }
  for (const [id, entry] of Object.entries(index.champions)) {
    if (!/^[1-9]\d*$/.test(id) || Number(id) > 2147483647 || !/^[a-fA-F0-9]{64}$/.test(entry.sha256)
      || !Number.isInteger(entry.guides) || entry.guides < 0 || entry.guides > 2048
      || !Number.isInteger(entry.general ?? 0) || (entry.general ?? 0) < 0 || (entry.general ?? 0) > 5
      || !Number.isInteger(entry.vods ?? 0) || (entry.vods ?? 0) < 0 || (entry.vods ?? 0) > 2048
      || entry.guides + (entry.general ?? 0) + (entry.vods ?? 0) < 1) throw new Error(`Invalid champion entry: ${id}`);
    const bytes = await readFile(resolve(source, "champions", `${id}.json`));
    if (bytes.length > MAX_FILE_BYTES) throw new Error(`Champion ${id} exceeds 8 MB.`);
    if (createHash("sha256").update(bytes).digest("hex") !== entry.sha256.toLowerCase()) throw new Error(`SHA-256 mismatch for ${id}.`);
    const champion = JSON.parse(bytes);
    if (champion.schema !== 1 || champion.champion !== Number(id) || !Array.isArray(champion.guides)
      || champion.guides.length !== entry.guides || (champion.general?.length ?? 0) !== (entry.general ?? 0)
      || (champion.vods !== undefined && (!Array.isArray(champion.vods) || champion.vods.some(vod => !Array.isArray(vod.videos))))
      || (champion.vods?.length ?? 0) !== (entry.vods ?? 0)
      || (champion.vods ?? []).flatMap(vod => vod.videos).some(video => Object.hasOwn(video, 'fixture'))
      || [...champion.guides, ...(champion.general ?? [])].some(guide => Object.hasOwn(guide, "fixture"))) throw new Error(`Invalid champion file: ${id}`);
    files.push([id, bytes]);
  }
  // Validate the entire snapshot before changing the bundled copy; preserve hash-exact bytes.
  await mkdir(resolve(destination, "champions"), { recursive: true });
  for (const [id, bytes] of files) await writeFile(resolve(destination, "champions", `${id}.json`), bytes);
  if (blindBytes) await writeFile(resolve(destination, "blind-picks.json"), blindBytes);
  await writeFile(resolve(destination, "index.json"), indexBytes);
  return files.length;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try {
    if (!process.argv[2]) throw new Error("Usage: npm run guides:sync -- <local compiled dist directory>");
    console.log(`Bundled ${await syncGuides(resolve(process.argv[2]))} champion guide files.`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
