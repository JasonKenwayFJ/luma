import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const keyPath = process.env.TAURI_SIGNING_PRIVATE_KEY || `${homedir()}/.tauri/luma.key`;
const signingKeyExists = existsSync(keyPath);
const usesDefaultKey = !process.env.TAURI_SIGNING_PRIVATE_KEY && signingKeyExists;
const env = { ...process.env };

if (!env.TAURI_SIGNING_PRIVATE_KEY && signingKeyExists) {
    env.TAURI_SIGNING_PRIVATE_KEY = keyPath;
}

if (usesDefaultKey && !env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD && process.argv.includes("build")) {
    console.error(
        "The default updater key is encrypted. Set TAURI_SIGNING_PRIVATE_KEY_PASSWORD in this PowerShell session before building."
    );
    process.exit(1);
}

const cliPath = fileURLToPath(new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url));
const result = spawnSync(process.execPath, [cliPath, ...process.argv.slice(2)], {
    cwd: process.cwd(),
    env,
    stdio: "inherit",
});

if (result.error) {
    console.error(`Could not start Tauri CLI: ${result.error.message}`);
    process.exit(1);
}

process.exit(result.status ?? 1);
