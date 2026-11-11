// Run the Tauri CLI against crates/mooncalc-tauri from this folder.
//
// The CLI looks for tauri.conf.json below the current directory, but the Rust
// crate lives elsewhere in the workspace, so point it there explicitly.
// Usage: `npm run tauri -- dev`, `npm run tauri -- build`, `npm run tauri -- icon <file>`.
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { delimiter, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const frontendDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const env = {
  ...process.env,
  TAURI_APP_PATH: resolve(frontendDir, '../../crates/mooncalc-tauri'),
  TAURI_FRONTEND_PATH: frontendDir,
};

// Rustup's default install directory is not always on PATH (e.g. on Windows
// shells started before installation); add it when present.
const home = process.env.USERPROFILE ?? process.env.HOME;
const cargoBin = home ? join(home, '.cargo', 'bin') : undefined;
if (cargoBin && existsSync(cargoBin)) {
  const key = Object.keys(env).find((k) => k.toUpperCase() === 'PATH') ?? 'PATH';
  env[key] = `${cargoBin}${delimiter}${env[key] ?? ''}`;
}

const cli = join(frontendDir, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
const result = spawnSync(process.execPath, [cli, ...process.argv.slice(2)], {
  cwd: frontendDir,
  env,
  stdio: 'inherit',
});
process.exit(result.status ?? 1);
