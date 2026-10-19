// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

export async function load() {
  // `pnpm dev` opened in a normal browser: swap in the fake backend so the UI can be previewed.
  // The dynamic import sits behind DEV, so production builds never include the mock.
  if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
    const { installMock } = await import('$lib/dev/mock');
    installMock();
  }
  return {};
}
