import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

// 端口固定 1420：tauri.conf.json 的 devUrl 指向它，改这里要同步改配置。
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: 'chrome110',
    outDir: 'dist',
    emptyOutDir: true,
  },
});
