// Theme preference: system, dark or light. index.html sets the first value
// before paint; this module keeps it in sync after that.

export type ThemePref = 'system' | 'dark' | 'light';
export type ThemeName = 'dark' | 'light';

const KEY = 'halogen-theme';
const mq = window.matchMedia('(prefers-color-scheme: dark)');

function readPref(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v === 'dark' || v === 'light' || v === 'system') return v;
  } catch {
    /* storage can be blocked */
  }
  return 'system';
}

class Theme {
  pref = $state<ThemePref>(readPref());
  system = $state<ThemeName>(mq.matches ? 'dark' : 'light');
  resolved = $derived<ThemeName>(this.pref === 'system' ? this.system : this.pref);

  constructor() {
    mq.addEventListener('change', (e) => {
      this.system = e.matches ? 'dark' : 'light';
      this.apply();
    });
  }

  set(p: ThemePref): void {
    this.pref = p;
    try {
      localStorage.setItem(KEY, p);
    } catch {
      /* storage can be blocked */
    }
    this.apply();
  }

  apply(): void {
    const root = document.documentElement;
    root.dataset.theme = this.resolved;
    root.style.colorScheme = this.resolved;
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute('content', this.resolved === 'dark' ? '#0d0a14' : '#f6f4fb');
  }
}

export const theme = new Theme();

/** Read a CSS custom property of the root element. */
export function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}
