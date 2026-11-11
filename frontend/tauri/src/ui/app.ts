// Application shell: sidebar plus tabbed main area.

import type { AppInfo, Backend, CriterionInfo } from '../backend';
import type { Store } from '../state';
import { createAbout } from './about';
import { CalendarView } from './calendar-view';
import { EphemerisView } from './ephemeris-view';
import { createEarliestView } from './earliest-view';
import { h } from './dom';
import { MapView } from './map-view';
import { ReportView } from './report-view';
import { MoonTabView } from './moon-view';
import { SkyView } from './sky-view';
import { TrajectoryView } from './trajectory-view';
import { createSidebar } from './sidebar';

type TabId = 'map' | 'report' | 'earliest' | 'ephemeris' | 'calendar' | 'sky' | 'moon' | 'trajectory' | 'about';

interface Tab {
  id: TabId;
  label: string;
  panel: HTMLElement;
  setActive?: (active: boolean) => void;
}

const TAB_KEY = 'mooncalc.tab';

/** The panel and activation hook of a view object. */
const view = (v: { el: HTMLElement; setActive(a: boolean): void }) => ({ panel: v.el, setActive: (a: boolean) => v.setActive(a) });

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
    {
      id: 'earliest',
      label: 'Earliest sighting',
      panel: createEarliestView({
        backend: ctx.backend,
        store: ctx.store,
        criteria: ctx.criteria,
        showMap: (date, latDeg, lonDeg) => {
          // Noon local time on that date, so the map shows its evening.
          const utMinutes = Math.round((12 - ctx.store.get().tzOffsetHours) * 60);
          const dayShift = Math.floor(utMinutes / 1440);
          const minuteOfDay = utMinutes - dayShift * 1440;
          const base = new Date(0);
          base.setUTCFullYear(date.year, date.month - 1, date.day + dayShift);
          ctx.store.update({
            crescent: 'evening',
            instant: {
              year: base.getUTCFullYear(),
              month: base.getUTCMonth() + 1,
              day: base.getUTCDate(),
              hour: Math.floor(minuteOfDay / 60),
              minute: minuteOfDay % 60,
              second: 0,
            },
          });
          select('map');
          map.centreOn(latDeg, lonDeg);
        },
      }),
    },
    { id: 'ephemeris', label: 'Ephemeris', ...view(new EphemerisView(ctx)) },
    { id: 'calendar', label: 'Calendar', ...view(new CalendarView(ctx)) },
    { id: 'sky', label: 'Sky', ...view(new SkyView(ctx.backend, ctx.store)) },
    { id: 'moon', label: 'Moon', ...view(new MoonTabView(ctx.backend, ctx.store)) },
    { id: 'trajectory', label: 'Trajectory', ...view(new TrajectoryView(ctx.backend, ctx.store)) },
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
