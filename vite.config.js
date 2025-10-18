import { defineConfig } from 'vite'

// Vite automatically handles modern ESM dependencies like livekit-client,
// so you don’t need extra plugins unless you use TypeScript, React, etc.

export default defineConfig({
  root: './js',
  build: {
    outDir: '../static',           // Output directory for static files
    sourcemap: true,          // Optional: include source maps
    minify: 'esbuild',        // Use esbuild for fast minification
    target: 'esnext',         // Modern JS output
    manifest: true,
    rollupOptions: {
      input: 'js/index.html',    // Entry HTML file
    },
  },
  server: {
    host: '0.0.0.0',          // Allow LAN/devices to access dev server
    port: 5173,               // Default port (change if needed)
  },
})