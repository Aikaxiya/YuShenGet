import { invoke } from '@tauri-apps/api/core';
import { createPinia } from 'pinia';
import { createApp } from 'vue';

import App from './App.vue';
import { router } from './router';
import './styles.css';

/**
 * 启动阶段打点：写进应用数据目录的 frontend-ready.log。
 * 桌面端没有控制台，白屏时靠它判断「前端没跑起来」还是「跑起来了但渲染异常」。
 */
function mark(stage: string) {
  if (!('__TAURI_INTERNALS__' in window)) {
    return;
  }
  invoke('frontend_ready', { route: stage }).catch(() => {
    // 打点失败不影响主流程
  });
}

/**
 * 白屏是桌面端最难排查的故障：没有控制台，用户只会看到一片空白。
 * 这里把任何启动期异常直接画到页面上，至少能截图/描述出来。
 */
function showFatal(title: string, detail: string) {
  const host = document.getElementById('app');
  if (!host) {
    return;
  }
  host.innerHTML = '';
  const box = document.createElement('div');
  box.style.cssText =
    'padding:20px;font-family:Consolas,monospace;font-size:12px;line-height:1.7;color:#ffc9d2;background:#1b1020;height:100vh;box-sizing:border-box;overflow:auto;user-select:text';
  const heading = document.createElement('div');
  heading.style.cssText = 'font-size:14px;font-weight:600;margin-bottom:10px;color:#e8c56a';
  heading.textContent = title;
  const body = document.createElement('pre');
  body.style.cssText = 'white-space:pre-wrap;word-break:break-all;margin:0';
  body.textContent = detail;
  box.appendChild(heading);
  box.appendChild(body);
  host.appendChild(box);
  mark(`${title}: ${detail.slice(0, 200)}`);
}

window.addEventListener('error', (event) => {
  showFatal('界面启动出错', `${event.message}\n${event.filename}:${event.lineno}:${event.colno}`);
});

window.addEventListener('unhandledrejection', (event) => {
  const reason = event.reason;
  const detail = reason instanceof Error ? (reason.stack ?? reason.message) : String(reason);
  showFatal('界面出现未处理的异常', detail);
});

try {
  const app = createApp(App);
  // Vue 会吞掉渲染期异常，只打印到控制台；桌面端没有控制台，必须自己兜住
  app.config.errorHandler = (err, _instance, info) => {
    const detail =
      err instanceof Error ? `${info}\n${err.stack ?? err.message}` : `${info}\n${String(err)}`;
    showFatal('界面渲染出错', detail);
  };
  app.use(createPinia());
  app.use(router);

  let mounted = false;
  // 路由初始化卡住时，页面会永远停在占位文案上，这里主动暴露
  window.setTimeout(() => {
    if (!mounted) {
      showFatal('路由初始化超时', `3 秒内未完成首次导航\nlocation = ${window.location.href}`);
    }
  }, 3000);

  router.isReady().then(async () => {
    app.mount('#app');
    mounted = true;
    mark(`mounted:${window.location.hash || '#/'}`);

    // 调试入口：TEYVAT_START_ROUTE=/analysis/<uid> 时直接跳到指定页面
    if ('__TAURI_INTERNALS__' in window) {
      try {
        const route = await invoke<string | null>('start_route');
        if (route) {
          await router.push(route);
        }
      } catch {
        // 调试入口失败不影响正常流程
      }
    }
  });
} catch (err) {
  const detail = err instanceof Error ? (err.stack ?? err.message) : String(err);
  showFatal('应用初始化失败', detail);
}
