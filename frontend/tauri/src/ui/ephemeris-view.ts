// Ephemeris tab: tables and event lists for the Sun and Moon.
//
// Sub-views, each computed when shown and its inputs changed:
// Table (configurable start, step, count and columns, copy as CSV), Rise/set
// (a month at the app location), Lunar phases, Apsides & nodes, Eclipses
// (list plus a details panel with local circumstances) and Seasons (with the
// equation of time). Times are shown in the app time zone; clicking one sets
// the app date and time.

import './calendar.css';
import './ephemeris.css';

import { BackendError, type Backend } from '../backend';
import type { Store } from '../state';
import { h, replaceChildren } from './dom';
import { createEclipsesView } from './ephemeris-eclipses';
import { createApsidesView, createPhasesView, createRiseSetView, createSeasonsView } from './ephemeris-events';
import { createTableView } from './ephemeris-table';

/** One sub-view: its element and a hook called when it is shown. */
export interface SubView {
  el: HTMLElement;
  show(): void;
}

export interface EphemerisContext {
  backend: Backend;
  store: Store;
}

const VIEW_KEY = 'mooncalc.ephemeris.view';

/**
 * Run a computation for a sub-view: show progress in `status`, drop stale
 * answers, and report errors.
 */
export function runner(status: HTMLElement) {
  let seq = 0;
  return async function run<T>(label: string, call: () => Promise<T>, done: (r: T) => void): Promise<void> {
    const my = ++seq;
    replaceChildren(status, h('span', { class: 'spinner', 'aria-hidden': 'true' }), ` ${label}…`);
    const t0 = performance.now();
    try {
      const r = await call();
      if (my !== seq) return;
      replaceChildren(status, `${((performance.now() - t0) / 1000).toFixed(2)} s`);
      done(r);
    } catch (err) {
      if (my !== seq) return;
      replaceChildren(status, h('span', { class: 'error' }, BackendError.from(err).message));
    }
  };
}

export class EphemerisView {
  readonly el = h('div', { class: 'report-view ephemeris-view' });
  private readonly views: { id: string; label: string; view: SubView }[];
  private current = '';
  private active = false;

  constructor(ctx: EphemerisContext) {
    this.views = [
      { id: 'table', label: 'Table', view: createTableView(ctx) },
      { id: 'riseset', label: 'Rise/set', view: createRiseSetView(ctx) },
      { id: 'phases', label: 'Lunar phases', view: createPhasesView(ctx) },
      { id: 'apsides', label: 'Apsides & nodes', view: createApsidesView(ctx) },
      { id: 'eclipses', label: 'Eclipses', view: createEclipsesView(ctx) },
      { id: 'seasons', label: 'Seasons', view: createSeasonsView(ctx) },
    ];
    const buttons = this.views.map((v) =>
      h('button', { type: 'button', class: 'subtab', 'data-id': v.id, onclick: () => this.select(v.id) }, v.label),
    );
    const panes = this.views.map((v) => {
      const pane = h('div', { class: 'subpane', 'data-id': v.id, hidden: true }, v.view.el);
      return pane;
    });
    replaceChildren(this.el, h('h2', {}, 'Ephemeris'), h('nav', { class: 'subtabs', role: 'tablist', 'aria-label': 'Ephemeris views' }, ...buttons), ...panes);
    let saved: string | null = null;
    try {
      saved = localStorage.getItem(VIEW_KEY);
    } catch {
      saved = null;
    }
    this.current = this.views.some((v) => v.id === saved) ? saved! : 'table';
    this.layout();
  }

  setActive(active: boolean): void {
    this.active = active;
    if (active) this.views.find((v) => v.id === this.current)?.view.show();
  }

  private select(id: string): void {
    this.current = id;
    try {
      localStorage.setItem(VIEW_KEY, id);
    } catch {
      // Not remembered.
    }
    this.layout();
    if (this.active) this.views.find((v) => v.id === id)?.view.show();
  }

  private layout(): void {
    for (const b of this.el.querySelectorAll<HTMLElement>('.subtab')) b.setAttribute('aria-selected', String(b.dataset.id === this.current));
    for (const p of this.el.querySelectorAll<HTMLElement>('.subpane')) p.hidden = p.dataset.id !== this.current;
  }
}
