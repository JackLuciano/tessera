# Contributing to Tessera

Thanks for wanting to help! 💜 Every contribution counts — bug reports, translations, design ideas,
docs fixes and code. You don't need to ask before opening an issue or a small PR.

## Ways to help

- 🐛 **Report a bug** — open an [issue](../../issues/new/choose) with your OS, Tessera version and steps to reproduce.
- 💡 **Suggest a feature** — describe the problem you want solved, not just the solution.
- 🌍 **Translate** — add or improve a language in [`src/lib/locales/`](src/lib/locales) (see below).
- 🧹 **Add a cleanup rule** — know another safe-to-delete cache? Rules live in `find_junk` in [`src-tauri/src/lib.rs`](src-tauri/src/lib.rs).
- 🎨 **Design** — the mockup in [`design/mockup.html`](design/mockup.html) is the visual reference; feedback welcome.
- 📖 **Docs** — typos, clearer explanations, screenshots.

Look for issues labelled **`good first issue`** if you're new.

## Getting set up

1. Install [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 22+, [pnpm](https://pnpm.io) and the
   [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.
2. Fork and clone the repo, then:

   ```bash
   pnpm install
   pnpm tauri dev
   ```

3. Before opening a PR, run the same checks as CI:

   ```bash
   pnpm check
   cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test
   ```

## Pull requests

`main` is protected: every change goes through a pull request that passes CI and is approved by a code owner.

- Branch from `main`, keep PRs focused on one thing.
- Use [Conventional Commits](https://www.conventionalcommits.org) for commit messages (`feat:`, `fix:`, `docs:` …).
- UI changes: include a before/after screenshot — **with sample data, nothing personal from your machine**.
- New user-facing text goes into `en.json` (and other locales if you can); never hard-code strings in components.
- Code comments are in English.

## Translations

1. Copy `src/lib/locales/en.json` to `src/lib/locales/<code>.json` (e.g. `fr.json`).
2. Translate the values, keep the keys and `{placeholders}` as they are.
3. Register the language in `LOCALES` in `src/lib/i18n.svelte.ts`.
4. Optionally add the language to `bundle.windows.nsis.languages` in `src-tauri/tauri.conf.json` so the installer speaks it too.

Missing keys automatically fall back to English, so partial translations are fine.

## Safety rules for code that deletes things

Tessera deletes files, so we're careful:

- Default to the Recycle Bin / Trash; permanent delete must stay opt-in and confirmed.
- Never weaken `is_protected` in `lib.rs` without discussion.
- Cleanup rules must only match data that is **regenerable** (caches, build output, downloaded packages).

## Code of conduct

Be kind and constructive. Harassment or discrimination of any kind isn't tolerated.
