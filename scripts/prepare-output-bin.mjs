// Phase 5.2 — stage the crash-isolated output process for bundling.
//
// Builds the `sundaystage-output` binary in release mode and copies it to the
// triple-suffixed `src-tauri/binaries/output-process-<triple>` path Tauri's
// `bundle.externalBin` expects, replacing the empty placeholder build.rs
// maintains for plain cargo builds. Runs from `beforeBuildCommand` so
// `npm run tauri build` always bundles a real, current sidecar.
import { execSync } from "node:child_process";
import { copyFileSync, mkdirSync, chmodSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const srcTauri = join(root, "src-tauri");

const triple = execSync("rustc -vV", { encoding: "utf8" })
  .split("\n")
  .find((l) => l.startsWith("host:"))
  .split(":")[1]
  .trim();
const ext = triple.includes("windows") ? ".exe" : "";

console.log(`[prepare-output-bin] building sundaystage-output (${triple})…`);
execSync("cargo build --release --bin sundaystage-output", {
  cwd: srcTauri,
  stdio: "inherit",
});

const built = join(srcTauri, "target", "release", `sundaystage-output${ext}`);
const dest = join(srcTauri, "binaries", `output-process-${triple}${ext}`);
mkdirSync(dirname(dest), { recursive: true });
copyFileSync(built, dest);
chmodSync(dest, 0o755);
console.log(`[prepare-output-bin] staged ${dest}`);
