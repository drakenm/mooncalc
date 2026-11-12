// Tauri transport of the Sky, Trajectory and Moon operations
// (crates/mooncalc-tauri/src/commands/sky.rs).

import type { SkyApi } from './types-sky';

type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export function tauriSkyApi(call: Call): SkyApi {
  return {
    skyScene: (params) => call('sky_scene', { params }),
    skyStars: (params) => call('sky_stars', { params }),
    trajectoryDay: (params) => call('trajectory_day', { params }),
    moonView: (params) => call('moon_view', { params }),
    moonFeatures: () => call('moon_features'),
  };
}
