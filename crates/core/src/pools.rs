//! 祈愿池与常驻五星表。
//!
//! 全部集中在这里，禁止在统计或 UI 代码里散落硬编码。
//! 新版本上线新池、或常驻池新增五星时，只改本文件。

use crate::model::PoolKind;

/// 池定义。`gacha_types` 为官方接口的池类型编号。
pub struct PoolDef {
    pub kind: PoolKind,
    pub gacha_types: &'static [u32],
    pub label: &'static str,
}

/// 池顺序即 UI 上的 Tab 顺序，与参考截图一致：角色 / 武器 / 常驻 / 混池 / 千星奇域。
pub const POOLS: &[PoolDef] = &[
    PoolDef {
        kind: PoolKind::Character,
        gacha_types: &[301, 400],
        label: "角色",
    },
    PoolDef {
        kind: PoolKind::Weapon,
        gacha_types: &[302],
        label: "武器",
    },
    PoolDef {
        kind: PoolKind::Standard,
        gacha_types: &[200],
        label: "常驻",
    },
    PoolDef {
        kind: PoolKind::Chronicled,
        gacha_types: &[500],
        label: "混池",
    },
    // 千星奇域：官方 gacha_type 编号未经实测确认。
    // 留空表示「暂不识别」，其记录会在统计结果里以 unknown_types 形式暴露出来，不会静默丢失。
    PoolDef {
        kind: PoolKind::Starlight,
        gacha_types: &[],
        label: "千星奇域",
    },
    PoolDef {
        kind: PoolKind::Beginner,
        gacha_types: &[100],
        label: "新手",
    },
];

/// 探测未知池类型时的候选编号。通过比对返回记录的 item_name 与游戏内公告确认。
pub const PROBE_CANDIDATE_TYPES: &[u32] = &[600, 700, 800, 900, 1000];

/// 由原始 gacha_type 反查池种类。
pub fn resolve_pool(gacha_type: u32) -> Option<PoolKind> {
    POOLS
        .iter()
        .find(|def| def.gacha_types.contains(&gacha_type))
        .map(|def| def.kind)
}

/// 池种类对应的 UI 标签。
pub fn pool_label(kind: PoolKind) -> &'static str {
    POOLS
        .iter()
        .find(|def| def.kind == kind)
        .map(|def| def.label)
        .unwrap_or("其他")
}

/// 常驻池五星角色。出现在 UP 池里即视为「歪」。
///
/// 数据基准：7.1 版本（2026-09），「奔行世间」常驻祈愿共 8 名五星角色。
/// 琴 / 迪卢克 / 七七 / 莫娜 / 刻晴 / 提纳里 / 迪希雅 为初版七人，
/// **梦见月瑞希（Mizuki）自 5.5 版本起加入常驻池**。
///
/// 维护说明：新版本把某个五星加入常驻池时，在这里补一行即可。
/// 万一漏了也不用担心——统计时会用你自己「常驻池」记录里出过的五星自动补充，
/// 见 `stats::learned_standard_names`。这份静态表是初始值，不是唯一真相。
pub const STANDARD_FIVE_STAR_CHARACTERS: &[&str] = &[
    "迪卢克",
    "琴",
    "莫娜",
    "七七",
    "刻晴",
    "提纳里",
    "迪希雅",
    "梦见月瑞希",
];

/// 常驻池五星武器。共 10 把，自开服以来未新增。
///
/// 注意「和璞鸢」属于常驻池，别漏——它和天空之脊同为长柄武器。
pub const STANDARD_FIVE_STAR_WEAPONS: &[&str] = &[
    "天空之刃",
    "天空之傲",
    "天空之卷",
    "天空之翼",
    "天空之脊",
    "阿莫斯之弓",
    "狼的末路",
    "风鹰剑",
    "四风原典",
    "和璞鸢",
];

/// 判断某个五星是否为常驻五星，即是否算「歪」。
///
/// 判定依据是物品名是否属于常驻五星集合，而不是「抽了多少抽」。
/// 这样在 5.0 引入捕获明光（Capturing Radiance）之后依然成立。
pub fn is_standard_five_star(name: &str) -> bool {
    STANDARD_FIVE_STAR_CHARACTERS.contains(&name) || STANDARD_FIVE_STAR_WEAPONS.contains(&name)
}

/// 静态名单里的全部常驻五星，供 UI 展示或调试使用。
pub fn static_standard_names() -> Vec<&'static str> {
    STANDARD_FIVE_STAR_CHARACTERS
        .iter()
        .chain(STANDARD_FIVE_STAR_WEAPONS.iter())
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 名单与游戏实际对齐：8 名角色 + 10 把武器。
    /// 新增常驻角色时改上面的常量，并同步改这个断言。
    #[test]
    fn standard_pool_roster_matches_game() {
        assert_eq!(STANDARD_FIVE_STAR_CHARACTERS.len(), 8);
        assert_eq!(STANDARD_FIVE_STAR_WEAPONS.len(), 10);
    }

    #[test]
    fn mizuki_and_jade_spear_are_standard() {
        assert!(is_standard_five_star("梦见月瑞希"));
        assert!(is_standard_five_star("和璞鸢"));
        assert!(is_standard_five_star("迪希雅"));
        assert!(!is_standard_five_star("纳西妲"));
    }
}

/// 一抽折算的原石数，用于「平均花费原石」指标。
pub const PRIMOGEM_PER_PULL: u64 = 160;
