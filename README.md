<div align="center">

<img src="src-tauri/icons/app-icon.svg" width="112" alt="Tessera logo" />

# Tessera

**See what's eating your disk — fast, beautiful, cross-platform.**

A lightweight disk space analyzer for Windows, macOS and Linux with a parallel Rust scanner,
a zoomable treemap and smart cleanup that knows what's safe to delete.

[![Release](https://img.shields.io/github/v/release/JackLuciano/tessera?include_prereleases&style=flat-square&color=7c5cff)](../../releases)
[![CI](https://img.shields.io/github/actions/workflow/status/JackLuciano/tessera/ci.yml?style=flat-square&label=CI)](../../actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-e8489a?style=flat-square)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-Windows%20·%20macOS%20·%20Linux-38bdf8?style=flat-square)
![Size](https://img.shields.io/badge/app%20size-~5%20MB-22c55e?style=flat-square)

<img src="docs/screenshots/overview.png" alt="Tessera overview: disk usage ring, type breakdown and biggest folders" width="900" />

</div>

---

## ✨ Features

|   |   |
|---|---|
| 🚀 **NTFS MFT fast path** | Run as administrator and Tessera reads the Master File Table straight from the volume — a whole drive in seconds, like WizTree. |
| ⚡ **Fast parallel scanner** | Without admin rights (or on other file systems) Rust + rayon walk every folder on all CPU cores. Junctions and symlinks are skipped so nothing is counted twice. |
| 👯 **Duplicate finder** | Size → quick head/tail hash → full **BLAKE3** confirmation. Keep newest, oldest or shortest path and remove the rest. |
| 📈 **History & diffs** | Every scan saves a small snapshot. Chart usage over time and see exactly which folders grew or shrank. |
| ⌨️ **Command palette** | <kbd>Ctrl</kbd>+<kbd>K</kbd> to jump anywhere, rescan, switch drives or open a big file. |
| 🔄 **Auto-update** | Signed updates from GitHub Releases, downloaded in the background and installed when you restart. |
| 🧩 **Zoomable treemap** | Squarified treemap on canvas — click to zoom in with a smooth animation, Backspace to go up, right-click for actions. |
| 🍩 **Breakdown by type** | Video, games, dev, apps, system, images, caches, archives, documents — at a glance. |
| 🧹 **Smart cleanup** | Finds regenerable junk: `node_modules`, build caches (`.next`, `target`, `__pycache__`), package-manager caches, browser caches, temp folders. |
| 📄 **Largest files** | Sortable, filterable list with "not modified in 6 months" detection and bulk delete. |
| 🗑️ **Safe by default** | Everything goes to the Recycle Bin / Trash. Permanent delete is opt-in, needs confirmation, and system locations are always protected. |
| 🌍 **Multilingual** | English, Magyar, Deutsch — detected from your system, English fallback. |
| 🌗 **Light & dark** | Follows the system or pick your own. |
| 🪶 **Tiny** | ~5 MB app thanks to Tauri — uses the OS webview instead of bundling Chromium. |

## 📸 Screenshots

<table>
  <tr>
    <td><img src="docs/screenshots/explorer.png" alt="Treemap explorer" /></td>
    <td><img src="docs/screenshots/cleanup.png" alt="Smart cleanup" /></td>
  </tr>
  <tr>
    <td align="center"><b>Explorer</b> — zoomable treemap</td>
    <td align="center"><b>Smart cleanup</b> — reclaim space safely</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/files.png" alt="Largest files" /></td>
    <td><img src="docs/screenshots/settings.png" alt="Settings" /></td>
  </tr>
  <tr>
    <td align="center"><b>Largest files</b> — filter, sort, bulk delete</td>
    <td align="center"><b>Settings</b> — language, theme, deletion</td>
  </tr>
</table>

> Screenshots are rendered from the [design mockup](design/mockup.html) with sample data.

## 📥 Download

Grab the latest installer from **[Releases](../../releases)**:

| Platform | File |
|---|---|
| Windows 10/11 | `Tessera_x.y.z_x64-setup.exe` (or `.msi`) |
| macOS 11+ (Intel & Apple Silicon) | `Tessera_x.y.z_universal.dmg` |
| Linux | `.AppImage`, `.deb` or `.rpm` |

The Windows setup lets you install **for the current user** (no admin) **or for all users**, in English,
Magyar or Deutsch. It creates a Start menu entry, registers Tessera in **Settings › Apps** (Add/Remove
Programs) and ships a proper uninstaller. Prefer group-policy deployment? Use the `.msi`.

> **Tip:** On Windows, folders like `C:\Windows\System32\config` need administrator rights to read.
> Tessera skips them and shows how many folders it couldn't access.

## 🛠️ Building locally

**Requirements:** [Rust](https://rustup.rs) (stable), [Node.js](https://nodejs.org) 22+, [pnpm](https://pnpm.io) 10+,
and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS
(on Windows: Microsoft C++ Build Tools + WebView2, which Windows 11 already has).

```powershell
# Windows
./scripts/build.ps1            # installers → src-tauri/target/release/bundle/
./scripts/build.ps1 -Dev       # dev mode with hot reload
./scripts/build.ps1 -Portable  # also copies a portable .exe to dist/
```

```bash
# macOS / Linux
./scripts/build.sh             # bundles → src-tauri/target/release/bundle/
./scripts/build.sh --dev       # dev mode with hot reload
```

Or directly: `pnpm install` then `pnpm tauri dev` / `pnpm tauri build`.

## 🧱 Tech stack

```
src-tauri/   Rust core — parallel scanner, size tree, junk rules, delete/trash, Tauri commands
src/         Svelte 5 + Tailwind 4 + daisyUI 5 — views, canvas treemap, i18n
design/      HTML mockup and logo explorations
```

- **Shell:** [Tauri 2](https://tauri.app) · **Core:** Rust, rayon, sysinfo, trash
- **UI:** Svelte 5 (runes), SvelteKit static adapter, Tailwind CSS 4, daisyUI 5
- The scan tree lives in Rust; the UI asks for one level at a time, so even million-file drives stay responsive.

## 🌍 Translations

Strings live in [`src/lib/locales/`](src/lib/locales). To add a language, copy `en.json` to `<code>.json`,
translate the values and register it in [`src/lib/i18n.svelte.ts`](src/lib/i18n.svelte.ts). Missing keys fall back to English.

## 🚀 Releases

Releases are built by [GitHub Actions](.github/workflows/release.yml) for all three platforms when a `v*` tag is
pushed, or manually from the Actions tab (pick one platform to save minutes). The result is a **draft** release
to review before publishing.

```bash
git tag v0.1.0 && git push origin v0.1.0
```

**Auto-update signing.** Updates are verified with a minisign key; the public half is in
`src-tauri/tauri.conf.json`. Maintainers keep the private key in `~/.tauri/tessera.key` (local builds pick it up
automatically) and in the repository secrets `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
for CI. Contributors without the key still get a working build — just without update artifacts.

## 🗺️ Roadmap

- [x] NTFS MFT fast path on Windows (needs admin)
- [x] Duplicate finder (size → partial hash → BLAKE3)
- [x] Usage history & snapshot diffs
- [x] Auto-update from GitHub Releases (signed, via `tauri-plugin-updater`)
- [x] Command palette (Ctrl+K)
- [ ] Watch for live changes (USN Journal / FSEvents / fanotify)
- [ ] Hardlink-based dedupe instead of deleting
- [ ] More languages — [help translate!](CONTRIBUTING.md#translations)

## 🤝 Contributing

**Feel free to contribute!** Bug reports, ideas, translations, new cleanup rules, design feedback and code are all
welcome — no contribution is too small. Good places to start:

- 🌍 add or polish a [translation](src/lib/locales)
- 🧹 teach Smart cleanup a new safe-to-delete cache
- 🐛 pick an issue labelled **`good first issue`**

Read **[CONTRIBUTING.md](CONTRIBUTING.md)** for setup and guidelines. `main` is protected: changes land through
pull requests that pass CI and are approved by a code owner.

## 📄 License

[MIT](LICENSE) © Tessera contributors
