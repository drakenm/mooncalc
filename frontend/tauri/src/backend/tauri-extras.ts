// Tauri command bindings for the calendar and ephemeris-table services
// (crates/mooncalc-tauri/src/commands/{calendar,tables}.rs). TauriBackend
// passes in its `call`, so this module needs no Tauri import of its own.

import type { CalendarApi } from './types-calendar';
import type { TablesApi } from './types-tables';

export type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export function tauriCalendarApi(call: Call): CalendarApi {
  return {
    list: (params) => call('calendar_list', { params }),
    monthStarts: (params) => call('calendar_month_starts', { params }),
    month: (params) => call('calendar_month', { params }),
    convert: (params) => call('calendar_convert', { params }),
    compare: (params) => call('calendar_compare', { params }),
  };
}

export function tauriTablesApi(call: Call): TablesApi {
  return {
    ephemerisTable: (params) => call('ephemeris_table', { params }),
    riseSetMonth: (params) => call('rise_set_month', { params }),
    lunarPhases: (params) => call('lunar_phases', { params }),
    apsidesNodes: (params) => call('apsides_nodes', { params }),
    eclipses: (params) => call('eclipses', { params }),
    seasons: (params) => call('seasons', { params }),
  };
}
