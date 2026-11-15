//! 角色配置与头像：角色名录完全放在本地 JSON 里，代码不硬编码任何角色。
//!
//! ```text
//! data/
//!   characters.json          主配置（支持 // 与 /* */ 注释，JSONC）
//!     {
//!       "avatar_sources": ["https://.../UI_AvatarIcon_{icon}.png"],
//!       "characters": { "纳西妲": { "element": "dendro", "icon": "Nahida" } },
//!       "avatars":    { "纳西妲": "data:image/png;base64,..." }
//!     }
//!   characters.local.json    可选：增量新增/覆盖，优先级高于主配置
//!   avatars/<角色名>.png     可选：单独放图片文件
//! ```
//!
//! **以后新增角色只改配置加一条**，不用改代码、不用重新编译：
//! 在 `characters` 里加 `"角色名": { "element": "...", "icon": "游戏内部英文名" }`，
//! 头像写进 `avatars` 的 base64，或留空让程序按 `avatar_sources` 自动下载。
//! 推荐把自定义条目放进 `characters.local.json`，程序启动时自动合并，主配置不会被改写。
//!
//! 头像优先级：配置内嵌 base64 → `data/avatars/<角色名>.png` → 按 `avatar_sources` 联网下载。
//! 头像地址按**游戏内部英文名**拼装（`UI_AvatarIcon_Nahida.png`），不是简体中文名。
//! 默认地址模板仍是米哈游官方图床；官方只收录到 3.x，4.0+ 由 `tools/gen_avatars.py`
//! 在生成阶段从游戏内同款资源取好后内嵌，运行时不发第三方请求。
//!
//! 配置支持 `//` 与 `/* */` 注释，字符串内的 `https://` 会被正确保留。

use std::collections::BTreeMap;
use std::path::PathBuf;

use gacha_store::app_home;
use serde::{Deserialize, Serialize};

/// 默认头像源（按顺序尝试）。两条都是米哈游官方图床，用户可在 characters.json 里覆盖。
const DEFAULT_AVATAR_SOURCES: &[&str] = &[
    "https://upload-os-bbs.mihoyo.com/game_record/genshin/character_icon/UI_AvatarIcon_{icon}.png",
    "https://upload-os-bbs.hoyolab.com/game_record/genshin/character_icon/UI_AvatarIcon_{icon}.png",
];

/// 首次启动时写出的默认角色名录（不含头像 base64）。
/// 内容全部来自 `resources/characters.seed.json`，新增角色不需要改 Rust 代码。
const SEED_CONFIG: &str = include_str!("../resources/characters.seed.json");

const LOCAL_CONFIG_FILE: &str = "characters.local.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterEntry {
    #[serde(default)]
    pub element: String,
    /// 游戏内部英文名，用于拼 `UI_AvatarIcon_{icon}.png`
    #[serde(default)]
    pub icon: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CharacterConfig {
    /// 头像地址模板列表，`{icon}` 占位符会被替换成角色内部名。
    /// 只在配置里没有内嵌头像时才会用到，留空则使用默认值。
    #[serde(default)]
    pub avatar_sources: Vec<String>,
    #[serde(default)]
    pub characters: BTreeMap<String, CharacterEntry>,
    /// 内嵌头像：角色名 → `data:image/png;base64,...`。
    /// 由 `tools/gen_avatars.py` 从官方图床预先生成，运行时直接返回，不发任何请求。
    /// 用户也可以手填自己的图片 data URL 覆盖。
    #[serde(default)]
    pub avatars: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterProfile {
    pub name: String,
    pub element: String,
    /// data:image/png;base64,...；暂无头像时为 None
    pub avatar: Option<String>,
}

pub fn config_path() -> PathBuf {
    app_home().join("characters.json")
}

pub fn local_config_path() -> PathBuf {
    app_home().join(LOCAL_CONFIG_FILE)
}

pub fn avatars_dir() -> PathBuf {
    app_home().join("avatars")
}

/// 允许配置里写 `//` 与 `/* */` 注释（JSONC）。
/// 逐字符扫描，字符串里的 `//`（例如 `https://...`）原样保留，不会被误删。
fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while let Some(ch) = chars.next() {
        if line_comment {
            if ch == '\n' {
                line_comment = false;
                out.push(ch);
            }
            continue;
        }
        if block_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                block_comment = false;
            }
            continue;
        }
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                line_comment = true;
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                block_comment = true;
            }
            _ => out.push(ch),
        }
    }
    out
}

fn parse_config(text: &str, source: &str) -> Option<CharacterConfig> {
    match serde_json::from_str::<CharacterConfig>(&strip_json_comments(text)) {
        Ok(config) => Some(config),
        Err(err) => {
            eprintln!("{source} 解析失败（{err}），已忽略");
            None
        }
    }
}

/// 把增量配置合并进主配置：同名以增量为准，新名字直接追加。
fn merge_config(base: &mut CharacterConfig, extra: CharacterConfig) {
    if !extra.avatar_sources.is_empty() {
        base.avatar_sources = extra.avatar_sources;
    }
    for (name, entry) in extra.characters {
        base.characters.insert(name, entry);
    }
    for (name, avatar) in extra.avatars {
        base.avatars.insert(name, avatar);
    }
}

/// 读取配置：主配置 + 可选的 `characters.local.json` 增量。
/// 程序不会改写用户编辑过的主配置（只有首次启动会写出默认名录）。
pub fn load_config() -> CharacterConfig {
    let path = config_path();

    if !path.exists() {
        let _ = std::fs::create_dir_all(app_home());
        let _ = std::fs::write(&path, SEED_CONFIG);
    }

    let mut config = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| parse_config(&text, "characters.json"))
        .unwrap_or_else(|| parse_config(SEED_CONFIG, "内置默认名录").unwrap_or_default());

    if config.avatar_sources.is_empty() {
        config.avatar_sources = DEFAULT_AVATAR_SOURCES
            .iter()
            .map(|item| (*item).to_string())
            .collect();
    }

    let local_path = local_config_path();
    if let Some(extra) = std::fs::read_to_string(&local_path)
        .ok()
        .and_then(|text| parse_config(&text, LOCAL_CONFIG_FILE))
    {
        merge_config(&mut config, extra);
    }

    let _ = std::fs::create_dir_all(avatars_dir());
    config
}

fn avatar_file(name: &str) -> Option<PathBuf> {
    for extension in ["png", "jpg", "jpeg", "webp"] {
        let path = avatars_dir().join(format!("{name}.{extension}"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn data_url(path: &PathBuf) -> Option<String> {
    use base64::Engine;

    let bytes = std::fs::read(path).ok()?;
    let mime = match path
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// 按配置里的地址模板下载头像并落盘；成功返回文件路径。
fn download_avatar(name: &str, icon: &str, sources: &[String]) -> Option<PathBuf> {
    if icon.is_empty() || sources.is_empty() {
        return None;
    }

    for template in sources {
        // 模板里没有占位符就跳过，避免把所有角色都下成同一张图
        if !template.contains("{icon}") {
            continue;
        }
        let url = template.replace("{icon}", icon);
        match gacha_fetch::get_bytes_fast(&url) {
            Ok(bytes) if bytes.len() > 512 => {
                let path = avatars_dir().join(format!("{name}.png"));
                if std::fs::write(&path, bytes).is_ok() {
                    return Some(path);
                }
            }
            _ => {
                continue;
            }
        }
    }
    None
}

/// 单次调用最多联网补几张图。缺图只影响显示，不能让一次 IPC 长期占用后台线程。
const MAX_DOWNLOADS_PER_CALL: usize = 4;

/// 批量取角色档案。`download_missing = true` 时会尝试联网补齐缺失头像。
pub fn profiles_for(names: &[String], download_missing: bool) -> Vec<CharacterProfile> {
    let config = load_config();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut downloads = 0usize;

    for name in names {
        if !seen.insert(name.clone()) {
            continue;
        }

        let entry = config.characters.get(name);
        let element = entry
            .map(|item| item.element.clone())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unknown".to_string());

        // 1) 配置里内嵌的 base64：命中即返回，完全不碰网络
        // 2) 本地 avatars/ 目录里的图片
        // 3) 仍缺图且允许联网时，按 avatar_sources 下载补齐
        let mut avatar = config
            .avatars
            .get(name)
            .cloned()
            .filter(|value| value.starts_with("data:image/"));

        if avatar.is_none() {
            let mut file = avatar_file(name);
            if file.is_none() && download_missing && downloads < MAX_DOWNLOADS_PER_CALL {
                if let Some(icon) = entry.map(|item| item.icon.clone()).filter(|v| !v.is_empty()) {
                    // 不管成功与否都占一个额度，避免反复重试同一个取不到的地址
                    downloads += 1;
                    file = download_avatar(name, &icon, &config.avatar_sources);
                }
            }
            avatar = file.as_ref().and_then(data_url);
        }

        out.push(CharacterProfile {
            name: name.clone(),
            element,
            avatar,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_dedupe_and_keep_order() {
        let names = vec!["某不存在的角色".to_string(), "某不存在的角色".to_string()];
        let profiles = profiles_for(&names, false);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].element, "unknown");
        assert!(profiles[0].avatar.is_none());
    }

    #[test]
    fn jsonc_comments_do_not_break_urls() {
        let text = r#"{
            // 行注释
            "avatar_sources": ["https://upload-os-bbs.mihoyo.com/UI_AvatarIcon_Nahida.png"],
            /* 块注释 */ "characters": { "纳西妲": { "element": "dendro", "icon": "Nahida" } },
            "avatars": { "带//斜杠": "data:image/png;base64,abc//def" }
        }"#;
        let config: CharacterConfig = serde_json::from_str(&strip_json_comments(text)).unwrap();
        assert_eq!(config.avatar_sources.len(), 1);
        assert!(config.avatar_sources[0].contains("https://"));
        assert_eq!(config.characters["纳西妲"].icon, "Nahida");
        assert_eq!(
            config.avatars["带//斜杠"],
            "data:image/png;base64,abc//def"
        );
    }

    #[test]
    fn local_config_merges_and_overrides() {
        let mut base: CharacterConfig = serde_json::from_str(
            r#"{"characters":{"纳西妲":{"element":"dendro","icon":"Nahida"}},"avatars":{"纳西妲":"data:image/png;base64,old"}}"#,
        )
        .unwrap();
        let extra: CharacterConfig = serde_json::from_str(
            r#"{"characters":{"纳西妲":{"element":"hydro","icon":"Old"},"新角色":{"element":"anemo","icon":"New"}},"avatars":{"纳西妲":"data:image/png;base64,new"}}"#,
        )
        .unwrap();
        merge_config(&mut base, extra);
        assert_eq!(base.characters["纳西妲"].element, "hydro");
        assert_eq!(base.characters["新角色"].icon, "New");
        assert_eq!(base.avatars["纳西妲"], "data:image/png;base64,new");
    }

    #[test]
    fn seed_config_parses_and_is_config_driven() {
        let config: CharacterConfig =
            serde_json::from_str(&strip_json_comments(SEED_CONFIG)).unwrap();
        assert!(config.characters.len() > 100, "默认名录应来自配置文件");
        assert_eq!(config.characters["薇斯纳"].icon, "Vesna");
        assert_eq!(config.characters["玛薇卡"].element, "pyro");
        assert_eq!(config.characters["哥伦比娅"].icon, "Columbina");
    }
}
