// 浏览器预览用的假数据。只在没有 Tauri 运行时通过动态 import 加载，
// 不参与桌面端任何真实数据路径（也避免这段生成逻辑在启动时执行）。
import type { AccountRow, AccountStats, FiveStarEntry, PoolStats } from './types';

const STANDARD_CHARACTERS = ['迪卢克', '琴', '莫娜', '七七', '刻晴', '提纳里', '迪希雅'];
const UP_CHARACTERS = ['纳西妲', '芙宁娜', '雷电将军', '胡桃', '夜兰', '那维莱特', '仆人'];
const UP_WEAPONS = ['薙草之稻光', '静水流涌之辉', '最初的大魔术', '万世流涌大典'];
const STANDARD_WEAPONS = ['天空之刃', '天空之傲', '阿莫斯之弓', '狼的末路'];

function rng(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 0xffffffff;
  };
}

interface BuildOptions {
  totalPulls: number;
  upNames: string[];
  standardNames: string[];
  hasUp: boolean;
  itemType: string;
  generate: () => number;
  startDay: number;
}

function buildEntries(options: BuildOptions): FiveStarEntry[] {
  const { totalPulls, upNames, standardNames, hasUp, itemType, generate, startDay } = options;
  const entries: FiveStarEntry[] = [];
  let pity = 0;
  let pulls = 0;
  let index = 0;
  let day = startDay;

  // 终止条件必须只看已抽数 pulls，绝不能看 pity —— pity 会在出金时清零，
  // 用它当循环条件就是死循环（之前正是在这里把界面卡死的）。
  while (pulls < totalPulls) {
    pulls += 1;
    pity += 1;

    let fiveRate = 0.006;
    if (pity > 73) {
      fiveRate += (pity - 73) * 0.06;
    }
    if (generate() >= fiveRate && pity < 90) {
      continue;
    }

    const isUp = hasUp ? generate() > 0.45 : null;
    const pool = isUp === false ? standardNames : upNames;
    const name = pool[Math.floor(generate() * pool.length) % pool.length] ?? '某五星';

    index += 1;
    day += 3 + Math.floor(generate() * 9);
    // 必须由单一累加器算出时间，否则月份/日子会倒退，
    // 导致保底序列在界面上看起来是乱的。
    const stamp = new Date(Date.UTC(2023, 1, 13) + day * 86_400_000 + index * 3_600_000);
    entries.push({
      id: String(1700000000000 + index * 1000),
      name,
      item_type: itemType,
      time: `${stamp.getUTCFullYear()}-${String(stamp.getUTCMonth() + 1).padStart(2, '0')}-${String(
        stamp.getUTCDate(),
      ).padStart(2, '0')} 12:00:00`,
      pity,
      is_up: isUp,
      // 演示数据不推演保底状态，交给真实后端计算
      guaranteed: false,
      captured_radiance: false,
    });
    pity = 0;
  }

  entries.reverse();
  return entries;
}

function buildPool(
  kind: PoolStats['kind'],
  label: string,
  total: number,
  entries: FiveStarEntry[],
  hasUp: boolean,
): PoolStats {
  const fiveStars = entries.length;
  const upCount = entries.filter((entry) => entry.is_up === true).length;
  const offCount = entries.filter((entry) => entry.is_up === false).length;
  const judged = upCount + offCount;
  const avgUpPity = upCount > 0 ? Math.round((total / upCount) * 10) / 10 : null;

  let maxUp = 0;
  let maxOff = 0;
  let runUp = 0;
  let runOff = 0;
  for (const entry of entries) {
    if (entry.is_up === true) {
      runUp += 1;
      runOff = 0;
      maxUp = Math.max(maxUp, runUp);
    } else if (entry.is_up === false) {
      runOff += 1;
      runUp = 0;
      maxOff = Math.max(maxOff, runOff);
    }
  }

  return {
    kind,
    label,
    total,
    five_star_count: fiveStars,
    entries,
    avg_pity: fiveStars > 0 ? Math.round((total / fiveStars) * 10) / 10 : null,
    up_count: upCount,
    off_count: offCount,
    not_off_rate: judged > 0 ? Math.round((upCount / judged) * 1000) / 1000 : null,
    avg_up_pity: hasUp ? avgUpPity : null,
    max_up_streak: maxUp,
    max_off_streak: maxOff,
    current_pity: 11,
    cost_per_up: avgUpPity ? Math.round(avgUpPity * 160) : null,
  };
}

const generate = rng(20240213);

const characterEntries = buildEntries({
  totalPulls: 2100,
  upNames: UP_CHARACTERS,
  standardNames: STANDARD_CHARACTERS,
  hasUp: true,
  itemType: '角色',
  generate,
  startDay: 40,
});

const weaponEntries = buildEntries({
  totalPulls: 1500,
  upNames: UP_WEAPONS,
  standardNames: STANDARD_WEAPONS,
  hasUp: true,
  itemType: '武器',
  generate,
  startDay: 120,
});

const standardEntries = buildEntries({
  totalPulls: 600,
  upNames: [...STANDARD_CHARACTERS, ...STANDARD_WEAPONS],
  standardNames: [],
  hasUp: false,
  itemType: '角色',
  generate,
  startDay: 200,
});

const character = buildPool('Character', '角色', 2100, characterEntries, true);
const weapon = buildPool('Weapon', '武器', 1500, weaponEntries, true);
const standard = buildPool('Standard', '常驻', 600, standardEntries, false);

export const demoStats: AccountStats = {
  uid: '114709792',
  total_pulls: 4483,
  first_time: '2023-02-13 12:00:00',
  last_time: '2026-11-15 09:20:00',
  longest_gap_days: 19,
  pools: [
    character,
    weapon,
    standard,
    buildPool('Chronicled', '混池', 0, [], true),
    buildPool('Starlight', '千星奇域', 0, [], true),
    buildPool('Beginner', '新手', 20, [], false),
  ],
  rating: {
    score: 44.2,
    label: '平民',
    tier: 3,
    pity_component: 48.4,
    luck_component: 60.0,
    penalty: 8.0,
    sample_sufficient: true,
  },
  not_off_rate: 0.6,
  avg_up_character_pity: 96.7,
  avg_limited_weapon_pity: 60.9,
  max_up_streak: 5,
  max_off_streak: 3,
  first_run_ratio: null,
  cost_per_up_character: 15471,
  cost_per_limited_weapon: 9747,
  unknown_types: [],
};

export const demoAccounts: AccountRow[] = [
  {
    uid: '114709792',
    nickname: null,
    region: 'cn_gf01',
    game_biz: 'hk4e_cn',
    pull_count: 4483,
    first_time: '2023-02-13 12:00:00',
    last_time: '2026-11-15 09:20:00',
    last_sync_at: '2026-11-15 17:40:00',
  },
];
