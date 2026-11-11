// Wire types of the Hijri calendar service.
//
// Mirrors crates/mooncalc-app/src/calendar/dto.rs; keep the two in step.
// Civil days travel as a CivilDate (Julian before 15 October 1582, Gregorian
// after) and as a Julian Day Number `jdn` (the Julian Date at noon).

import type { CivilDate, Location } from './types';

/** "location" (the given location), "mecca", or a region preset id. */
export type VisibilityObservers = string;

export interface VisibilityCalendarSpec {
  criterionId: string;
  /** Seen in this zone or a better one (0 = most visible). */
  maxZone: number;
  observers: VisibilityObservers;
  location?: Location | null;
  locationName?: string | null;
  /** Region grid spacing, degrees (default 5). */
  gridStepDeg?: number | null;
}

export interface CalendarSpec {
  /** A built-in id, a rule id ("rule-umm-al-qura", ...) or "visibility". */
  id: string;
  visibility?: VisibilityCalendarSpec | null;
}

export interface CalendarListParams {
  visibility?: VisibilityCalendarSpec | null;
}

export type CalendarFamily = 'umm-al-qura' | 'tabular' | 'rule' | 'visibility';

export interface CalendarDescription {
  id: string;
  name: string;
  shortLabel: string;
  description: string;
  family: CalendarFamily;
  /** Month starts computed with the ephemeris, per Hijri year. */
  computed: boolean;
  cldrId: string | null;
  attribution: string | null;
  reference: string | null;
  firstYear: number | null;
  lastYear: number | null;
}

export interface RegionDescription {
  id: string;
  name: string;
}

export interface HijriMonthName {
  number: number;
  /** With diacritics, e.g. "Ramaḍān". */
  name: string;
  ascii: string;
  /** Arabic script, e.g. "رمضان". */
  arabic: string;
}

export interface WeekdayName {
  /** 0 = Sunday ... 6 = Saturday. */
  index: number;
  english: string;
  transliterated: string;
  arabic: string;
}

export interface CalendarCatalog {
  calendars: CalendarDescription[];
  defaultId: string;
  regions: RegionDescription[];
  hijriMonths: HijriMonthName[];
  weekdays: WeekdayName[];
  dayConvention: string;
}

export interface HijriYmd {
  year: number;
  month: number;
  day: number;
}

export interface CalendarDay {
  jdn: number;
  civil: CivilDate;
  hijri: HijriYmd | null;
  /** 0 = Sunday. */
  weekday: number;
}

export interface MissingYear {
  calendarId: string;
  hijriYear: number;
}

export interface MonthStartsParams {
  calendar: CalendarSpec;
  hijriYear: number;
  ephemerisId?: string | null;
}

export interface MonthStart {
  year: number;
  month: number;
  jdn: number;
  civil: CivilDate;
}

export interface MonthStartsYear {
  calendarId: string;
  calendarName: string;
  hijriYear: number;
  starts: MonthStart[];
}

export type PrimaryCalendar = 'hijri' | 'gregorian';

export interface MonthRef {
  year: number;
  month: number;
}

export interface CalendarMonthParams {
  calendar: CalendarSpec;
  primary?: PrimaryCalendar;
  month?: MonthRef | null;
  date?: CivilDate | null;
  /** 0 = Sunday ... 6 = Saturday (default Saturday). */
  weekStart?: number | null;
  ephemerisId?: string | null;
}

export interface CalendarMonth {
  calendarId: string;
  calendarName: string;
  primary: PrimaryCalendar;
  month: MonthRef;
  prev: MonthRef;
  next: MonthRef;
  /** Weekday of each column, 0 = Sunday. */
  columns: number[];
  weeks: (CalendarDay | null)[][];
  otherMonths: MonthRef[];
  message: string | null;
  missing: MissingYear[];
}

/** Give exactly one of `civil`, `hijri`. */
export interface CalendarConvertParams {
  calendar: CalendarSpec;
  civil?: CivilDate | null;
  hijri?: HijriYmd | null;
  ephemerisId?: string | null;
}

export interface CalendarConversion {
  day: CalendarDay | null;
  monthLength: number | null;
  message: string | null;
  missing: MissingYear[];
}

export interface CalendarCompareParams {
  hijriYear: number;
  visibility?: VisibilityCalendarSpec | null;
  builtinOnly?: boolean;
  ephemerisId?: string | null;
}

export interface CompareColumn {
  id: string;
  label: string;
  name: string;
  description: string;
  computed: boolean;
}

export interface CompareRow {
  month: number;
  starts: (MonthStart | null)[];
  consensus: number | null;
  disagrees: boolean;
  differs: boolean[];
}

export interface CalendarComparison {
  hijriYear: number;
  columns: CompareColumn[];
  rows: CompareRow[];
  missing: MissingYear[];
}

/** The calendar operations of a Backend (`backend.calendar`). */
export interface CalendarApi {
  list(params: CalendarListParams): Promise<CalendarCatalog>;
  /** Compute one Hijri year of a calendar (expensive for computed ones). */
  monthStarts(params: MonthStartsParams): Promise<MonthStartsYear>;
  /** Desktop transports compute missing years before answering. */
  month(params: CalendarMonthParams): Promise<CalendarMonth>;
  convert(params: CalendarConvertParams): Promise<CalendarConversion>;
  compare(params: CalendarCompareParams): Promise<CalendarComparison>;
}
