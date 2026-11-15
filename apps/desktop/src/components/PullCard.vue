<script setup lang="ts">
import { computed } from 'vue';

import type { CharacterProfile, FiveStarEntry } from '../types';

const props = defineProps<{
  entry: FiveStarEntry;
  maxPity: number;
  profile?: CharacterProfile | null;
}>();

const tone = computed(() => {
  if (props.entry.pity >= 74) {
    return 'high';
  }
  if (props.entry.pity >= 41) {
    return 'mid';
  }
  return 'low';
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
const element = computed(() => props.profile?.element ?? 'unknown');
const avatar = computed(() => props.profile?.avatar ?? null);
</script>

<template>
  <div class="card" :class="`card--${tone}`">
    <div class="card__head">
      <img v-if="avatar" class="card__avatar" :src="avatar" :alt="entry.name" />
      <div v-else class="card__avatar card__avatar--fallback" :class="`el-${element}`">◆</div>
      <span
        class="card__tag"
        :class="[`tag--${tag.tone}`, { 'fx-shine': tag.tone === 'up' }]"
        >{{ tag.text }}</span
      >
    </div>

    <div class="card__name" :title="entry.name">{{ entry.name }}</div>

    <div class="card__pity">
      <span class="card__pity-num">{{ entry.pity }}</span>
      <span class="card__pity-unit">抽</span>
      <div class="card__bar">
        <div
          class="card__bar-fill"
          :class="`fill--${tone}`"
          :style="{ width: `${Math.min(100, (entry.pity / maxPity) * 100)}%` }"
        />
      </div>
    </div>

    <div class="card__foot">
      <span v-if="entry.guaranteed" class="card__chip chip--guaranteed fx-shine" title="大保底"
        >保底</span
      >
      <span
        v-if="entry.captured_radiance"
        class="card__chip chip--radiance fx-shine fx-twinkle"
        title="捕获明光（推定：连续 3 次小保底歪后触发，仅角色池）"
        >明光</span
      >
      <span class="card__date">{{ dateText }}</span>
    </div>
  </div>
</template>

<style scoped>
.card {
  padding: 10px;
  border-radius: 10px;
  background: rgba(28, 22, 46, 0.72);
  border: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.card--high {
  border-color: rgba(207, 85, 99, 0.45);
}

.card--mid {
  border-color: rgba(220, 176, 79, 0.4);
}

.card__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
}

.card__avatar {
  width: 44px;
  height: 44px;
  border-radius: 9px;
  object-fit: cover;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(232, 197, 106, 0.3);
}

.card__avatar--fallback {
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  color: rgba(18, 16, 28, 0.55);
}

.card__tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 5px;
}

.tag--up {
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

.card__name {
  font-size: 12.5px;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.card__pity {
  display: flex;
  align-items: baseline;
  gap: 2px;
  font-size: 11px;
  color: var(--text-dim);
}

.card__pity-num {
  font-size: 18px;
  font-weight: 600;
  color: var(--gold-light);
  line-height: 1;
}

.card__bar {
  flex: 1;
  height: 4px;
  margin-left: 6px;
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.08);
  overflow: hidden;
  align-self: center;
}

.card__bar-fill {
  height: 100%;
  border-radius: 2px;
}

.fill--low {
  background: #58c98a;
}

.fill--mid {
  background: #dcb04f;
}

.fill--high {
  background: #cf5563;
}

.card__foot {
  display: flex;
  align-items: center;
  gap: 4px;
  min-height: 16px;
}

.card__chip {
  font-size: 9.5px;
  line-height: 14px;
  padding: 0 4px;
  border-radius: 4px;
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
}

.card__date {
  margin-left: auto;
  font-size: 10px;
  color: var(--text-faint);
}

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
</style>
