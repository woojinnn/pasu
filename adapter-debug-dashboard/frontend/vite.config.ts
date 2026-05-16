import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    // Proxy /api/* to the Rust backend so the frontend can fetch('/api/decode')
    // without thinking about CORS in dev. The Rust server defaults to :3000
    // but we run it on :8080 (port 3000 is taken on macOS by ControlCenter).
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
      },
    },
  },
})
