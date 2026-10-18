import vue from '@vitejs/plugin-vue'
import UI from '@codex-proxy/ui/vite'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'
export default defineConfig({ base: './', plugins: [vue(), tailwindcss(), UI(), { name: 'classic-script', transformIndexHtml: { order: 'post', handler: html => html.replace('type="module" crossorigin', 'defer') } }], build: { modulePreload: false, cssCodeSplit: false, rolldownOptions: { output: { format: 'iife', codeSplitting: false, entryFileNames: 'app.js', assetFileNames: 'app.[ext]' } } } })
