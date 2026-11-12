// Per-view display options remembered in localStorage (view mode, zoom,
// toggles, orientation), next to the app settings in state.ts. Storage can be
// missing or throw (private windows, blocked site data); the views then just
// start from their defaults.

/**
 * Display options saved under one key. `load` returns the defaults with every
 * saved field whose type matches the default's (and that passes `valid`, if
 * given) laid over them, so renamed or corrupt fields fall back silently.
 */
export class ViewPrefs<T extends Record<string, string | number | boolean>> {
  constructor(
    private readonly key: string,
    private readonly defaults: T,
    private readonly valid: { [K in keyof T]?: (v: T[K]) => boolean } = {},
  ) {}

  load(): T {
    const out = { ...this.defaults };
    try {
      const raw = localStorage.getItem(this.key);
      if (!raw) return out;
      const saved = JSON.parse(raw) as Record<string, unknown>;
      if (!saved || typeof saved !== 'object') return out;
      for (const k of Object.keys(this.defaults) as (keyof T & string)[]) {
        const v = saved[k];
        if (typeof v !== typeof this.defaults[k]) continue;
        if (typeof v === 'number' && !Number.isFinite(v)) continue;
        const ok = this.valid[k];
        if (ok && !ok(v as T[typeof k])) continue;
        out[k] = v as T[typeof k];
      }
    } catch {
      // Unavailable or unreadable: defaults.
    }
    return out;
  }

  save(value: T): void {
    try {
      localStorage.setItem(this.key, JSON.stringify(value));
    } catch {
      // Storage unavailable (private mode, quota): not remembered.
    }
  }
}
