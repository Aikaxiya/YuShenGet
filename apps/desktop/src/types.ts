export type PoolKind = 'Character' | 'Weapon' | 'Standard' | 'Chronicled' | 'Starlight' | 'Beginner';

export interface Rating {
  score: number;
  label: string;
  tier: number;
  pity_component: number;
  luck_component: number;
  penalty: number;
  sample_sufficient: boolean;
}

export interface FiveStarEntry {
  id: string;
  name: string;
  item_type: string;
  time: string;
  pity: number;
  is_up: boolean | null;
  /** 处于大保底状态（上一个五星歪了） */
  guaranteed: boolean;
  /** 判定为捕获明光（推定：仅角色池，连续 3 次小保底歪后下一次小保底出 UP） */
  captured_radiance: boolean;
}

export interface PoolStats {
  kind: PoolKind;
  label: string;
  total: number;
  five_star_count: number;
  entries: FiveStarEntry[];
  avg_pity: number | null;
  up_count: number;
  off_count: number;
  not_off_rate: number | null;
  avg_up_pity: number | null;
  max_up_streak: number;
  max_off_streak: number;
  current_pity: number;
  cost_per_up: number | null;
}

export interface AccountStats {
  uid: string;
  total_pulls: number;
  first_time: string | null;
  last_time: string | null;
  longest_gap_days: number;
  pools: PoolStats[];
  rating: Rating;
  not_off_rate: number | null;
  avg_up_character_pity: number | null;
  avg_limited_weapon_pity: number | null;
  max_up_streak: number;
  max_off_streak: number;
  first_run_ratio: [number, number] | null;
  cost_per_up_character: number | null;
  cost_per_limited_weapon: number | null;
  unknown_types: number[];
}

export interface AccountRow {
  uid: string;
  nickname: string | null;
  region: string;
  game_biz: string;
  pull_count: number;
  first_time: string | null;
  last_time: string | null;
  last_sync_at: string | null;
}

export interface LocalLink {
  gacha_type: number | null;
  score: number;
  cache_path: string;
  masked_authkey: string;
}

export interface SyncProgress {
  phase: string;
  pool: string;
  gacha_type: number;
  fetched: number;
}

export interface SyncOutcome {
  uid: string;
  fetched: number;
  added: number;
  total: number;
  /** 命中本地已有记录后提前停止（增量同步生效） */
  stopped_early: boolean;
}

export interface CharacterProfile {
  name: string;
  element: string;
  /** data:image/...;base64 形式；没有头像文件时为 null */
  avatar: string | null;
}

export interface MetricItem {
  label: string;
  value: string;
  suffix?: string;
  tone?: 'normal' | 'up' | 'off';
}
