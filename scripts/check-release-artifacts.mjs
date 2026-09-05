import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const nsisRoot = join("src-tauri", "target", "release", "bundle", "nsis");
const { version } = JSON.parse(readFileSync("package.json", "utf8"));

function filesUnder(root) {
  if (!existsSync(root)) return [];
  return readdirSync(root, { recursive: true })
    .map(value => join(root, String(value)))
    .filter(value => statSync(value).isFile());
}

const files = filesUnder(nsisRoot);
const installers = files.filter(value => value.endsWith(`ARZ Predictor_${version}_x64-setup.exe`));
const signatures = installers.filter(value => existsSync(`${value}.sig`));

if (installers.length === 0) throw new Error("NSIS installer was not generated.");
if (signatures.length !== installers.length) throw new Error("A signed updater sidecar is missing.");

console.log(`NSIS installers: ${installers.length}`);
console.log(`Signed NSIS updater artifacts: ${signatures.length}`);
