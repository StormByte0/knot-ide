import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath, URL } from 'node:url';

// Tauri 2 + Svelte 5 + Monaco spike Vite config.
// Key: NO vite-plugin-monaco-editor — it conflicts with @codingame/monaco-vscode-api.
// Monaco workers are loaded via ?worker imports in src/lib/editor/workers.ts.
export default defineConfig({
  plugins: [svelte()],

  // $lib path alias (matches tsconfig.json paths).
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL('./src/lib', import.meta.url)),
      // monaco-vscode-api uses deep subpath exports (./vscode/* → ./vscode/src/*)
      // that Vite's resolver doesn't handle. Map them explicitly.
      '@codingame/monaco-vscode-api/vscode': fileURLToPath(
        new URL('./node_modules/@codingame/monaco-vscode-api/vscode/src', import.meta.url),
      ),
    },
  },

  // Tauri uses a fixed port in dev; Vite must not conflict.
  clearScreen: false,
  worker: {
    // monaco-vscode-api's TextMate worker uses ES module imports (code
    // splitting), which requires 'es' format. The default 'iife' doesn't
    // support code splitting.
    format: 'es',
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Don't watch the Rust backend — tauri dev handles that.
      ignored: ['**/src-tauri/**'],
    },
  },

  // Tauri webview needs this for ESM.
  envPrefix: ['VITE_', 'TAURI_ENV_*'],

  build: {
    target: 'esnext',
    // Monaco + monaco-vscode-api produce a large bundle; chunking helps.
    chunkSizeWarningLimit: 4096,
    rollupOptions: {
      output: {
        manualChunks: {
          monaco: ['monaco-editor'],
          'monaco-vscode': [
            '@codingame/monaco-vscode-api',
            '@codingame/monaco-vscode-textmate-service-override',
            '@codingame/monaco-vscode-theme-service-override',
            '@codingame/monaco-vscode-languages-service-override',
            '@codingame/monaco-vscode-extensions-service-override',
            '@codingame/monaco-vscode-theme-defaults-default-extension',
          ],
          'lsp-client': ['monaco-languageclient', 'vscode-languageclient', 'vscode-jsonrpc'],
        },
      },
    },
  },
});
