import { createRouter, createWebHashHistory } from 'vue-router';

import AnalysisView from './views/AnalysisView.vue';
import QueryView from './views/QueryView.vue';

// 用 hash 路由：Tauri 的 webview 直接加载本地文件，history 模式需要额外配置回退。
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'query', component: QueryView },
    { path: '/analysis/:uid', name: 'analysis', component: AnalysisView, props: true },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
});
