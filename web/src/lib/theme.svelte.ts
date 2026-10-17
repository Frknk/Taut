// Color theme of the panel: follow the OS, or force light or dark. The choice lives in
// localStorage and sets `data-theme` on <html>, which layout.css reads. app.html applies the
// stored value before the first paint, so a forced theme does not flash.

export type Theme = 'system' | 'light' | 'dark';

const STORAGE_KEY = 'taut.theme';

function load(): Theme {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === 'light' || stored === 'dark' ? stored : 'system';
  } catch {
    return 'system';
  }
}

class ThemePreference {
  current = $state<Theme>(load());

  set(theme: Theme): void {
    this.current = theme;
    const root = document.documentElement;
    if (theme === 'system') delete root.dataset.theme;
    else root.dataset.theme = theme;
    try {
      if (theme === 'system') localStorage.removeItem(STORAGE_KEY);
      else localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // Storage unavailable: the choice lasts until reload.
    }
  }
}

export const theme = new ThemePreference();
