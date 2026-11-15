//! `gacha-fetch`：祈愿记录获取层。
//!
//! 取址有两条路径：
//! 1. 外部工具导出的链接（手机上用 HoYoGet 之类导出后传回电脑，或手动复制）
//! 2. 扫描游戏本地浏览器缓存回溯链接
//!
//! authkey 只在本模块内存中流转，不写日志、不落盘。

pub mod cache;
pub mod client;
pub mod error;
pub mod http;
pub mod url;
pub mod url_extract;

pub use cache::{best_local_url, scan_local_cache, CacheHit};
pub use client::{FetchOutcome, GachaClient, PAGE_SIZE};
pub use error::FetchError;
pub use http::{get_bytes, get_bytes_fast};
pub use url::GachaUrl;
pub use url_extract::extract_gacha_urls;
