//! `gacha-core`：祈愿记录领域模型与统计内核。
//!
//! 无 IO、无异步、无网络依赖，可被桌面端、CLI 与单元测试直接复用。

pub mod model;
pub mod pools;
pub mod rating;
pub mod stats;

pub use model::{Account, PoolKind, PullRecord};
pub use pools::{
    is_standard_five_star, pool_label, resolve_pool, static_standard_names, PoolDef, POOLS,
    PRIMOGEM_PER_PULL, PROBE_CANDIDATE_TYPES, STANDARD_FIVE_STAR_CHARACTERS,
    STANDARD_FIVE_STAR_WEAPONS,
};
pub use rating::{evaluate, Rating, RatingInput};
pub use stats::{
    analyze, analyze_pool, analyze_pool_with, learned_standard_names, AccountStats, FiveStarEntry,
    PoolStats,
};
