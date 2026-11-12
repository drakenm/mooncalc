// Calendar tab: Hijri calendars from the `calendar` service.
//
// A calendar selector (built-in tables and arithmetical calendars, rule
// calendars and a configurable visibility-based calendar computed with the
// selected ephemeris), a month view laid out by the Hijri or the Gregorian
// month (clicking a day sets the app date, keeping the local time), a
// converter, and the start of every month of a Hijri year under every
// calendar with disagreements highlighted. Month and weekday names are shown
// in Arabic script as well as transliterated.

import './calendar.css';

import {
  BackendError,
  type Backend,
  type CalendarCatalog,
  type CalendarComparison,
  type CalendarConversion,
  type CalendarDay,
  type CalendarMonth,
  type CalendarSpec,
  type CivilDate,
  type CriterionInfo,
  type HijriYmd,
  type MonthRef,
  type PrimaryCalendar,
  type VisibilityCalendarSpec,
} from '../backend';
import { changed, type Store } from '../state';
import { appDay, longDate, MONTH_NAMES, sameDay, setAppDay, shortDate, todayLocal } from './app-time';
import { debounce, h, replaceChildren } from './dom';
import { tzOf } from '../zone';

const SETTINGS_KEY = 'mooncalc.calendar.v1';

interface VisibilitySettings {
  criterionId: string;
  maxZone: number;
  /** "location", "mecca" or a region id. */
  observers: string;
  gridStepDeg: number;
}

interface CalendarSettings {
  calendarId: string;
  primary: PrimaryCalendar;
  /** 0 = Sunday ... 6 = Saturday. */
  weekStart: number;
  visibility: VisibilitySettings;
}

const DEFAULT_SETTINGS: CalendarSettings = {
  calendarId: 'umm-al-qura',
  primary: 'hijri',
  weekStart: 6,
  visibility: { criterionId: 'yallop', maxZone: 1, observers: 'location', gridStepDeg: 5 },
};

function loadSettings(): CalendarSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return structuredClone(DEFAULT_SETTINGS);
    const s = JSON.parse(raw) as Partial<CalendarSettings>;
    return { ...DEFAULT_SETTINGS, ...s, visibility: { ...DEFAULT_SETTINGS.visibility, ...s.visibility } };
  } catch {
    return structuredClone(DEFAULT_SETTINGS);
  }
}

function saveSettings(s: CalendarSettings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    // Not remembered; fine.
  }
}

/** Short Hijri month labels for month-view cells. */
const HIJRI_ABBR = ['Muh', 'Saf', 'Rab I', 'Rab II', 'Jum I', 'Jum II', 'Raj', 'Sha', 'Ram', 'Shw', 'Dhu Q', 'Dhu H'];

const arabicDigits = new Intl.NumberFormat('ar-SA', { useGrouping: false });
const ar = (n: number) => arabicDigits.format(n);

/** Arabic-script text, laid out right to left. */
const arabic = (text: string, cls = 'arabic') => h('span', { class: cls, lang: 'ar', dir: 'rtl' }, text);

export interface CalendarContext {
  backend: Backend;
  store: Store;
  criteria: CriterionInfo[];
}

export class CalendarView {
  readonly el = h('div', { class: 'report-view calendar-view' });
  private catalog: CalendarCatalog | null = null;
  private settings = loadSettings();
  private active = false;
  private stale = true;

  /** The month on screen; null follows the selected date. */
  private view: MonthRef | null = null;
  /** Hijri year of the comparison; null follows the selected date. */
  private compareYear: number | null = null;
  private selectedHijri: HijriYmd | null = null;
  private seq = { month: 0, selected: 0, compare: 0, civil: 0, hijri: 0 };

  // Fixed parts of the page.
  private readonly calendarSelect = h('select', { 'aria-label': 'Calendar' });
  private readonly visibilityBox = h('div', { class: 'visibility-settings' });
  private readonly calendarInfo = h('p', { class: 'hint' });
  private readonly status = h('span', { class: 'status' });
  private readonly selectedLine = h('p', { class: 'selected-line' });
  private readonly monthTitle = h('div', { class: 'month-title' });
  private readonly monthGrid = h('div', { class: 'month-grid-wrap' });
  private readonly converterOut = { civil: h('div', { class: 'conv-result' }), hijri: h('div', { class: 'conv-result' }) };
  private readonly compareTitle = h('span', {});
  private readonly compareTable = h('div', { class: 'compare-wrap' });

  // Converter inputs.
  private readonly civilDay = h('input', { type: 'number', min: 1, max: 31, step: 1, 'aria-label': 'Day', class: 'day-input' });
  private readonly civilMonth = h('select', { 'aria-label': 'Month' }, ...MONTH_NAMES.map((m, i) => h('option', { value: i + 1 }, m)));
  private readonly civilYear = h('input', { type: 'number', min: -4712, max: 9999, step: 1, 'aria-label': 'Year', class: 'year-input' });
  private readonly hijriDay = h('input', { type: 'number', min: 1, max: 30, step: 1, 'aria-label': 'Hijri day', class: 'day-input' });
  private readonly hijriMonth = h('select', { 'aria-label': 'Hijri month' });
  private readonly hijriYear = h('input', { type: 'number', min: 1, max: 9999, step: 1, 'aria-label': 'Hijri year', class: 'year-input' });
  private readonly compareYearInput = h('input', { type: 'number', min: 1, max: 9999, step: 1, 'aria-label': 'Hijri year', class: 'year-input' });
  private readonly followButton = h('button', { type: 'button', class: 'button small', hidden: true }, 'Follow selected date');

  constructor(private readonly ctx: CalendarContext) {
    const refresh = debounce(() => void this.refreshAll(), 120);
    ctx.store.subscribe((s, prev) => {
      const visLoc = this.settings.visibility.observers === 'location';
      if (
        changed(s, prev, ['instant', 'tzOffsetHours', 'timeZone', 'ephemerisId']) ||
        (visLoc && changed(s, prev, ['location', 'locationName']))
      ) {
        if (changed(s, prev, ['instant', 'tzOffsetHours', 'timeZone'])) this.view = null;
        this.stale = true;
        if (this.active) refresh();
      }
    });
    this.build();
  }

  setActive(active: boolean): void {
    this.active = active;
    if (active && this.stale) void this.refreshAll();
  }

  // --- requests -------------------------------------------------------------

  private visibilitySpec(): VisibilityCalendarSpec {
    const s = this.ctx.store.get();
    const v = this.settings.visibility;
    return {
      criterionId: v.criterionId,
      maxZone: v.maxZone,
      observers: v.observers,
      location: s.location,
      locationName: s.locationName,
      gridStepDeg: v.gridStepDeg,
    };
  }

  private spec(): CalendarSpec {
    return { id: this.settings.calendarId, visibility: this.visibilitySpec() };
  }

  private description() {
    return this.catalog?.calendars.find((c) => c.id === this.settings.calendarId) ?? null;
  }

  private computed(): boolean {
    return this.description()?.computed ?? false;
  }

  private busy = 0;
  private setBusy(delta: number): void {
    this.busy += delta;
    this.status.textContent = this.busy > 0 ? (this.computed() ? 'Computing month starts…' : 'Loading…') : '';
  }

  private async run<T>(key: keyof CalendarView['seq'], call: () => Promise<T>, done: (r: T) => void, fail: (msg: string) => void): Promise<void> {
    const my = ++this.seq[key];
    this.setBusy(1);
    try {
      const r = await call();
      if (my === this.seq[key]) done(r);
    } catch (err) {
      if (my === this.seq[key]) fail(BackendError.from(err).message);
    } finally {
      this.setBusy(-1);
    }
  }

  private async refreshAll(): Promise<void> {
    this.stale = false;
    if (!this.catalog) {
      try {
        this.catalog = await this.ctx.backend.calendar.list({ visibility: this.visibilitySpec() });
        this.fillSelectors();
      } catch (err) {
        replaceChildren(this.el, h('p', { class: 'error' }, `Calendars unavailable: ${BackendError.from(err).message}`));
        return;
      }
    }
    this.renderInfo();
    void this.refreshSelected();
    void this.refreshMonth();
    this.fillConverterFromSelected();
  }

  /** The selected date's Hijri date; then the comparison (which follows it). */
  private async refreshSelected(): Promise<void> {
    const s = this.ctx.store.get();
    const date = appDay(this.ctx.store);
    await this.run(
      'selected',
      () => this.ctx.backend.calendar.convert({ calendar: this.spec(), civil: date, ephemerisId: s.ephemerisId }),
      (r) => {
        this.selectedHijri = r.day?.hijri ?? null;
        this.renderSelected(date, r);
        void this.refreshCompare();
      },
      (msg) => {
        this.selectedHijri = null;
        replaceChildren(this.selectedLine, h('span', { class: 'error' }, msg));
        void this.refreshCompare();
      },
    );
  }

  private async refreshMonth(): Promise<void> {
    const s = this.ctx.store.get();
    await this.run(
      'month',
      () =>
        this.ctx.backend.calendar.month({
          calendar: this.spec(),
          primary: this.settings.primary,
          month: this.view,
          date: this.view ? null : appDay(this.ctx.store),
          weekStart: this.settings.weekStart,
          ephemerisId: s.ephemerisId,
        }),
      (m) => this.renderMonth(m),
      (msg) => replaceChildren(this.monthGrid, h('p', { class: 'error' }, msg)),
    );
  }

  private async refreshCompare(): Promise<void> {
    const year = this.compareYear ?? this.selectedHijri?.year ?? this.estimateHijriYear();
    this.compareYearInput.value = String(year);
    this.followButton.hidden = this.compareYear === null;
    const s = this.ctx.store.get();
    await this.run(
      'compare',
      () => this.ctx.backend.calendar.compare({ hijriYear: year, visibility: this.visibilitySpec(), ephemerisId: s.ephemerisId }),
      (c) => this.renderCompare(c),
      (msg) => replaceChildren(this.compareTable, h('p', { class: 'error' }, msg)),
    );
  }

  /** The Hijri year of the selected date by the mean year length. */
  private estimateHijriYear(): number {
    const d = appDay(this.ctx.store);
    return Math.max(1, Math.floor(((d.year + (d.month - 1) / 12 - 622.54) * 365.2425) / 354.367) + 1);
  }

  // --- building -------------------------------------------------------------

  private build(): void {
    this.calendarSelect.addEventListener('change', () => {
      this.settings.calendarId = this.calendarSelect.value;
      this.changedSettings(true);
    });

    const primary = (['hijri', 'gregorian'] as const).map((p) =>
      h(
        'button',
        {
          type: 'button',
          class: 'segment',
          'aria-pressed': String(this.settings.primary === p),
          onclick: () => {
            this.settings.primary = p;
            primary.forEach((b, i) => b.setAttribute('aria-pressed', String(['hijri', 'gregorian'][i] === p)));
            this.view = null;
            saveSettings(this.settings);
            void this.refreshMonth();
          },
        },
        p === 'hijri' ? 'Hijri month' : 'Gregorian month',
      ),
    );
    const weekStart = h(
      'select',
      { 'aria-label': 'Week starts on' },
      ...[
        [6, 'Saturday'],
        [0, 'Sunday'],
        [1, 'Monday'],
      ].map(([v, l]) => h('option', { value: v as number, selected: this.settings.weekStart === v }, l as string)),
    );
    weekStart.addEventListener('change', () => {
      this.settings.weekStart = Number(weekStart.value);
      saveSettings(this.settings);
      void this.refreshMonth();
    });

    const nav = (label: string, title: string, f: () => void) => h('button', { type: 'button', class: 'button small', title, onclick: f }, label);
    const step = (d: number) => () => {
      const cur = this.lastMonth?.month;
      if (!cur) return;
      this.view = d < 0 ? this.lastMonth!.prev : this.lastMonth!.next;
      void this.refreshMonth();
    };

    // Converter.
    const onCivil = debounce(() => void this.convertCivil(), 200);
    const onHijri = debounce(() => void this.convertHijri(), 200);
    for (const el of [this.civilDay, this.civilMonth, this.civilYear]) el.addEventListener('input', onCivil);
    for (const el of [this.hijriDay, this.hijriMonth, this.hijriYear]) el.addEventListener('input', onHijri);

    // Comparison year.
    const setYear = (y: number) => {
      this.compareYear = Math.min(9999, Math.max(1, y));
      void this.refreshCompare();
    };
    this.compareYearInput.addEventListener('change', () => setYear(Math.round(this.compareYearInput.valueAsNumber)));
    this.followButton.addEventListener('click', () => {
      this.compareYear = null;
      void this.refreshCompare();
    });

    replaceChildren(
      this.el,
      h('h2', {}, 'Hijri calendar ', arabic('التقويم الهجري', 'arabic heading-ar')),
      h('div', { class: 'toolbar cal-toolbar' }, h('label', { class: 'inline' }, 'Calendar ', this.calendarSelect), this.status),
      this.visibilityBox,
      this.calendarInfo,
      h(
        'p',
        { class: 'banner sunset-note' },
        h('strong', {}, 'The Islamic day begins at sunset. '),
        'As in almanacs and official calendars, each Hijri date is shown on the civil day (midnight to midnight) that begins after its first sunset: the evening before already belongs to it.',
      ),
      this.selectedLine,
      h('h3', {}, 'Month view'),
      h(
        'div',
        { class: 'toolbar cal-toolbar' },
        h('span', { class: 'segmented' }, ...primary),
        h('label', { class: 'inline' }, 'Week starts ', weekStart),
        h(
          'span',
          { class: 'nav' },
          nav('« Prev', 'Previous month', step(-1)),
          nav('Today', 'Show the current month', () => {
            this.view = null;
            void this.showDay(todayLocal(tzOf(this.ctx.store.get())));
          }),
          nav('Selected', "Show the selected date's month", () => {
            this.view = null;
            void this.refreshMonth();
          }),
          nav('Next »', 'Next month', step(1)),
        ),
      ),
      this.monthTitle,
      this.monthGrid,
      h('h3', {}, 'Date converter'),
      h(
        'div',
        { class: 'converter' },
        h('span', { class: 'conv-label' }, 'Gregorian'),
        h('span', { class: 'conv-inputs' }, this.civilDay, this.civilMonth, this.civilYear),
        this.converterOut.civil,
        h('span', { class: 'conv-label' }, 'Hijri'),
        h('span', { class: 'conv-inputs' }, this.hijriDay, this.hijriMonth, this.hijriYear, h('span', { class: 'muted' }, 'AH')),
        this.converterOut.hijri,
      ),
      h('p', {}, h('button', { type: 'button', class: 'button small', onclick: () => this.fillConverterFromSelected() }, 'Fill from selected date')),
      h('h3', {}, 'Month starts under every calendar ', this.compareTitle),
      h(
        'div',
        { class: 'toolbar cal-toolbar' },
        h(
          'span',
          { class: 'stepper' },
          h('button', { type: 'button', class: 'button small', title: 'Previous year', onclick: () => setYear(Math.round(this.compareYearInput.valueAsNumber) - 1) }, '«'),
          this.compareYearInput,
          h('span', { class: 'muted' }, 'AH'),
          h('button', { type: 'button', class: 'button small', title: 'Next year', onclick: () => setYear(Math.round(this.compareYearInput.valueAsNumber) + 1) }, '»'),
        ),
        this.followButton,
      ),
      this.compareTable,
    );
  }

  /** Show the month containing `date` without changing the app date. */
  private async showDay(date: CivilDate): Promise<void> {
    const s = this.ctx.store.get();
    await this.run(
      'month',
      () =>
        this.ctx.backend.calendar.month({
          calendar: this.spec(),
          primary: this.settings.primary,
          date,
          weekStart: this.settings.weekStart,
          ephemerisId: s.ephemerisId,
        }),
      (m) => {
        this.view = m.month;
        this.renderMonth(m);
      },
      (msg) => replaceChildren(this.monthGrid, h('p', { class: 'error' }, msg)),
    );
  }

  private fillSelectors(): void {
    const c = this.catalog!;
    const groups: [string, (f: string) => boolean][] = [
      ['Umm al-Qura', (f) => f === 'umm-al-qura'],
      ['Computed with the ephemeris', (f) => f === 'rule' || f === 'visibility'],
      ['Arithmetical (tabular)', (f) => f === 'tabular'],
    ];
    replaceChildren(
      this.calendarSelect,
      ...groups.map(([label, test]) =>
        h('optgroup', { label }, ...c.calendars.filter((d) => test(d.family)).map((d) => h('option', { value: d.id }, d.id === 'visibility' ? 'Crescent visibility (configurable)' : d.name))),
      ),
    );
    if (!c.calendars.some((d) => d.id === this.settings.calendarId)) this.settings.calendarId = c.defaultId;
    this.calendarSelect.value = this.settings.calendarId;
    replaceChildren(
      this.hijriMonth,
      ...c.hijriMonths.map((m) => h('option', { value: m.number }, `${m.number}. ${m.name} — ${m.arabic}`)),
    );
    this.buildVisibilityBox();
  }

  private buildVisibilityBox(): void {
    const v = this.settings.visibility;
    const criteria = this.ctx.criteria;
    const criterion = h('select', { 'aria-label': 'Criterion' }, ...criteria.map((c) => h('option', { value: c.id, selected: c.id === v.criterionId }, c.name)));
    const zone = h('select', { 'aria-label': 'Threshold zone' });
    const fillZones = () => {
      const c = criteria.find((x) => x.id === v.criterionId) ?? criteria[0];
      if (!c) return;
      if (v.maxZone >= c.zones.length - 1) v.maxZone = Math.max(0, Math.min(1, c.zones.length - 2));
      replaceChildren(
        zone,
        ...c.zones.slice(0, -1).map((z, i) => h('option', { value: i, selected: i === v.maxZone }, `${z.code} or better — ${z.description}`)),
      );
    };
    fillZones();
    const observers = h(
      'select',
      { 'aria-label': 'Where' },
      h('option', { value: 'location' }, 'At the app location'),
      h('option', { value: 'mecca' }, 'At Mecca'),
      ...(this.catalog?.regions ?? []).map((r) => h('option', { value: r.id }, `Anywhere: ${r.name}`)),
    );
    observers.value = v.observers;
    const grid = h('select', { 'aria-label': 'Region grid' }, ...[2, 5, 10].map((g) => h('option', { value: g, selected: g === v.gridStepDeg }, `${g}° grid`)));
    const gridLabel = h('label', { class: 'inline' }, 'Grid ', grid);
    const sync = () => {
      gridLabel.hidden = v.observers === 'location' || v.observers === 'mecca';
    };
    sync();
    criterion.addEventListener('change', () => {
      v.criterionId = criterion.value;
      fillZones();
      this.changedSettings(true);
    });
    zone.addEventListener('change', () => {
      v.maxZone = Number(zone.value);
      this.changedSettings(true);
    });
    observers.addEventListener('change', () => {
      v.observers = observers.value;
      sync();
      this.changedSettings(true);
    });
    grid.addEventListener('change', () => {
      v.gridStepDeg = Number(grid.value);
      this.changedSettings(true);
    });
    replaceChildren(
      this.visibilityBox,
      h('span', { class: 'vis-label' }, 'Visibility calendar:'),
      h('label', { class: 'inline' }, 'Criterion ', criterion),
      h('label', { class: 'inline' }, 'Seen at ', zone),
      h('label', { class: 'inline' }, observers),
      gridLabel,
    );
  }

  private changedSettings(refetchName: boolean): void {
    saveSettings(this.settings);
    this.view = null;
    if (refetchName) {
      this.catalog = null;
    }
    void this.refreshAll();
  }

  // --- rendering ------------------------------------------------------------

  private monthNames(n: number) {
    return this.catalog?.hijriMonths[n - 1] ?? { number: n, name: String(n), ascii: String(n), arabic: '' };
  }

  private renderInfo(): void {
    const d = this.description();
    this.visibilityBox.hidden = this.settings.calendarId !== 'visibility';
    if (!d) {
      replaceChildren(this.calendarInfo);
      return;
    }
    replaceChildren(
      this.calendarInfo,
      h('strong', {}, d.name),
      ` — ${d.description}`,
      d.firstYear !== null && d.lastYear !== null ? ` Covers ${d.firstYear}–${d.lastYear} AH.` : '',
      d.reference ? h('span', { class: 'muted' }, ` Source: ${d.reference}.`) : '',
      d.attribution ? h('span', { class: 'muted' }, ` ${d.attribution}`) : '',
    );
  }

  private hijriLong(hd: HijriYmd): HTMLElement {
    const m = this.monthNames(hd.month);
    return h(
      'span',
      { class: 'hijri-long' },
      h('strong', {}, `${hd.day} ${m.name} ${hd.year} AH`),
      ' ',
      arabic(`${ar(hd.day)} ${m.arabic} ${ar(hd.year)} هـ`),
    );
  }

  private renderSelected(date: CivilDate, r: CalendarConversion): void {
    const wd = r.day ? this.catalog?.weekdays[r.day.weekday] : undefined;
    replaceChildren(
      this.selectedLine,
      'Selected date: ',
      r.day?.hijri ? this.hijriLong(r.day.hijri) : h('span', { class: 'muted' }, r.message ?? 'outside this calendar'),
      ` — ${wd?.english ?? ''} ${longDate(date)} (local)`,
      wd ? arabic(` ${wd.arabic}`, 'arabic muted') : '',
    );
  }

  private lastMonth: CalendarMonth | null = null;

  private renderMonth(m: CalendarMonth): void {
    this.lastMonth = m;
    const hijri = m.primary === 'hijri';
    const names = this.monthNames(m.month.month);
    const title = hijri
      ? h('span', {}, `${names.name} ${m.month.year} AH `, arabic(`${names.arabic} ${ar(m.month.year)} هـ`, 'arabic title-ar'))
      : h('span', {}, `${MONTH_NAMES[m.month.month - 1]} ${m.month.year}`);
    const sub = m.otherMonths
      .map((o) => (hijri ? `${MONTH_NAMES[o.month - 1]} ${o.year}` : `${this.monthNames(o.month).name} ${o.year}`))
      .join(' – ');
    const subAr = hijri ? '' : m.otherMonths.map((o) => `${this.monthNames(o.month).arabic} ${ar(o.year)}`).join(' – ');
    replaceChildren(this.monthTitle, h('div', { class: 'month-name' }, title), h('div', { class: 'muted' }, sub, subAr ? arabic(subAr, 'arabic sub-ar') : ''));

    if (m.weeks.length === 0) {
      replaceChildren(this.monthGrid, h('p', { class: 'muted' }, m.message ?? 'No days to show.'));
      return;
    }
    const selected = appDay(this.ctx.store);
    const today = todayLocal(tzOf(this.ctx.store.get()));
    const wds = this.catalog?.weekdays ?? [];
    const cell = (d: CalendarDay | null) => {
      if (!d) return h('td', { class: 'empty' });
      const hd = d.hijri;
      const first = hd?.day === 1;
      const main = hijri ? (hd ? String(hd.day) : '?') : String(d.civil.day);
      const mainAr = hijri && hd ? ar(hd.day) : '';
      const secondary = hijri
        ? `${d.civil.day} ${MONTH_NAMES[d.civil.month - 1]!.slice(0, 3)}${d.civil.day === 1 || d === m.weeks.flat().find(Boolean) ? ` ${d.civil.year}` : ''}`
        : hd
          ? `${hd.day} ${first ? this.monthNames(hd.month).name : HIJRI_ABBR[hd.month - 1]}`
          : '—';
      const classes = ['day', sameDay(d.civil, selected) ? 'selected' : '', sameDay(d.civil, today) ? 'today' : '', first ? 'month-first' : '', d.weekday === 5 ? 'friday' : '']
        .filter(Boolean)
        .join(' ');
      const wd = wds[d.weekday];
      return h(
        'td',
        { class: classes },
        h(
          'button',
          {
            type: 'button',
            title: `${wd?.english ?? ''} ${longDate(d.civil)}${hd ? ` — ${hd.day} ${this.monthNames(hd.month).name} ${hd.year} AH` : ''}\nClick to set the app date`,
            onclick: () => setAppDay(this.ctx.store, d.civil),
          },
          h('span', { class: 'day-main' }, main, mainAr ? arabic(mainAr, 'arabic day-ar') : ''),
          h('span', { class: 'day-sub' }, secondary),
          !hijri && first && hd ? arabic(this.monthNames(hd.month).arabic, 'arabic day-month-ar') : '',
        ),
      );
    };
    replaceChildren(
      this.monthGrid,
      h(
        'table',
        { class: 'month-grid' },
        h('thead', {}, h('tr', {}, ...m.columns.map((c) => h('th', {}, wds[c]?.english.slice(0, 3) ?? '', arabic(wds[c]?.arabic ?? '', 'arabic th-ar'))))),
        h('tbody', {}, ...m.weeks.map((w) => h('tr', {}, ...w.map(cell)))),
      ),
      m.message ? h('p', { class: 'hint' }, m.message) : '',
    );
  }

  // --- converter ------------------------------------------------------------

  private fillConverterFromSelected(): void {
    const d = appDay(this.ctx.store);
    this.civilDay.value = String(d.day);
    this.civilMonth.value = String(d.month);
    this.civilYear.value = String(d.year);
    void this.convertCivil(true);
  }

  private setButton(day: CalendarDay) {
    return h('button', { type: 'button', class: 'button small', onclick: () => setAppDay(this.ctx.store, day.civil) }, 'Set date');
  }

  private async convertCivil(fillHijri = false): Promise<void> {
    const civil = { year: Math.round(this.civilYear.valueAsNumber), month: Number(this.civilMonth.value), day: Math.round(this.civilDay.valueAsNumber) };
    if (![civil.year, civil.month, civil.day].every(Number.isFinite)) return;
    const s = this.ctx.store.get();
    await this.run(
      'civil',
      () => this.ctx.backend.calendar.convert({ calendar: this.spec(), civil, ephemerisId: s.ephemerisId }),
      (r) => {
        const d = r.day;
        replaceChildren(
          this.converterOut.civil,
          '→ ',
          d?.hijri ? this.hijriLong(d.hijri) : h('span', { class: 'warn' }, r.message ?? 'No such date.'),
          d ? h('span', { class: 'muted' }, ` (${this.catalog?.weekdays[d.weekday]?.english ?? ''}) `) : '',
          d ? this.setButton(d) : '',
        );
        if (fillHijri && d?.hijri) {
          this.hijriDay.value = String(d.hijri.day);
          this.hijriMonth.value = String(d.hijri.month);
          this.hijriYear.value = String(d.hijri.year);
          void this.convertHijri();
        }
      },
      (msg) => replaceChildren(this.converterOut.civil, h('span', { class: 'error' }, msg)),
    );
  }

  private async convertHijri(): Promise<void> {
    const hijri = { year: Math.round(this.hijriYear.valueAsNumber), month: Number(this.hijriMonth.value), day: Math.round(this.hijriDay.valueAsNumber) };
    if (![hijri.year, hijri.month, hijri.day].every(Number.isFinite)) return;
    const s = this.ctx.store.get();
    await this.run(
      'hijri',
      () => this.ctx.backend.calendar.convert({ calendar: this.spec(), hijri, ephemerisId: s.ephemerisId }),
      (r) => {
        const d = r.day;
        replaceChildren(
          this.converterOut.hijri,
          '→ ',
          d ? h('strong', {}, `${this.catalog?.weekdays[d.weekday]?.english ?? ''} ${longDate(d.civil)}`) : h('span', { class: 'warn' }, r.message ?? 'No such date.'),
          r.monthLength ? h('span', { class: 'muted' }, ` (month of ${r.monthLength} days) `) : ' ',
          d ? this.setButton(d) : '',
        );
      },
      (msg) => replaceChildren(this.converterOut.hijri, h('span', { class: 'error' }, msg)),
    );
  }

  // --- comparison -----------------------------------------------------------

  private renderCompare(c: CalendarComparison): void {
    this.compareTitle.textContent = `(${c.hijriYear} AH)`;
    const selected = appDay(this.ctx.store);
    const current = this.settings.calendarId;
    const wds = this.catalog?.weekdays ?? [];
    replaceChildren(
      this.compareTable,
      h(
        'table',
        { class: 'criteria-table compare-table' },
        h(
          'thead',
          {},
          h(
            'tr',
            {},
            h('th', {}, 'Month'),
            ...c.columns.map((col) =>
              h('th', { class: col.id === current ? 'current' : '', title: `${col.name}\n${col.description}` }, col.label, col.computed ? h('sup', { title: 'Computed with the selected ephemeris' }, '*') : ''),
            ),
          ),
        ),
        h(
          'tbody',
          {},
          ...c.rows.map((row) => {
            const names = this.monthNames(row.month);
            return h(
              'tr',
              { class: row.disagrees ? 'disagrees' : '' },
              h('th', { scope: 'row' }, `${row.month}. ${names.name}`, arabic(names.arabic, 'arabic month-ar')),
              ...row.starts.map((st, i) => {
                if (!st) return h('td', { class: 'muted' }, '—');
                const cls = [row.differs[i] ? 'differs' : '', c.columns[i]!.id === current ? 'current' : '', sameDay(st.civil, selected) ? 'selected' : ''].filter(Boolean).join(' ');
                const wd = wds[(((st.jdn + 1) % 7) + 7) % 7];
                return h(
                  'td',
                  { class: cls },
                  h(
                    'button',
                    {
                      type: 'button',
                      class: 'time-link',
                      title: `${c.columns[i]!.name}: 1 ${names.name} ${row.month === st.month ? c.hijriYear : st.year} AH = ${wd?.english ?? ''} ${longDate(st.civil)}${row.differs[i] ? ' (differs from the most common date)' : ''}\nClick to set the app date`,
                      onclick: () => setAppDay(this.ctx.store, st.civil),
                    },
                    shortDate(st.civil).replace(/ \d+$/, ''),
                    h('span', { class: 'muted wd' }, ` ${wd?.english.slice(0, 3) ?? ''}`),
                  ),
                );
              }),
            );
          }),
        ),
      ),
      h(
        'p',
        { class: 'hint' },
        'Each cell is the civil date of the 1st of the month. Highlighted cells differ from the most common date in their row; ',
        h('sup', {}, '*'),
        ' computed with the selected ephemeris. Click a date to select it.',
      ),
    );
  }
}
