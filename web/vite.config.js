import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  // Inline GLSL files as raw strings via ?raw imports
  assetsInclude: ['**/*.glsl'],
  server: {
    port: Number(process.env.PORT) || 5173,
    // Proxy WebSocket and API to running lopi sail server
    proxy: {
      '/ws': {
        target: 'ws://localhost:3000',
        ws: true
      },
      '/api': 'http://localhost:3000'
    }
  }
});
