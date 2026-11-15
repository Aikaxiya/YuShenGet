<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import {
  formatDate,
  getStats,
  loadCharacterProfiles,
  maskUid,
  syncRecords,
} from '../api';
import MetricGrid from '../components/MetricGrid.vue';
import PityRow from '../components/PityRow.vue';
import PullCard from '../components/PullCard.vue';
import type { AccountStats, CharacterProfile, MetricItem, PoolKind } from '../types';

const props = defineProps<{ uid: string }>();
const router = useRouter();

const stats = ref<AccountStats | null>(null);
const loading = ref(true);
const syncing = ref(false);
const error = ref('');
const activePool = ref<PoolKind>('Character');
const profiles = ref<Record<string, CharacterProfile>>({});
const syncNote = ref('');
const viewMode = ref<'list' | 'card'>('list');

const VISIBLE_POOLS: PoolKind[] = ['Character', 'Weapon', 'Standard', 'Chronicled', 'Starlight'];

function allNames(): string[] {
  const data = stats.value;
  if (!data) {
    return [];
  }
  return Array.from(
    new Set(data.pools.flatMap((pool) => pool.entries.map((entry) => entry.name))),
  );
}

function applyProfiles(list: CharacterProfile[]) {
  const map: Record<string, CharacterProfile> = { ...profiles.value };
  for (const profile of list) {
    map[profile.name] = profile;
  }
  profiles.value = map;
}

/** 只用本地已有头像，秒回；绝不在这里联网。 */
async function loadLocalProfiles() {
  const names = allNames();
  if (names.length === 0) {
    return;
  }
  try {
    applyProfiles(await loadCharacterProfiles(names, false));
  } catch {
    // 头像缺失不影响主体数据
  }
}

/** 后台补齐缺失头像：下载完再刷新，不阻塞首屏。 */
async function fillMissingAvatars() {
  const names = allNames();
  const missing = names.filter((name) => !profiles.value[name]?.avatar);
  if (missing.length === 0) {
    return;
  }
  try {
    const filled = await loadCharacterProfiles(missing, true);
    if (filled.some((item) => item.avatar)) {
      applyProfiles(filled);
    }
  } catch {
    // 联网失败就保持元素圆牌
  }
}

async function load() {
  loading.value = true;
  error.value = '';
  try {
    stats.value = await getStats(props.uid);
    await loadLocalProfiles();
  } catch (err) {
    error.value = String(err);
  } finally {
    loading.value = false;
  }
  // 首屏已渲染，再慢慢补头像
  void fillMissingAvatars();
}

onMounted(load);
watch(() => props.uid, load);

const activeStats = computed(() => {
  if (!stats.value) {
    return null;
  }
  return stats.value.pools.find((pool) => pool.kind === activePool.value) ?? null;
});

function pct(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return '—';
  }
  return `${(value * 100).toFixed(1)}%`;
}

function num(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined) {
    return '—';
  }
  return value.toFixed(digits);
}

const metrics = computed<MetricItem[]>(() => {
  const data = stats.value;
  if (!data) {
    return [];
  }
  return [
    { label: '小保底不歪', value: pct(data.not_off_rate) },
    { label: '总抽数', value: String(data.total_pulls) },
    { label: 'UP角色平均', value: num(data.avg_up_character_pity), suffix: '抽' },
    { label: '限定武器平均', value: num(data.avg_limited_weapon_pity), suffix: '抽' },
    { label: '最多连UP', value: String(data.max_up_streak), tone: 'up' },
    { label: '最多连歪', value: String(data.max_off_streak), tone: 'off' },
    { label: '最久未抽卡', value: String(data.longest_gap_days), suffix: '天' },
    {
      label: '首发/复刻出金',
      value: data.first_run_ratio ? `${data.first_run_ratio[0]}:${data.first_run_ratio[1]}` : '—',
    },
  ];
});

const sortedEntries = computed(() => {
  const pool = activeStats.value;
  if (!pool) {
    return [];
  }
  return [...pool.entries].reverse();
});

async function resync() {
  if (syncing.value) {
    return;
  }
  syncing.value = true;
  error.value = '';
  try {
    const outcome = await syncRecords(null, true);
    await load();
    error.value = '';
    syncNote.value =
      outcome.added > 0 ? `新增 ${outcome.added} 条` : '没有新记录，本地已是最新';
  } catch (err) {
    error.value = String(err);
  } finally {
    syncing.value = false;
  }
}
</script>

<template>
  <div class="topbar">
    <button class="icon-btn" @click="router.push({ name: 'query' })">‹</button>
    <h1 class="topbar__title">祈愿数据分析</h1>
    <button class="icon-btn" title="从本地缓存重新同步" :disabled="syncing" @click="resync">
      ⟳
    </button>
  </div>

  <p v-if="error" class="error-text">{{ error }}</p>
  <p v-if="loading" class="muted loading">正在计算…</p>

  <template v-if="stats && !loading">
    <div class="account-card">
      <div class="avatar">{{ stats.rating.label.slice(0, 1) }}</div>
      <div class="account-info">
        <div class="account-uid">{{ maskUid(stats.uid) }}</div>
        <div class="account-sub faint">
          {{ formatDate(stats.first_time) }} 起 · {{ stats.total_pulls }} 抽
        </div>
      </div>
    </div>

    <div class="panel rating-panel">
      <p class="rating-intro">
        经分析，你从 {{ formatDate(stats.first_time) }} 至今的抽卡欧皇度为：
      </p>
      <div class="rating-label">{{ stats.rating.label }}</div>
      <div class="rating-score">
        {{ stats.rating.score.toFixed(1) }} 分
        <span v-if="!stats.rating.sample_sufficient" class="faint">（样本不足 100 抽）</span>
      </div>
      <div class="rating-breakdown faint">
        出金节奏 {{ stats.rating.pity_component.toFixed(1) }} × 50% ·
        不歪率 {{ stats.rating.luck_component.toFixed(1) }} × 50%
        <template v-if="stats.rating.penalty > 0">
          · 连歪扣分 -{{ stats.rating.penalty.toFixed(1) }}
        </template>
      </div>
    </div>

    <MetricGrid :items="metrics" />

    <div class="section-title">抽卡总结</div>
    <div class="cost-grid">
      <div class="panel cost-card">
        <div class="cost-label">平均每个 UP 角色花费</div>
        <div class="cost-value">
          {{ stats.cost_per_up_character ?? '—' }}<span class="cost-unit">原石</span>
        </div>
      </div>
      <div class="panel cost-card">
        <div class="cost-label">平均每把限定武器花费</div>
        <div class="cost-value">
          {{ stats.cost_per_limited_weapon ?? '—' }}<span class="cost-unit">原石</span>
        </div>
      </div>
    </div>

    <div class="section-title">出金历史</div>

    <div class="tabs">
      <button
        v-for="kind in VISIBLE_POOLS"
        :key="kind"
        class="tab"
        :class="activePool === kind ? ['tab--active', 'fx-shine'] : ''"
        @click="activePool = kind"
      >
        {{ stats.pools.find((pool) => pool.kind === kind)?.label ?? kind }}
      </button>
    </div>

    <div v-if="activeStats && activeStats.total > 0" class="pool-meta faint">
      <span>
        {{ activeStats.total }} 抽 · {{ activeStats.five_star_count }} 金 · 已垫
        {{ activeStats.current_pity }} 抽
        <template v-if="activeStats.avg_pity !== null">
          · 平均 {{ activeStats.avg_pity }} 抽/金
        </template>
      </span>
      <span class="view-toggle">
        <button
          class="view-btn"
          :class="viewMode === 'list' ? ['view-btn--active', 'fx-shine'] : ''"
          @click="viewMode = 'list'"
        >
          列表
        </button>
        <button
          class="view-btn"
          :class="viewMode === 'card' ? ['view-btn--active', 'fx-shine'] : ''"
          @click="viewMode = 'card'"
        >
          卡片
        </button>
      </span>
    </div>

    <div v-if="sortedEntries.length === 0" class="panel empty">
      该池暂无五星记录。
    </div>

    <div v-else-if="viewMode === 'card'" class="card-grid">
      <PullCard
        v-for="entry in sortedEntries"
        :key="entry.id"
        :entry="entry"
        :max-pity="activePool === 'Weapon' ? 80 : 90"
        :profile="profiles[entry.name]"
      />
    </div>

    <div v-else class="pity-list">
      <div class="pity-list__head">
        <span class="pad-note fx-shine">
          已垫 {{ activeStats?.current_pity ?? 0 }} 抽
        </span>
        <span class="faint">共 {{ sortedEntries.length }} 次出金</span>
      </div>
      <PityRow
        v-for="entry in sortedEntries"
        :key="entry.id"
        :entry="entry"
        :max-pity="activePool === 'Weapon' ? 80 : 90"
        :profile="profiles[entry.name]"
      />
    </div>

    <p v-if="syncNote" class="sync-note faint">{{ syncNote }}</p>

    <p v-if="stats.unknown_types.length > 0" class="footnote faint">
      检测到未识别的池类型 {{ stats.unknown_types.join(', ') }}，该部分记录未计入分池统计。
    </p>

  </template>
</template>

<style scoped>
.loading {
  padding: 28px 0;
  text-align: center;
  font-size: 13px;
}

.account-card {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 6px 2px 18px;
}

.avatar {
  width: 54px;
  height: 54px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 22px;
  font-weight: 600;
  color: #4a3410;
  background: linear-gradient(160deg, #ffeeb8, #d3ab52);
  box-shadow: 0 0 0 2px rgba(232, 197, 106, 0.22), 0 6px 16px rgba(0, 0, 0, 0.3);
}

.account-uid {
  font-size: 17px;
  font-weight: 500;
  letter-spacing: 1px;
}

.account-sub {
  font-size: 12px;
}

.rating-panel {
  padding: 16px 18px 14px;
  text-align: center;
}

.rating-intro {
  margin: 0;
  font-size: 12.5px;
  color: var(--text-dim);
}

/* 评级大字：金色渐变在文字内流动，外加一层缓慢呼吸的光晕 */
@keyframes fx-rating-sweep {
  0% {
    background-position: 130% 0;
  }
  100% {
    background-position: -30% 0;
  }
}

@keyframes fx-rating-glow {
  0%,
  100% {
    filter: drop-shadow(0 0 2px rgba(232, 197, 106, 0.15));
  }
  50% {
    filter: drop-shadow(0 0 12px rgba(232, 197, 106, 0.55));
  }
}

.rating-label {
  margin: 6px 0 2px;
  font-size: 30px;
  font-weight: 700;
  letter-spacing: 4px;
  background: linear-gradient(
    100deg,
    #b8913f 0%,
    var(--gold) 30%,
    #fffdf2 48%,
    var(--gold) 66%,
    #b8913f 100%
  );
  background-size: 220% 100%;
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  animation: fx-rating-sweep 5s linear infinite, fx-rating-glow 2.8s ease-in-out infinite;
}

@media (prefers-reduced-motion: reduce) {
  .rating-label {
    animation: none;
  }
}

.rating-score {
  font-size: 13px;
  color: var(--gold-light);
}

.rating-breakdown {
  margin-top: 6px;
  font-size: 11px;
}

.cost-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}

.cost-card {
  padding: 14px 12px;
  text-align: center;
}

.cost-label {
  font-size: 12px;
  color: var(--text-dim);
}

.cost-value {
  margin-top: 4px;
  font-size: 24px;
  font-weight: 600;
  color: var(--gold-light);
  letter-spacing: 1px;
}

.cost-unit {
  margin-left: 3px;
  font-size: 12px;
  font-weight: 400;
  color: var(--text-dim);
}

.tabs {
  display: flex;
  gap: 4px;
  padding: 4px;
  border-radius: 10px;
  background: rgba(20, 15, 34, 0.6);
}

.tab {
  flex: 1;
  padding: 7px 0;
  border-radius: 8px;
  font-size: 13px;
  color: var(--text-dim);
  transition: background 0.16s ease, color 0.16s ease;
}

/* 亮底标签用 overlay 混合，扫光不会把金字冲白 */
.tab--active {
  --fx-light: rgba(255, 255, 255, 0.45);
  --fx-blend: overlay;
  --fx-duration: 4s;
  color: #2a1f08;
  font-weight: 600;
  background: linear-gradient(180deg, #f2d98e, #c9a24c);
}

.pool-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin: 10px 2px 8px;
  font-size: 11.5px;
}

.view-toggle {
  display: inline-flex;
  border-radius: 7px;
  background: rgba(20, 15, 34, 0.6);
  padding: 2px;
}

.view-btn {
  padding: 2px 10px;
  border-radius: 6px;
  font-size: 11px;
  color: var(--text-dim);
}

.view-btn--active {
  --fx-light: rgba(255, 255, 255, 0.45);
  --fx-blend: overlay;
  --fx-duration: 4s;
  color: #2a1f08;
  background: linear-gradient(180deg, #f2d98e, #c9a24c);
}

.card-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}

.pity-list {
  padding: 8px 10px 12px;
  border-radius: var(--radius-lg);
  background: rgba(28, 22, 46, 0.6);
  border: 1px solid var(--border);
}

.pity-list__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 0 6px 2px;
  font-size: 11.5px;
}

.pad-note {
  --fx-light: rgba(255, 255, 255, 0.5);
  --fx-blend: overlay;
  --fx-duration: 3.8s;
  padding: 2px 8px;
  border-radius: 6px;
  color: #12321f;
  font-weight: 600;
  background: linear-gradient(180deg, #8fe3b4, #4cbf85);
}

.empty {
  padding: 26px;
  text-align: center;
  font-size: 13px;
  color: var(--text-faint);
}

.footnote {
  margin-top: 18px;
  font-size: 11.5px;
  text-align: center;
}

.sync-note {
  margin: 10px 2px 0;
  font-size: 11.5px;
}

</style>
