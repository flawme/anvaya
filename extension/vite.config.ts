import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { resolve } from 'node:path';
import { cpSync, mkdirSync, existsSync } from 'node:fs';

// Copy static extension assets (manifest + icons) into the dist/ root so the
// output directory can be loaded directly as an unpacked Chrome / Edge / Brave
// extension or as a temporary Firefox add-on.
//
// The single manifest.json carries BOTH `background.service_worker` (Chrome /
// Edge / Brave) and `background.scripts` (Firefox temporary installs, where
// service_worker is disabled). Per MDN, Firefox prefers `scripts` when both
// are present; Chrome 121+ ignores `scripts` and uses `service_worker`.
function copyStaticAssets() {
  return {
    name: 'anvaya-copy-static',
    closeBundle() {
      const outDir = resolve(__dirname, 'dist');
      if (!existsSync(outDir)) mkdirSync(outDir, { recursive: true });
      cpSync(resolve(__dirname, 'manifest.json'), resolve(outDir, 'manifest.json'));
      if (existsSync(resolve(__dirname, 'icons'))) {
        cpSync(resolve(__dirname, 'icons'), resolve(outDir, 'icons'), { recursive: true });
      }
    },
  };
}

// Vite is configured to emit one bundle per entrypoint so the resulting
// `dist/` can be loaded directly as an unpacked Chrome extension. The popup
// and options pages are HTML entries; the background service worker is a
// plain JS entry emitted as `background.js`.
export default defineConfig(({ mode }) => {
  const isExtension = mode !== 'web';

  return {
    plugins: [react(), copyStaticAssets()],
    resolve: {
      alias: { '@': resolve(__dirname, 'src') },
    },
    build: {
      // The service worker is a single JS file; keep the module graph flat.
      target: 'es2022',
      outDir: 'dist',
      emptyOutDir: true,
      rollupOptions: {
        input: {
          index: resolve(__dirname, 'index.html'),
          options: resolve(__dirname, 'options.html'),
          background: resolve(__dirname, 'src/background/index.ts'),
        },
        output: {
          entryFileNames: '[name].js',
          chunkFileNames: 'chunks/[name].js',
          assetFileNames: 'assets/[name][extname]',
        },
      },
    },
    define: {
      // Service workers have no `window`; keep code DOM-agnostic where needed.
      'process.env.NODE_ENV': JSON.stringify(process.env.NODE_ENV ?? 'production'),
    },
    server: {
      port: 5174,
      strictPort: true,
      hmr: { port: 5174 },
    },
  };
});