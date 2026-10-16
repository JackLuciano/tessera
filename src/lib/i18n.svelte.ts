/**
 * Minimal i18n: flat key → string dictionaries with {param} interpolation.
 * The locale is detected from the OS/webview language and falls back to English;
 * missing keys in a translation also fall back to English.
 */
import en from './locales/en.json';
import hu from './locales/hu.json';
import de from './locales/de.json';

type Dict = Record<string, string>;
export const LOCALES: Record<string, { name: string; dict: Dict }> = {
  en: { name: 'English', dict: en },
  hu: { name: 'Magyar', dict: hu },
  de: { name: 'Deutsch', dict: de },
};

function detect(): string {
  try {
    const saved = localStorage.getItem('tessera-locale');
    if (saved && LOCALES[saved]) return saved;
  } catch {}
  for (const tag of navigator.languages ?? [navigator.language]) {
    const base = tag.toLowerCase().split('-')[0];
    if (LOCALES[base]) return base;
  }
  return 'en';
}

class I18n {
  locale = $state(detect());
  setLocale(l: string) {
    if (!LOCALES[l]) return;
    this.locale = l;
    try { localStorage.setItem('tessera-locale', l); } catch {}
    document.documentElement.lang = l;
  }
}
export const i18n = new I18n();

export function t(key: string, params?: Record<string, string | number>): string {
  const s = LOCALES[i18n.locale]?.dict[key] ?? (en as Dict)[key] ?? key;
  return params ? s.replace(/\{(\w+)\}/g, (_, k) => String(params[k] ?? `{${k}}`)) : s;
}
