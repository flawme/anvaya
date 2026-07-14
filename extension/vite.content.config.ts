import { defineConfig } from 'vite';
import { resolve } from 'node:path';
import { cpSync, mkdirSync, existsSync } from 'node:fs';

// Content scripts cannot be ES modules in Chrome or Firefox — they are loaded
// as classic scripts and any top-level `import` will throw a SyntaxError. So
// we build `content.js` separately as an IIFE with everything inlined: no
// chunks, no dynamic imports, no `import`/`export` statements at the top level.
// The result is a single self-contained classic script the browser can inject.
//
// Run after the main build: `vite build --config vite.content.config.ts`.
// `emptyOutDir: false` keeps the rest of `dist/` intact.
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

export default defineConfig({
  resolve: {
    alias: { '@': resolve(__dirname, 'src') },
  },
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: false,
    rollupOptions: {
      input: { content: resolve(__dirname, 'src/content/index.ts') },
      output: {
        entryFileNames: '[name].js',
        format: 'iife',
        inlineDynamicImports: true,
      },
    },
    minify: false,
  },
  plugins: [copyStaticAssets()],
});