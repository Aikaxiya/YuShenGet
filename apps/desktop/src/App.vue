<script setup lang="ts">
import { isDesktop } from './api';
</script>

<template>
  <div class="shell">
    <div v-if="!isDesktop" class="preview-badge">浏览器预览模式 · 数据为演示样本</div>
    <router-view v-slot="{ Component, route }">
      <transition name="fade" mode="out-in">
        <component :is="Component" v-if="Component" :key="route.path" />
        <!-- 路由未匹配时给一个用户可读的兜底，不显示路径等调试信息 -->
        <div v-else class="route-missing">页面暂时无法打开，请返回重试</div>
      </transition>
    </router-view>
  </div>
</template>

<style scoped>
.preview-badge {
  margin: 8px 0 -6px;
  padding: 4px 10px;
  align-self: flex-start;
  border-radius: 999px;
  font-size: 11px;
  color: var(--gold-light);
  background: rgba(232, 197, 106, 0.14);
  border: 1px solid rgba(232, 197, 106, 0.32);
}

.route-missing {
  padding: 40px 16px;
  text-align: center;
  font-size: 13px;
  line-height: 1.9;
  color: #ffc9d2;
}
</style>
