//! 统计内核：保底判定、歪/不歪、连击、欧皇度。
//!
//! 设计原则：
//! 1. 全部为纯函数，不读文件、不发请求，便于单测。
//! 2. 输入允许乱序，内部统一按 (time, id) 升序重排。
//! 3. 任何无法从记录推出的指标（例如首发/复刻比）一律返回 None，绝不用假数据填充。

use std::collections::HashSet;

use chrono::NaiveDateTime;
use serde::Serialize;

use crate::model::{PoolKind, PullRecord};
use crate::pools::{self, PRIMOGEM_PER_PULL};
use crate::rating::{evaluate, Rating, RatingInput};

/// 一次五星出金。
#[derive(Debug, Clone, Serialize)]
pub struct FiveStarEntry {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub time: NaiveDateTime,
    /// 距上一个五星的抽数（含本次）。
    pub pity: u32,
    /// Some(true)=UP，Some(false)=歪，None=该池无 UP 概念（常驻/新手）。
    pub is_up: Option<bool>,
    /// 本次出金是否处于大保底状态（上一个五星歪了，本次必为 UP）。
    pub guaranteed: bool,
    /// 是否判定为「捕获明光」（推定）。
    ///
    /// 官方规则（5.0 公告 2025-02-10 补充）：连续三次「第二次获得五星才出 UP」
    /// （即连续三次小保底歪、靠大保底才出 UP）后，下一次出五星**必定**触发明光。
    /// 换算成判定条件：角色活动祈愿、处于小保底、连歪计数 ≥3、本次出 UP。
    ///
    /// 注意两点局限：
    /// - 非零连歪时明光还有 0.018% 的基础触发概率，记录里没有明光字段，无法识别，
    ///   这里只标记「必定触发」的那一次；
    /// - 大保底出的 UP 不参与计数，小保底直接出 UP 会把连续计数打断归零。
    pub captured_radiance: bool,
}

/// 单个池的统计结果。
#[derive(Debug, Clone, Serialize)]
pub struct PoolStats {
    pub kind: PoolKind,
    pub label: String,
    pub total: u32,
    pub five_star_count: u32,
    /// 按时间升序，UI 展示时自行反转。
    pub entries: Vec<FiveStarEntry>,
    /// 每金平均抽数。
    pub avg_pity: Option<f64>,
    pub up_count: u32,
    pub off_count: u32,
    /// 小保底不歪率。
    pub not_off_rate: Option<f64>,
    /// 抽数 ÷ UP 数量。
    pub avg_up_pity: Option<f64>,
    pub max_up_streak: u32,
    pub max_off_streak: u32,
    /// 当前已垫抽数（距下一个五星）。
    pub current_pity: u32,
    /// 平均每个 UP 花费原石。
    pub cost_per_up: Option<u64>,
}

/// 账号级统计结果，直接序列化给前端。
#[derive(Debug, Clone, Serialize)]
pub struct AccountStats {
    pub uid: String,
    pub total_pulls: u32,
    pub first_time: Option<NaiveDateTime>,
    pub last_time: Option<NaiveDateTime>,
    /// 相邻两抽之间的最长间隔（天）。
    pub longest_gap_days: u32,
    pub pools: Vec<PoolStats>,
    pub rating: Rating,
    pub not_off_rate: Option<f64>,
    pub avg_up_character_pity: Option<f64>,
    pub avg_limited_weapon_pity: Option<f64>,
    pub max_up_streak: u32,
    pub max_off_streak: u32,
    /// 首发/复刻出金比。无法从祈愿记录推导，需要卡池排期表，暂返回 None。
    pub first_run_ratio: Option<(u32, u32)>,
    pub cost_per_up_character: Option<u64>,
    pub cost_per_limited_weapon: Option<u64>,
    /// 未被识别的 gacha_type，用于暴露「千星奇域」这类新池缺口，而不是静默丢弃。
    pub unknown_types: Vec<u32>,
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn id_ord(record: &PullRecord) -> u64 {
    record.id.parse::<u64>().unwrap_or(0)
}

/// 从账号自身的「常驻池」记录里学习常驻五星名单。
///
/// 常驻池（gacha_type=200）出过的五星，一定属于常驻池五星。
/// 这样即便游戏新版本把某个限定角色放进常驻池、而代码里的静态表还没更新，
/// 也不会把它误判成 UP。
pub fn learned_standard_names(records: &[PullRecord]) -> HashSet<String> {
    records
        .iter()
        .filter(|record| {
            record.is_five_star()
                && pools::resolve_pool(record.gacha_type) == Some(PoolKind::Standard)
        })
        .map(|record| record.item_name.clone())
        .collect()
}

/// 对单个池做统计。传入的 records 可以乱序。
pub fn analyze_pool(kind: PoolKind, label: &str, records: &[PullRecord]) -> PoolStats {
    analyze_pool_with(kind, label, records, &HashSet::new())
}

/// 带「已学习常驻名单」的池统计。
pub fn analyze_pool_with(
    kind: PoolKind,
    label: &str,
    records: &[PullRecord],
    learned_standard: &HashSet<String>,
) -> PoolStats {
    let mut sorted: Vec<&PullRecord> = records.iter().collect();
    sorted.sort_by_key(|r| (r.time, id_ord(r)));

    let mut entries: Vec<FiveStarEntry> = Vec::new();
    let mut counter: u32 = 0;
    // 大保底状态：上一次五星歪了，本次出金必为 UP
    let mut guaranteed = false;
    // 小保底连歪次数（上限 3），用于推定捕获明光：连歪 3 次后下次小保底必出 UP
    let mut radiance_counter: u32 = 0;

    for record in &sorted {
        counter += 1;
        if !record.is_five_star() {
            continue;
        }
        let is_up = if kind.has_up() {
            Some(
                !(pools::is_standard_five_star(&record.item_name)
                    || learned_standard.contains(&record.item_name)),
            )
        } else {
            None
        };
        // 捕获明光推定：仅角色活动祈愿；大保底出 UP 属正常，不算明光；
        // 只有「小保底 + 已连歪 3 次 + 本次出 UP」才是官方保证必定触发的那一次。
        let captured_radiance = match is_up {
            Some(true) => {
                kind.has_capturing_radiance() && !guaranteed && radiance_counter >= 3
            }
            _ => false,
        };

        entries.push(FiveStarEntry {
            id: record.id.clone(),
            name: record.item_name.clone(),
            item_type: record.item_type.clone(),
            time: record.time,
            pity: counter,
            is_up,
            guaranteed,
            captured_radiance,
        });

        match is_up {
            Some(false) => {
                // 小保底歪了 → 下一个五星进大保底，并累计连歪（官方计数上限为 3）
                guaranteed = true;
                radiance_counter = (radiance_counter + 1).min(3);
            }
            Some(true) => {
                if !guaranteed {
                    // 小保底直接出 UP（50/50 赢或明光触发）：连歪被打断，计数归零。
                    // 大保底 UP 是连歪序列中的一环，不打断计数。
                    radiance_counter = 0;
                }
                guaranteed = false;
            }
            None => {}
        }
        counter = 0;
    }

    let total = sorted.len() as u32;
    let five_star_count = entries.len() as u32;
    let current_pity = counter;

    let avg_pity = if five_star_count > 0 {
        Some(round1(total as f64 / five_star_count as f64))
    } else {
        None
    };

    let up_count = entries
        .iter()
        .filter(|e| matches!(e.is_up, Some(true)))
        .count() as u32;
    let off_count = entries
        .iter()
        .filter(|e| matches!(e.is_up, Some(false)))
        .count() as u32;

    let judged = up_count + off_count;
    let not_off_rate = if judged > 0 {
        let ratio = up_count as f64 / judged as f64;
        Some((ratio * 1000.0).round() / 1000.0)
    } else {
        None
    };

    let avg_up_pity = if up_count > 0 {
        Some(round1(total as f64 / up_count as f64))
    } else {
        None
    };

    let (mut max_up_streak, mut max_off_streak) = (0u32, 0u32);
    let (mut up_run, mut off_run) = (0u32, 0u32);
    for entry in &entries {
        match entry.is_up {
            Some(true) => {
                up_run += 1;
                off_run = 0;
                max_up_streak = max_up_streak.max(up_run);
            }
            Some(false) => {
                off_run += 1;
                up_run = 0;
                max_off_streak = max_off_streak.max(off_run);
            }
            None => {
                up_run = 0;
                off_run = 0;
            }
        }
    }

    let cost_per_up = avg_up_pity.map(|avg| (avg * PRIMOGEM_PER_PULL as f64).round() as u64);

    PoolStats {
        kind,
        label: label.to_string(),
        total,
        five_star_count,
        entries,
        avg_pity,
        up_count,
        off_count,
        not_off_rate,
        avg_up_pity,
        max_up_streak,
        max_off_streak,
        current_pity,
        cost_per_up,
    }
}

/// 账号级汇总。
pub fn analyze(uid: &str, records: &[PullRecord]) -> AccountStats {
    let mut sorted: Vec<&PullRecord> = records.iter().collect();
    sorted.sort_by_key(|r| (r.time, id_ord(r)));

    // 先用常驻池记录补齐「常驻五星」认知，再逐池统计
    let learned = learned_standard_names(records);

    let pools_stats: Vec<PoolStats> = pools::POOLS
        .iter()
        .map(|def| {
            let bucket: Vec<PullRecord> = sorted
                .iter()
                .filter(|r| def.gacha_types.contains(&r.gacha_type))
                .map(|r| (*r).clone())
                .collect();
            analyze_pool_with(def.kind, def.label, &bucket, &learned)
        })
        .collect();

    let mut unknown_types: Vec<u32> = Vec::new();
    for record in &sorted {
        if pools::resolve_pool(record.gacha_type).is_none()
            && !unknown_types.contains(&record.gacha_type)
        {
            unknown_types.push(record.gacha_type);
        }
    }

    let total_pulls = sorted.len() as u32;
    let first_time = sorted.first().map(|r| r.time);
    let last_time = sorted.last().map(|r| r.time);

    let mut longest_gap_days = 0u32;
    for pair in sorted.windows(2) {
        let gap = (pair[1].time - pair[0].time).num_days();
        if gap > longest_gap_days as i64 {
            longest_gap_days = gap as u32;
        }
    }

    let character = pools_stats
        .iter()
        .find(|p| p.kind == PoolKind::Character)
        .cloned();
    let weapon = pools_stats
        .iter()
        .find(|p| p.kind == PoolKind::Weapon)
        .cloned();

    let not_off_rate = character.as_ref().and_then(|p| p.not_off_rate);
    let avg_up_character_pity = character.as_ref().and_then(|p| p.avg_up_pity);
    let avg_limited_weapon_pity = weapon.as_ref().and_then(|p| p.avg_up_pity);
    let max_up_streak = pools_stats.iter().map(|p| p.max_up_streak).max().unwrap_or(0);
    let max_off_streak = pools_stats
        .iter()
        .map(|p| p.max_off_streak)
        .max()
        .unwrap_or(0);

    let rating = evaluate(RatingInput {
        pulls: character.as_ref().map(|p| p.total).unwrap_or(0),
        avg_up_pity: avg_up_character_pity,
        not_off_rate,
        max_off_streak,
    });

    let cost_per_up_character = character.as_ref().and_then(|p| p.cost_per_up);
    let cost_per_limited_weapon = weapon.as_ref().and_then(|p| p.cost_per_up);

    AccountStats {
        uid: uid.to_string(),
        total_pulls,
        first_time,
        last_time,
        longest_gap_days,
        pools: pools_stats,
        rating,
        not_off_rate,
        avg_up_character_pity,
        avg_limited_weapon_pity,
        max_up_streak,
        max_off_streak,
        first_run_ratio: None,
        cost_per_up_character,
        cost_per_limited_weapon,
        unknown_types,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: u64, gacha_type: u32, name: &str, rank: u8, day: u32, hour: u32) -> PullRecord {
        let text = format!("2023-02-{day:02} {hour:02}:00:00");
        PullRecord {
            id: id.to_string(),
            uid: "114709792".to_string(),
            gacha_type,
            item_id: format!("item{id}"),
            item_name: name.to_string(),
            item_type: if rank == 5 { "角色" } else { "武器" }.to_string(),
            rank_type: rank,
            time: NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M:%S").unwrap(),
        }
    }

    #[test]
    fn pity_counts_pulls_including_the_five_star() {
        let records = vec![
            rec(1, 301, "三星武器", 3, 1, 10),
            rec(2, 301, "四星武器", 4, 1, 11),
            rec(3, 301, "三星武器", 3, 1, 12),
            rec(4, 301, "纳西妲", 5, 1, 13),
            rec(5, 301, "三星武器", 3, 1, 14),
        ];
        let stats = analyze_pool(PoolKind::Character, "角色", &records);
        assert_eq!(stats.five_star_count, 1);
        assert_eq!(stats.entries[0].pity, 4);
        assert_eq!(stats.current_pity, 1);
        assert_eq!(stats.total, 5);
    }

    #[test]
    fn standard_five_star_is_judged_as_off_rate() {
        let records = vec![
            rec(1, 301, "迪卢克", 5, 1, 10),
            rec(2, 301, "纳西妲", 5, 2, 10),
            rec(3, 301, "琴", 5, 3, 10),
        ];
        let stats = analyze_pool(PoolKind::Character, "角色", &records);
        assert_eq!(stats.up_count, 1);
        assert_eq!(stats.off_count, 2);
        let rate = stats.not_off_rate.expect("rate");
        assert!((rate - 0.333).abs() < 1e-9, "rate = {rate}");
        assert_eq!(stats.max_off_streak, 1);
    }

    #[test]
    fn streaks_are_tracked_separately() {
        let records = vec![
            rec(1, 302, "天空之刃", 5, 1, 10),
            rec(2, 302, "天空之傲", 5, 2, 10),
            rec(3, 302, "薙草之稻光", 5, 3, 10),
            rec(4, 302, "薙草之稻光", 5, 4, 10),
            rec(5, 302, "薙草之稻光", 5, 5, 10),
        ];
        let stats = analyze_pool(PoolKind::Weapon, "武器", &records);
        assert_eq!(stats.max_off_streak, 2);
        assert_eq!(stats.max_up_streak, 3);
    }

    #[test]
    fn standard_pool_has_no_up_concept() {
        let records = vec![rec(1, 200, "迪卢克", 5, 1, 10)];
        let stats = analyze_pool(PoolKind::Standard, "常驻", &records);
        assert!(stats.entries[0].is_up.is_none());
        assert_eq!(stats.not_off_rate, None);
        assert_eq!(stats.up_count, 0);
    }

    #[test]
    fn learned_standard_names_come_from_standard_pool_records() {
        let records = vec![
            rec(1, 200, "薇斯纳", 5, 1, 10),
            rec(2, 200, "三星武器", 3, 2, 10),
            rec(3, 301, "菲林斯", 5, 3, 10),
        ];
        let learned = learned_standard_names(&records);
        assert!(learned.contains("薇斯纳"));
        assert!(!learned.contains("菲林斯"));
        assert_eq!(learned.len(), 1);
    }

    /// 回归用例：静态表里没有的新常驻角色，只要在常驻池出过，
    /// 角色池里再出就必须判为「歪」。
    #[test]
    fn new_standard_character_from_own_history_is_judged_as_off_rate() {
        let records = vec![
            rec(1, 200, "薇斯纳", 5, 1, 10),
            rec(2, 301, "薇斯纳", 5, 2, 10),
            rec(3, 301, "纳西妲", 5, 3, 10),
        ];
        let stats = analyze("114709792", &records);
        let character = stats
            .pools
            .iter()
            .find(|pool| pool.kind == PoolKind::Character)
            .expect("character pool");

        let newcomer = character
            .entries
            .iter()
            .find(|entry| entry.name == "薇斯纳")
            .expect("薇斯纳 entry");
        assert_eq!(newcomer.is_up, Some(false));
        assert_eq!(character.up_count, 1);
        assert_eq!(character.off_count, 1);
    }

    /// 歪 → 大保底 UP，guaranteed 标志必须跟着走
    #[test]
    fn guaranteed_flag_follows_losses() {
        let records = vec![
            rec(1, 301, "迪卢克", 5, 1, 10),
            rec(2, 301, "纳西妲", 5, 2, 10),
            rec(3, 301, "琴", 5, 3, 10),
            rec(4, 301, "纳西妲", 5, 4, 10),
        ];
        let stats = analyze_pool(PoolKind::Character, "角色", &records);
        assert!(!stats.entries[0].guaranteed);
        assert!(stats.entries[1].guaranteed);
        assert!(!stats.entries[2].guaranteed);
        assert!(stats.entries[3].guaranteed);
    }

    /// 官方规则：连续三次小保底歪之后，第四次小保底出 UP 才是捕获明光
    #[test]
    fn radiance_triggers_after_three_consecutive_losses() {
        let records = vec![
            rec(1, 301, "迪卢克", 5, 1, 10), // 歪 1
            rec(2, 301, "纳西妲", 5, 2, 10), // 大保底 UP
            rec(3, 301, "琴", 5, 3, 10),     // 歪 2
            rec(4, 301, "纳西妲", 5, 4, 10), // 大保底 UP
            rec(5, 301, "莫娜", 5, 5, 10),   // 歪 3
            rec(6, 301, "纳西妲", 5, 6, 10), // 大保底 UP
            rec(7, 301, "纳西妲", 5, 7, 10), // 小保底 UP → 明光
        ];
        let stats = analyze_pool(PoolKind::Character, "角色", &records);
        for (index, entry) in stats.entries.iter().enumerate() {
            if index == 6 {
                assert!(!entry.guaranteed);
                assert!(entry.captured_radiance, "第 7 次应判定为捕获明光");
            } else {
                assert!(!entry.captured_radiance, "第 {} 次不该是明光", index + 1);
            }
        }
    }

    /// 只连歪 2 次、或中途小保底赢过一次，都不满足「连续三次」
    #[test]
    fn radiance_requires_three_uninterrupted_losses() {
        let records = vec![
            rec(1, 301, "迪卢克", 5, 1, 10), // 歪 1
            rec(2, 301, "纳西妲", 5, 2, 10), // 大保底 UP
            rec(3, 301, "琴", 5, 3, 10),     // 歪 2
            rec(4, 301, "纳西妲", 5, 4, 10), // 大保底 UP
            rec(5, 301, "纳西妲", 5, 5, 10), // 小保底赢 → 计数归零
            rec(6, 301, "七七", 5, 6, 10),   // 歪 1
            rec(7, 301, "纳西妲", 5, 7, 10), // 大保底 UP
            rec(8, 301, "刻晴", 5, 8, 10),   // 歪 2
            rec(9, 301, "纳西妲", 5, 9, 10), // 大保底 UP
            rec(10, 301, "纳西妲", 5, 10, 10), // 小保底赢 → 仍不是明光
        ];
        let stats = analyze_pool(PoolKind::Character, "角色", &records);
        assert!(!stats.entries[4].captured_radiance);
        assert!(!stats.entries[9].captured_radiance);
        assert!(stats.entries.iter().all(|entry| !entry.captured_radiance));
    }

    /// 官方公告：武器祈愿没有捕获明光
    #[test]
    fn radiance_only_applies_to_character_banner() {
        let records = vec![
            rec(1, 302, "天空之刃", 5, 1, 10),
            rec(2, 302, "薙草之稻光", 5, 2, 10),
            rec(3, 302, "天空之傲", 5, 3, 10),
            rec(4, 302, "薙草之稻光", 5, 4, 10),
            rec(5, 302, "阿莫斯之弓", 5, 5, 10),
            rec(6, 302, "薙草之稻光", 5, 6, 10),
            rec(7, 302, "薙草之稻光", 5, 7, 10),
        ];
        let stats = analyze_pool(PoolKind::Weapon, "武器", &records);
        assert!(stats.entries.iter().all(|entry| !entry.captured_radiance));
    }

    #[test]
    fn unsorted_input_is_reordered() {
        let records = vec![
            rec(9, 301, "纳西妲", 5, 5, 10),
            rec(1, 301, "三星武器", 3, 1, 10),
            rec(2, 301, "三星武器", 3, 2, 10),
        ];
        let stats = analyze_pool(PoolKind::Character, "角色", &records);
        assert_eq!(stats.entries[0].pity, 3);
    }

    #[test]
    fn unknown_pool_type_is_reported_not_dropped() {
        let records = vec![
            rec(1, 301, "纳西妲", 5, 1, 10),
            rec(2, 600, "某千星角色", 5, 2, 10),
        ];
        let stats = analyze("114709792", &records);
        assert_eq!(stats.unknown_types, vec![600]);
        assert_eq!(stats.total_pulls, 2);
    }

    #[test]
    fn longest_gap_is_computed_across_all_pools() {
        let records = vec![
            rec(1, 301, "三星武器", 3, 1, 10),
            rec(2, 301, "三星武器", 3, 5, 10),
            rec(3, 200, "三星武器", 3, 9, 10),
        ];
        let stats = analyze("114709792", &records);
        assert_eq!(stats.longest_gap_days, 4);
        assert_eq!(stats.total_pulls, 3);
        assert!(stats.first_run_ratio.is_none());
    }
}
