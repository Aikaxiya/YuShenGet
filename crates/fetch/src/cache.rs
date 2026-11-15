//! 从原神本地浏览器缓存里回溯祈愿记录链接。
//!
//! 原理：游戏内打开「祈愿记录」时，内嵌浏览器会请求
//! `.../getGachaLog?authkey=...`，该请求 URL 以明文残留在
//! `%LOCALAPPDATA%\Low\miHoYo\原神\webCaches\<版本>\Cache\Cache_Data\data_2` 里。
//!
//! 注意：路径含中文与空格，全程用 `PathBuf`/`OsStr` 处理，
//! 禁止 `to_str().unwrap()`，否则在非中文系统或特殊编码下会直接 panic。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::FetchError;
use crate::url::GachaUrl;
use crate::url_extract::extract_gacha_urls;

/// 单个缓存命中结果。
#[derive(Debug, Clone)]
pub struct CacheHit {
    pub cache_path: PathBuf,
    pub url: String,
    /// 排序权重，越大越可能是可用的起始链接。
    pub score: i32,
}

/// 单文件扫描上限，避免把上百 MB 的缓存整个读进内存。
const MAX_SCAN_BYTES: usize = 96 * 1024 * 1024;

/// 可能的缓存根目录（国服 / 国际服 / 旧版布局）。
pub fn cache_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let base = PathBuf::from(local);
        roots.push(base.join("Low").join("miHoYo").join("原神").join("webCaches"));
        roots.push(
            base.join("Low")
                .join("miHoYo")
                .join("Genshin Impact")
                .join("webCaches"),
        );
        roots.push(base.join("miHoYo").join("原神").join("webCaches"));
    }
    roots.retain(|path| path.is_dir());
    roots
}

fn version_key(path: &Path) -> Vec<u32> {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    name.split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect()
}

fn version_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    dirs
}

fn data_file_candidates(version_dir: &Path) -> Vec<PathBuf> {
    let named = [
        version_dir.join("Cache").join("Cache_Data").join("data_2"),
        version_dir.join("Cache").join("data_2"),
        version_dir.join("Cache_Data").join("data_2"),
    ];
    let mut found: Vec<PathBuf> = named.into_iter().filter(|path| path.is_file()).collect();

    if found.is_empty() {
        // 兜底：浅层递归找 data_2，深度限制 4 层，避免全盘遍历。
        found = find_data_files(version_dir, 0, 4);
    }
    found
}

fn find_data_files(dir: &Path, depth: usize, max_depth: usize) -> Vec<PathBuf> {
    if depth > max_depth {
        return Vec::new();
    }
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => {
            return out;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(find_data_files(&path, depth + 1, max_depth));
        } else if path
            .file_name()
            .map(|name| name.eq_ignore_ascii_case("data_2"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    out
}

fn score_url(url: &str) -> i32 {
    let mut score = 0;
    if url.contains("end_id=0") {
        score += 4;
    }
    if url.contains("page=1") {
        score += 2;
    }
    if url.contains("gacha_type=") {
        score += 1;
    }
    if url.contains("authkey=") {
        score += 8;
    }
    score
}

fn scan_file(path: &Path) -> Vec<String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => {
            return Vec::new();
        }
    };
    let length = metadata.len() as usize;

    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Vec::new();
        }
    };

    let slice = if length > MAX_SCAN_BYTES {
        &bytes[length - MAX_SCAN_BYTES..]
    } else {
        &bytes[..]
    };

    extract_gacha_urls(slice)
}

/// 扫描所有已知缓存目录，返回可用的祈愿链接（已按可用性排序、去重）。
pub fn scan_local_cache() -> Result<Vec<CacheHit>, FetchError> {
    let roots = cache_roots();
    if roots.is_empty() {
        return Err(FetchError::CacheMissing(
            "%LOCALAPPDATA%\\Low\\miHoYo\\原神\\webCaches".to_string(),
        ));
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut hits: Vec<CacheHit> = Vec::new();

    for root in roots {
        for version_dir in version_dirs(&root).into_iter().take(3) {
            for data_file in data_file_candidates(&version_dir) {
                for url in scan_file(&data_file) {
                    if !seen.insert(url.clone()) {
                        continue;
                    }
                    // 只保留能解析出 authkey 的链接
                    if GachaUrl::parse(&url).is_err() {
                        continue;
                    }
                    hits.push(CacheHit {
                        cache_path: data_file.clone(),
                        score: score_url(&url),
                        url,
                    });
                }
            }
        }
    }

    hits.sort_by(|a, b| b.score.cmp(&a.score));
    Ok(hits)
}

/// 只取最优的一条链接，通常就是「角色活动祈愿」的第 1 页。
pub fn best_local_url() -> Result<CacheHit, FetchError> {
    let hits = scan_local_cache()?;
    hits.into_iter().next().ok_or_else(|| {
        FetchError::CacheMissing(
            "缓存中没有找到祈愿记录链接，请先在游戏内打开一次「祈愿 → 祈愿记录」".to_string(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_url_scores_higher_than_cursor_page() {
        let start = score_url("https://x/getGachaLog?authkey=K&gacha_type=301&page=1&end_id=0");
        let cursor = score_url("https://x/getGachaLog?authkey=K&gacha_type=301&end_id=999");
        assert!(start > cursor);
    }

    #[test]
    fn version_dirs_sort_descending() {
        assert!(version_key(Path::new("5.2.0.0")) > version_key(Path::new("4.10.0.0")));
    }
}
