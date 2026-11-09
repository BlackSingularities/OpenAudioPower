// Builds the plug-in host (a separate executable, so a crashing VST3 / CLAP plug-in cannot take
// the app down) and stages it where tauri.conf.json picks it up for the installer.
import { execSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";

execSync("cargo build --release -p oap-plughost", { stdio: "inherit" });
mkdirSync("src-tauri/redist", { recursive: true });
copyFileSync("target/release/oap-plughost.exe", "src-tauri/redist/oap-plughost.exe");
console.log("staged src-tauri/redist/oap-plughost.exe");
