import { createApp, h } from 'vue'
import { ZConfigProvider } from '@codex-proxy/ui/config-provider'
import { zhHans } from '@codex-proxy/ui/locale'
import App from './App.vue'
import '@codex-proxy/ui/styles.css'
import './utilities.css'
import './style.css'
const theme = () => { document.documentElement.dataset.theme = window.codexProxyPlugin?.theme || 'light' }
theme()
window.addEventListener('codex-proxy-themechange', theme)
createApp({ render: () => h(ZConfigProvider, { locale: zhHans }, () => h(App)) }).mount('#app')
