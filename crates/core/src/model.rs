//! 领域模型：一次祈愿记录、账号、祈愿池种类。
//! 该层不依赖任何 IO，可被 GUI / CLI / 单元测试直接复用。

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// 单条祈愿记录。字段与官方 `getGachaLog` 返回一一对应，
/// 但 `rank_type` 已从字符串归一化为整数，`time` 已完成解析。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRecord {
    /// 官方记录 id，全局唯一，用作数据库主键，天然实现幂等写入。
    pub id: String,
    pub uid: String,
    /// 原始池类型（100/200/301/302/400/500/...）。
    pub gacha_type: u32,
    pub item_id: String,
    pub item_name: String,
    /// 「角色」或「武器」。
    pub item_type: String,
    /// 星级：3 / 4 / 5。
    pub rank_type: u8,
    /// 服务器本地时间，尚未做时区归一。
    pub time: NaiveDateTime,
}

impl PullRecord {
    pub fn is_five_star(&self) -> bool {
        self.rank_type == 5
    }
}

/// 祈愿池种类。`Starlight` 对应「千星奇域」，其 gacha_type 尚未实测确认，
/// 在 `pools.rs` 中留空，探测后再补，不允许在业务代码里硬编码猜测值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PoolKind {
    Character,
    Weapon,
    Standard,
    Chronicled,
    Starlight,
    Beginner,
}

impl PoolKind {
    /// 该池是否存在「UP 角色/武器」概念。常驻池与新手池没有 UP，不参与歪率统计。
    pub fn has_up(self) -> bool {
        matches!(
            self,
            PoolKind::Character | PoolKind::Weapon | PoolKind::Chronicled | PoolKind::Starlight
        )
    }

    /// 捕获明光只在「角色活动祈愿 / 角色活动祈愿-2」生效。
    /// 官方 5.0 公告明确：武器祈愿、集录祈愿（混池）没有这套机制，
    /// 因此不能用 `has_up()` 代替。
    pub fn has_capturing_radiance(self) -> bool {
        matches!(self, PoolKind::Character)
    }

    /// 该池的 5★ 保底硬上限，用于计算「距离保底还差多少」。
    pub fn hard_pity(self) -> u32 {
        match self {
            PoolKind::Weapon => 80,
            _ => 90,
        }
    }
}

/// 账号基本信息，不含任何凭据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub uid: String,
    pub nickname: Option<String>,
    pub region: String,
    pub game_biz: String,
}

impl Account {
    /// UID 首字符决定服务器时区偏移（单位：小时），用于把记录时间归一到用户本地时间。
    pub fn time_zone_offset_hours(&self) -> i32 {
        match self.uid.chars().next() {
            Some('6') => -5,
            Some('7') => 1,
            _ => 8,
        }
    }
}
