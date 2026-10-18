import { createApp } from 'vue'
import App from './App.vue'
import '@codex-proxy/ui/styles.css'
import './utilities.css'
import './style.css'
const theme = () => { document.documentElement.dataset.theme = window.codexProxyPlugin?.theme || 'light' }
theme()
window.addEventListener('codex-proxy-themechange', theme)
createApp(App).mount('#app')
