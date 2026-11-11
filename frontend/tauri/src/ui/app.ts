// Application shell: sidebar plus tabbed main area.

import type { AppInfo, Backend, CriterionInfo } from '../backend';
import type { Store } from '../state';
import { createAbout } from './about';
import { h } from './dom';
import { MapView } from './map-view';
import { createPlaceholder, PLACEHOLDERS } from './placeholders';
import { ReportView } from './report-view';
import { createSidebar } from './sidebar';

type TabId = 'map' | 'report' | 'ephemeris' | 'calendar' | 'sky' | 'moon' | 'trajectory' | 'about';

interface Tab {
  id: TabId;
  label: string;
  panel: HTMLElement;
  setActive?: (active: boolean) => void;
}

const TAB_KEY = 'mooncalc.tab';

export interface AppContext {
  backend: Backend;
  store: Store;
  info: AppInfo;
  criteria: CriterionInfo[];
}

export function createApp(ctx: AppContext): HTMLElement {
  const map = new MapView(ctx.backend, ctx.store);
  const report = new ReportView(ctx.backend, ctx.store, ctx.criteria);

  const tabs: Tab[] = [
    { id: 'map', label: 'Map', panel: map.el, setActive: (a) => map.setActive(a) },
    { id: 'report', label: 'Report', panel: report.el, setActive: (a) => report.setActive(a) },
    { id: 'ephemeris', label: 'Ephemeris', panel: createPlaceholder(PLACEHOLDERS.ephemeris) },
    { id: 'calendar', label: 'Calendar', panel: createPlaceholder(PLACEHOLDERS.calendar) },
    { id: 'sky', label: 'Sky', panel: createPlaceholder(PLACEHOLDERS.sky) },
    { id: 'moon', label: 'Moon', panel: createPlaceholder(PLACEHOLDERS.moon) },
    { id: 'trajectory', label: 'Trajectory', panel: createPlaceholder(PLACEHOLDERS.trajectory) },
    { id: 'about', label: 'About', panel: createAbout(ctx.info) },
  ];

  const buttons = tabs.map((t) =>
    h(
      'button',
      {
        type: 'button',
        role: 'tab',
        id: `tab-${t.id}`,
        'aria-controls': `panel-${t.id}`,
        class: 'tab',
        onclick: () => select(t.id),
      },
      t.label,
    ),
  );
  const panels = tabs.map((t) =>
    h('section', { role: 'tabpanel', id: `panel-${t.id}`, 'aria-labelledby': `tab-${t.id}`, class: `tab-panel panel-${t.id}`, hidden: true }, t.panel),
  );

  let current: TabId | null = null;
  function select(id: TabId, focus = false): void {
    if (id === current) return;
    tabs.forEach((t, i) => {
      const on = t.id === id;
      buttons[i]!.setAttribute('aria-selected', String(on));
      buttons[i]!.tabIndex = on ? 0 : -1;
      panels[i]!.hidden = !on;
      if (focus && on) buttons[i]!.focus();
    });
    // Activate after the panel is visible so it can measure itself.
    const prev = tabs.find((t) => t.id === current);
    prev?.setActive?.(false);
    current = id;
    tabs.find((t) => t.id === id)?.setActive?.(true);
    try {
      localStorage.setItem(TAB_KEY, id);
    } catch {
      // Not remembered; fine.
    }
  }

  const tablist = h('nav', { class: 'tabs', role: 'tablist', 'aria-label': 'Views' }, ...buttons);
  tablist.addEventListener('keydown', (e) => {
    const i = tabs.findIndex((t) => t.id === current);
    const next = e.key === 'ArrowRight' ? i + 1 : e.key === 'ArrowLeft' ? i - 1 : null;
    if (next === null) return;
    e.preventDefault();
    select(tabs[(next + tabs.length) % tabs.length]!.id, true);
  });

  const root = h(
    'div',
    { class: 'app' },
    createSidebar(ctx),
    h('main', { class: 'main' }, tablist, h('div', { class: 'panels' }, ...panels)),
  );

  let saved: string | null = null;
  try {
    saved = localStorage.getItem(TAB_KEY);
  } catch {
    saved = null;
  }
  // Select once the shell is in the document (see main.ts) so layout exists.
  queueMicrotask(() => select(tabs.some((t) => t.id === saved) ? (saved as TabId) : 'map'));
  return root;
}
