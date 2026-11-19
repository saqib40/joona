import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");

const isWindows = process.platform === "win32";
const isRelease = process.argv.includes("--release");
const profile = isRelease ? "release" : "debug";

if (isWindows) {
  const psArgs = [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    path.join(__dirname, "build-engine.ps1"),
    ...(isRelease ? ["-Release"] : []),
  ];
  execSync(`powershell ${psArgs.join(" ")}`, { stdio: "inherit", cwd: root });
  process.exit(0);
}

// On macOS and Linux:
const rustcInfo = execSync("rustc -vV", { encoding: "utf8" });
const match = rustcInfo.match(/^host: (.+)$/m);
if (!match) {
  console.error("Could not determine host triple from rustc -vV");
  process.exit(1);
}
const hostTriple = match[1].trim();

const cargoArgs = ["build", "-p", "joona-engine"];
if (isRelease) cargoArgs.push("--release");
if (process.platform === "darwin") cargoArgs.push("--features", "metal");

console.log(`Running: cargo ${cargoArgs.join(" ")}`);
execSync(`cargo ${cargoArgs.join(" ")}`, { stdio: "inherit", cwd: root });

const exeSuffix = process.platform === "win32" ? ".exe" : "";
const built = path.join(root, "target", profile, `joona-engine${exeSuffix}`);
const binDir = path.join(root, "apps", "desktop", "src-tauri", "binaries");
fs.mkdirSync(binDir, { recursive: true });

const dest = path.join(binDir, `joona-engine-${hostTriple}${exeSuffix}`);
fs.copyFileSync(built, dest);
fs.chmodSync(dest, 0o755);
console.log(`Engine copied to ${dest}`);
