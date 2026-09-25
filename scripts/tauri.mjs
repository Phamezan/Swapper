import { createHash } from "node:crypto";
import { mkdirSync } from "node:fs";
import { spawn } from "node:child_process";
import { dirname, join, parse } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const env = { ...process.env };

// GNU windres does not quote Tauri's generated resource path correctly when
// Cargo's target directory lives under a Windows profile with spaces.
if (process.platform === "win32" && /\s/.test(projectRoot) && !env.CARGO_TARGET_DIR) {
  const projectId = createHash("sha256").update(projectRoot.toLowerCase()).digest("hex").slice(0, 12);
  const targetDir = join(parse(projectRoot).root, "Swapper-target", projectId);
  try {
    mkdirSync(targetDir, { recursive: true });
  } catch (error) {
    console.error(`Could not create a space-free Cargo target directory at ${targetDir}: ${error}`);
    process.exit(1);
  }
  env.CARGO_TARGET_DIR = targetDir;
  console.log(`Using space-free Cargo target directory: ${targetDir}`);
}

const cli = join(projectRoot, "node_modules", "@tauri-apps", "cli", "tauri.js");
const child = spawn(process.execPath, [cli, ...process.argv.slice(2)], {
  cwd: projectRoot,
  env,
  stdio: "inherit",
});

child.on("error", (error) => {
  console.error(`Could not start Tauri: ${error}`);
  process.exitCode = 1;
});
child.on("exit", (code, signal) => {
  process.exitCode = code ?? (signal ? 1 : 0);
});
