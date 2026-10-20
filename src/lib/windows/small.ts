/**
 * Which small window this page is (overlay, widget, prompt) and the game for the boost prompt.
 * The app sets `window.__TESSERA_WINDOW` before the page loads; the dev preview in a browser uses
 * `?window=…&game=…` instead.
 */
const injected = (globalThis as { __TESSERA_WINDOW?: { window?: string; game?: string } }).__TESSERA_WINDOW;
const query = typeof location !== 'undefined' ? new URLSearchParams(location.search) : new URLSearchParams();

export const SMALL_KIND: string | null = injected?.window || query.get('window');
export const PROMPT_GAME: string = injected?.game || query.get('game') || '';
