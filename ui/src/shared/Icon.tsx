import type { ReactNode } from 'react';

const paths = {
  pulse: <path d="M2 12h4l3-7 5 14 3-7h5" />,
  overview: <><rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" /><rect x="3" y="14" width="7" height="7" rx="1.5" /><rect x="14" y="14" width="7" height="7" rx="1.5" /></>,
  models: <><path d="m12 3 9 5-9 5-9-5 9-5Z" /><path d="m3 12 9 5 9-5M3 16l9 5 9-5" /></>,
  projects: <path d="M3 7a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z" />,
  sessions: <><path d="M5 4h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H9l-6 4V6a2 2 0 0 1 2-2Z" /><path d="M7 8h10M7 12h7" /></>,
  events: <><path d="M8 5h13M8 12h13M8 19h13" /><circle cx="3" cy="5" r=".6" /><circle cx="3" cy="12" r=".6" /><circle cx="3" cy="19" r=".6" /></>,
  diagnostics: <path d="M3 12h4l3-8 4 16 3-8h4" />,
  settings: <><path d="M3 6h18M3 12h18M3 18h18" /><rect x="7" y="4" width="3" height="4" rx="1" /><rect x="14" y="10" width="3" height="4" rx="1" /><rect x="6" y="16" width="3" height="4" rx="1" /></>,
  refresh: <path d="M20 7a9 9 0 1 0 1 8M20 3v5h-5" />,
  mini: <><rect x="3" y="4" width="18" height="16" rx="3" /><rect x="12" y="12" width="6" height="5" rx="1" /></>,
  tray: <><path d="M3 13v6a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-6M3 13h5l2 3h4l2-3h5M12 3v8m-4-4 4 4 4-4" /></>,
  close: <path d="m6 6 12 12M18 6 6 18" />,
  pin: <path d="M9 3h6l-1 6 4 4H6l4-4-1-6ZM12 13v8" />,
  expand: <path d="M5 9V5h4M15 5h4v4M19 15v4h-4M9 19H5v-4" />,
  collapse: <path d="M9 5v4H5M19 9h-4V5M15 19v-4h4M5 15h4v4" />,
  calendar: <><rect x="3" y="5" width="18" height="16" rx="3" /><path d="M7 3v4M17 3v4M3 11h18" /></>,
  check: <path d="m5 12 4 4L19 6" />,
  eye: <><path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z" /><circle cx="12" cy="12" r="3" /></>,
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof paths;

/** Decorative icons always accompany a visible label or the button's accessible name. */
export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  return <svg className="ui-icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}
