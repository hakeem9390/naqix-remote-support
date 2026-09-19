import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath } from 'node:url'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    // fileURLToPath, not .pathname: the repo path contains a space, which a
    // URL pathname percent-encodes into %20 and breaks resolution.
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  server: {
    port: 5180,
    proxy: {
      // Point at the MeshCentral instance during development so the browser
      // sees one origin and cookies/websockets behave as they will in prod.
      '/mesh': {
        target: process.env.MESH_ORIGIN ?? 'https://remote.naqix.example',
        changeOrigin: true,
        secure: true,
        ws: true,
        rewrite: (p) => p.replace(/^\/mesh/, ''),
      },
    },
  },
})
