import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3000,
    proxy: {
      '/1': {
        target: process.env.VITE_SEARCH_URL ?? 'http://localhost:8108',
        changeOrigin: true,
      },
    },
  },
})
