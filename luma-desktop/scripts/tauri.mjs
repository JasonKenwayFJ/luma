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

const args = process.argv.slice(2);
const isAndroidCommand = args[0] === "android";
const buildsAndroidRelease = args[0] === "android" && args[1] === "build";
const createsUpdaterArtifacts = args[0] !== "android" && (args.includes("build") || args.includes("bundle")) && !args.includes("--no-bundle");

if (usesDefaultKey && !env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD && createsUpdaterArtifacts) {
    console.error(
        "The default updater key is encrypted. Set TAURI_SIGNING_PRIVATE_KEY_PASSWORD in this PowerShell session before building."
    );
    process.exit(1);
}

const cliPath = fileURLToPath(new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url));
const patchAndroidMediaPermissions = () => {
    const patchScript = fileURLToPath(new URL("./patch-android-media-permissions.mjs", import.meta.url));
    const patchResult = spawnSync(process.execPath, [patchScript], {
        cwd: process.cwd(),
        env,
        stdio: "inherit",
    });
    if (patchResult.error) {
        console.error(`Could not configure Android media permissions: ${patchResult.error.message}`);
        return 1;
    }
    return patchResult.status ?? 1;
};

if (isAndroidCommand && args[1] !== "init" && (args[1] === "build" || args[1] === "dev")) {
    const patchStatus = patchAndroidMediaPermissions();
    if (patchStatus !== 0) process.exit(patchStatus);
}

if (buildsAndroidRelease) {
    const signingSetup = spawnSync(process.execPath, [fileURLToPath(new URL("./prepare-android-signing.mjs", import.meta.url))], {
        cwd: process.cwd(),
        env,
        stdio: "inherit",
    });
    if (signingSetup.error) {
        console.error(`Could not configure Android signing: ${signingSetup.error.message}`);
        process.exit(1);
    }
    if (signingSetup.status !== 0) process.exit(signingSetup.status ?? 1);
}

const result = spawnSync(process.execPath, [cliPath, ...args], {
    cwd: process.cwd(),
    env,
    stdio: "inherit",
});

if (result.error) {
    console.error(`Could not start Tauri CLI: ${result.error.message}`);
    process.exit(1);
}

if (result.status === 0 && isAndroidCommand && args[1] === "init") {
    const patchStatus = patchAndroidMediaPermissions();
    if (patchStatus !== 0) process.exit(patchStatus);
}

process.exit(result.status ?? 1);
