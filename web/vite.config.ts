import { defineConfig, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// The Collector API target for the dev proxy. Override with COLLECTOR_URL.
const target = process.env.COLLECTOR_URL ?? 'http://127.0.0.1:8790';

// Dev-only mock of the Collector contract. It is loaded only in `--mode mock`
// and only by the dev server (configureServer never runs in `vite build`).
function mockCollector(): Plugin {
  return {
    name: 'halogen-mock-collector',
    apply: 'serve',
    async configureServer(server) {
      const { createMockHandler } = await import('./mock/collector.ts');
      server.middlewares.use(createMockHandler());
    },
  };
}

export default defineConfig(({ mode }) => {
  const mock = mode === 'mock';
  return {
    plugins: [svelte(), ...(mock ? [mockCollector()] : [])],
    server: {
      port: 5179,
      strictPort: true,
      host: '127.0.0.1',
      proxy: mock
        ? undefined
        : {
            '/api': { target, changeOrigin: true },
            '/metrics': { target, changeOrigin: true },
            '/healthz': { target, changeOrigin: true },
          },
    },
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      target: 'es2022',
      chunkSizeWarningLimit: 600,
    },
  };
});
