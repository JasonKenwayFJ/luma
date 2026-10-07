# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Tauri updater

Updater bundles must be signed with the private key that matches `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. The `tauri` npm script sets `TAURI_SIGNING_PRIVATE_KEY` to `~/.tauri/luma.key` when that file exists, so the regular command works on this machine:

```powershell
npm run tauri -- build
```

If the key is stored elsewhere, set the environment variable to its path before building:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = 'C:\path\to\luma.key'
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<the password used when the key was generated>'
npm run tauri -- build
```

The default key at `~/.tauri/luma.key` is encrypted, so its original password is required even when the key path is detected automatically. Never commit or share the private key or its password. `.env` files are not read by the Tauri bundler.

Pushing a `v*` tag runs `.github/workflows/release.yml`. Add the matching private key contents as the repository Actions secret `TAURI_SIGNING_PRIVATE_KEY` and, if needed, its password as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The workflow creates a draft GitHub Release with signed bundles and `latest.json`; publish the draft to make it available to the updater.
