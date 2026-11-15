<script setup lang="ts">
import type { MetricItem } from '../types';

defineProps<{ items: MetricItem[] }>();
</script>

<template>
  <div class="metric-grid">
    <div v-for="item in items" :key="item.label" class="metric-cell">
      <div class="metric-value" :class="`tone-${item.tone ?? 'normal'}`">
        {{ item.value }}<span v-if="item.suffix" class="metric-suffix">{{ item.suffix }}</span>
      </div>
      <div class="metric-label">{{ item.label }}</div>
    </div>
  </div>
</template>

<style scoped>
.metric-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 1px;
  padding: 4px 0;
  border-radius: var(--radius-lg);
  background: rgba(120, 100, 180, 0.12);
}

.metric-cell {
  padding: 12px 4px 10px;
  text-align: center;
  background: rgba(24, 19, 40, 0.55);
}

.metric-cell:nth-child(4n + 1) {
  border-top-left-radius: 12px;
}

.metric-cell:nth-child(4n) {
  border-top-right-radius: 12px;
}

.metric-cell:nth-last-child(-n + 4) {
  border-bottom-left-radius: 12px;
  border-bottom-right-radius: 12px;
}

.metric-value {
  font-size: 21px;
  font-weight: 600;
  letter-spacing: 0.5px;
  color: var(--gold-light);
  line-height: 1.25;
}

.metric-value.tone-up {
  color: var(--up);
}

.metric-value.tone-off {
  color: var(--off);
}

.metric-suffix {
  margin-left: 1px;
  font-size: 12px;
  font-weight: 400;
  color: var(--text-dim);
}

.metric-label {
  margin-top: 3px;
  font-size: 11.5px;
  color: var(--text-faint);
}
</style>
