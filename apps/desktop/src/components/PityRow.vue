<script setup lang="ts">
import { computed } from 'vue';

import type { CharacterProfile, FiveStarEntry } from '../types';

const props = defineProps<{
  entry: FiveStarEntry;
  maxPity: number;
  profile?: CharacterProfile | null;
}>();

// 与截图口径一致：≤40 绿，41–73 黄，≥74 红。
const tone = computed(() => {
  if (props.entry.pity >= 74) {
    return 'high';
  }
  if (props.entry.pity >= 41) {
    return 'mid';
  }
  return 'low';
});

const width = computed(() => {
  const ratio = props.entry.pity / props.maxPity;
  return `${Math.min(100, Math.max(6, ratio * 100))}%`;
});

const tag = computed(() => {
  if (props.entry.is_up === true) {
    return { text: 'UP', tone: 'up' };
  }
  if (props.entry.is_up === false) {
    return { text: '歪', tone: 'off' };
  }
  return { text: '常驻', tone: 'plain' };
});

const dateText = computed(() => props.entry.time.replace('T', ' ').slice(0, 10));

// 元素仅用于兜底圆牌配色；有头像时以头像为准。
const element = computed(() => props.profile?.element ?? 'unknown');
const avatar = computed(() => props.profile?.avatar ?? null);
</script>

<template>
  <div class="pity-row">
    <div class="avatar-slot">
      <img v-if="avatar" class="avatar-img" :src="avatar" :alt="entry.name" />
      <!-- 没拿到头像时用元素色圆牌 + 菱形符号，不用汉字占位 -->
      <div v-else class="avatar-fallback" :class="`el-${element}`">◆</div>
    </div>
    <div class="bar-wrap">
      <div class="bar" :class="`bar--${tone}`" :style="{ width }">
        <span class="bar-text">{{ entry.pity }}抽</span>
      </div>
      <span class="item-name">{{ entry.name }}</span>
    </div>
    <span v-if="entry.guaranteed" class="chip chip--guaranteed fx-shine" title="大保底">保</span>
    <span
      v-if="entry.captured_radiance"
      class="chip chip--radiance fx-shine fx-twinkle"
      title="捕获明光（推定：连续 3 次小保底歪后触发，仅角色池）"
      >光</span
    >
    <span class="tag" :class="[`tag--${tag.tone}`, { 'fx-shine': tag.tone === 'up' }]">{{
      tag.text
    }}</span>
    <span class="date">{{ dateText }}</span>
  </div>
</template>

<style scoped>
.pity-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 0;
}

.avatar-slot {
  flex: 0 0 auto;
  width: 30px;
  height: 30px;
}

.avatar-img {
  width: 30px;
  height: 30px;
  border-radius: 7px;
  object-fit: cover;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(232, 197, 106, 0.34);
}

.avatar-fallback {
  width: 30px;
  height: 30px;
  border-radius: 7px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  color: rgba(18, 16, 28, 0.55);
  border: 1px solid rgba(255, 255, 255, 0.18);
}

/* 元素配色（原神元素色） */
.el-anemo {
  background: linear-gradient(160deg, #a9e5cf, #74c2a8);
}
.el-geo {
  background: linear-gradient(160deg, #f3d99a, #d9ab55);
}
.el-electro {
  background: linear-gradient(160deg, #d5bcf5, #a97fe0);
}
.el-dendro {
  background: linear-gradient(160deg, #cfe89a, #93c14a);
}
.el-hydro {
  background: linear-gradient(160deg, #a9d8f5, #4fa8e0);
}
.el-pyro {
  background: linear-gradient(160deg, #f7b9a4, #e06a48);
}
.el-cryo {
  background: linear-gradient(160deg, #d6f2f8, #8fd6e8);
}
.el-unknown {
  background: linear-gradient(160deg, #cfcae0, #8b86a0);
}

.bar-wrap {
  position: relative;
  flex: 1;
  min-width: 0;
  height: 26px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.05);
  overflow: hidden;
}

.bar {
  height: 100%;
  border-radius: 6px;
  display: flex;
  align-items: center;
  padding-left: 8px;
  transition: width 0.25s ease;
}

.bar--low {
  background: linear-gradient(90deg, #2f8f5d, #58c98a);
}

.bar--mid {
  background: linear-gradient(90deg, #a57a22, #dcb04f);
}

.bar--high {
  background: linear-gradient(90deg, #8f2f3d, #cf5563);
}

.bar-text {
  font-size: 11.5px;
  font-weight: 600;
  color: #fff;
  white-space: nowrap;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
}

.item-name {
  position: absolute;
  top: 50%;
  left: 100%;
  transform: translate(-100%, -50%);
  margin-left: -8px;
  max-width: 46%;
  padding-right: 6px;
  font-size: 12.5px;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-align: right;
}

.tag {
  flex: 0 0 auto;
  width: 30px;
  text-align: center;
  font-size: 11px;
  padding: 1px 0;
  border-radius: 5px;
}

.chip {
  flex: 0 0 auto;
  width: 16px;
  text-align: center;
  font-size: 10px;
  line-height: 16px;
  border-radius: 4px;
  cursor: default;
}

/* 扫光配色：在各自标签上改 CSS 变量即可，不用动动画本身 */
.chip--guaranteed {
  --fx-light: rgba(255, 226, 150, 0.72);
  --fx-duration: 3s;
  color: #ffd98a;
  background: rgba(232, 197, 106, 0.16);
  border: 1px solid rgba(232, 197, 106, 0.4);
}

.chip--radiance {
  --fx-light: rgba(206, 184, 255, 0.8);
  --fx-duration: 2.8s;
  color: #cbb6ff;
  background: rgba(167, 139, 250, 0.18);
  border: 1px solid rgba(167, 139, 250, 0.45);
}

.tag--up {
  --fx-light: rgba(170, 255, 210, 0.45);
  --fx-duration: 4.2s;
  color: var(--up);
  background: rgba(111, 211, 155, 0.16);
}

.tag--off {
  color: var(--off);
  background: rgba(226, 103, 125, 0.16);
}

.tag--plain {
  color: var(--text-faint);
  background: rgba(255, 255, 255, 0.06);
}

.date {
  flex: 0 0 auto;
  width: 62px;
  text-align: right;
  font-size: 10.5px;
  color: var(--text-faint);
}
</style>
