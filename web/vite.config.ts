import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    outDir: '../assets/web',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        entryFileNames: 'app.js',
        chunkFileNames: '[name].js',
        assetFileNames: '[name][extname]',
      },
    },
  },
});
