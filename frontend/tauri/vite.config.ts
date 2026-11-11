import { defineConfig } from 'vite';

// Tauri expects a fixed dev-server port (tauri.conf.json `build.devUrl`).
export default defineConfig({
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // The About tab bundles the repository LICENSE file.
    fs: { allow: ['.', '../../LICENSE'] },
    // Don't rebuild when the Rust side changes.
    watch: { ignored: ['**/target/**', '**/crates/**'] },
  },
  build: {
    // WebView2 (Windows) and WebKitGTK/WKWebView versions Tauri 2 supports.
    target: ['es2022', 'chrome105', 'safari15'],
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
  },
});
