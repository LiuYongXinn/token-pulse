import { useEffect } from 'react';
import type { AppTheme } from './generated/contracts';

/** Shared by both webviews. Saved preference is authoritative; system changes only affect system mode. */
export function useAppTheme(theme: AppTheme | undefined) {
  useEffect(() => {
    const preference = theme ?? 'dark';
    const system = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = () => {
      document.documentElement.dataset.themePreference = preference;
      document.documentElement.dataset.theme = preference === 'system' ? (system.matches ? 'dark' : 'light') : preference;
    };
    apply();
    if (preference === 'system') system.addEventListener('change', apply);
    return () => { if (preference === 'system') system.removeEventListener('change', apply); };
  }, [theme]);
}
