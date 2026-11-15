// 桌面端 Tauri 壳：只做 IPC 与事件转发，全部业务逻辑在 gacha-* crate 里。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod characters;

use std::sync::Mutex;

use gacha_core::{analyze, Account, AccountStats, PullRecord, POOLS, PROBE_CANDIDATE_TYPES};
use gacha_fetch::{best_local_url, scan_local_cache, GachaClient, GachaUrl};
use gacha_store::{default_db_path, AccountRow, Store};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    db: Mutex<Store>,
}

fn open_store() -> Store {
    let path = default_db_path();
    match Store::open(&path) {
        Ok(store) => store,
        Err(err) => {
            eprintln!("无法打开数据库 {}：{err}，回退到内存库", path.display());
            Store::open_in_memory().expect("内存数据库也无法创建")
        }
    }
}

#[derive(Serialize)]
struct LocalLinkDto {
    gacha_type: Option<u32>,
    score: i32,
    cache_path: String,
    masked_authkey: String,
}

#[derive(Serialize, Clone)]
struct SyncProgress {
    phase: String,
    pool: String,
    gacha_type: u32,
    fetched: u32,
}

#[derive(Serialize)]
struct SyncOutcome {
    uid: String,
    fetched: u32,
    added: usize,
    total: i64,
    /// 是否因为命中本地已有记录而提前停止（增量同步生效）
    stopped_early: bool,
}

#[derive(Serialize)]
struct ProbeHit {
    gacha_type: u32,
    count: usize,
}

fn mask_uid(uid: &str) -> String {
    let chars: Vec<char> = uid.chars().collect();
    if chars.len() <= 4 {
        return "****".to_string();
    }
    let head: String = chars[..chars.len() - 3].iter().collect();
    format!("{head}***")
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=");
    let start = url.find(&pattern)? + pattern.len();
    let rest = &url[start..];
    let end = rest.find('&').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// 解析链接：优先用用户粘贴/工具导出的链接，其次扫描本机游戏缓存兜底。
fn resolve_url(url: Option<String>) -> Result<GachaUrl, String> {
    if let Some(raw) = url {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return GachaUrl::parse(trimmed).map_err(|err| err.to_string());
        }
    }

    let hit = best_local_url().map_err(|err| err.to_string())?;
    GachaUrl::parse(&hit.url).map_err(|err| err.to_string())
}

#[tauri::command]
fn scan_local_links() -> Result<Vec<LocalLinkDto>, String> {
    let hits = scan_local_cache().map_err(|err| err.to_string())?;
    Ok(hits
        .into_iter()
        .map(|hit| {
            let masked = GachaUrl::parse(&hit.url)
                .map(|url| url.masked_authkey())
                .unwrap_or_else(|_| "****".to_string());
            LocalLinkDto {
                gacha_type: query_param(&hit.url, "gacha_type").and_then(|value| value.parse().ok()),
                score: hit.score,
                cache_path: hit.cache_path.display().to_string(),
                masked_authkey: masked,
            }
        })
        .collect())
}

/// 同样是联网命令，必须离开主线程，否则池类型探测会把窗口冻住。
#[tauri::command]
async fn probe_pools(url: Option<String>) -> Result<Vec<ProbeHit>, String> {
    tauri::async_runtime::spawn_blocking(
        move || -> Result<Vec<ProbeHit>, String> {
            let parsed = resolve_url(url)?;
            let client = GachaClient::new(parsed);
            Ok(client
                .probe_pool_types(PROBE_CANDIDATE_TYPES)
                .into_iter()
                .map(|(gacha_type, count)| ProbeHit { gacha_type, count })
                .collect())
        },
    )
    .await
    .map_err(|err| format!("探测线程崩溃：{err}"))?
}

fn run_sync(app: &AppHandle, url: Option<String>, auto: bool) -> Result<SyncOutcome, String> {
    let parsed = if auto {
        // auto = 扫描本机游戏缓存（需先在游戏内打开过祈愿记录）
        resolve_url(None)?
    } else {
        resolve_url(url)?
    };

    let client = GachaClient::new(parsed.clone());
    let mut all: Vec<PullRecord> = Vec::new();
    let mut stopped_early = false;

    // 先把本地已有记录 id 全部取出来，供增量判断（此时还不知道 uid，故按全局取）
    let known = {
        let state = app.state::<AppState>();
        let store = state
            .db
            .lock()
            .map_err(|_| "数据库被占用，请重启应用".to_string())?;
        store.all_existing_ids().map_err(|err| err.to_string())?
    };

    for def in POOLS {
        if def.gacha_types.is_empty() {
            continue;
        }
        for &gacha_type in def.gacha_types {
            let pool_name = def.label.to_string();
            let outcome = client
                .fetch_pool_incremental(gacha_type, &known, |fetched| {
                    let _ = app.emit(
                        "sync-progress",
                        SyncProgress {
                            phase: "fetching".to_string(),
                            pool: pool_name.clone(),
                            gacha_type,
                            fetched,
                        },
                    );
                })
                .map_err(|err| format!("{}池抓取失败：{err}", def.label))?;
            stopped_early = stopped_early || outcome.stopped_early;
            all.extend(outcome.records);
        }
    }

    // 增量同步下「没有新记录」是正常结果，不能当失败处理
    if all.is_empty() {
        let state = app.state::<AppState>();
        let store = state
            .db
            .lock()
            .map_err(|_| "数据库被占用，请重启应用".to_string())?;
        let accounts = store.list_accounts().map_err(|err| err.to_string())?;
        if let Some(account) = accounts.first() {
            store.mark_synced(&account.uid).map_err(|err| err.to_string())?;
            return Ok(SyncOutcome {
                uid: account.uid.clone(),
                fetched: 0,
                added: 0,
                total: store.count_pulls(&account.uid).map_err(|err| err.to_string())?,
                stopped_early: true,
            });
        }
        return Err("没有抓到任何记录。authkey 可能已过期，或链接不属于当前账号。".to_string());
    }

    let uid = all
        .iter()
        .find(|record| !record.uid.is_empty())
        .map(|record| record.uid.clone())
        .ok_or_else(|| "记录里没有 UID，无法关联账号".to_string())?;

    let state = app.state::<AppState>();
    let mut store = state
        .db
        .lock()
        .map_err(|_| "数据库被占用，请重启应用".to_string())?;

    store
        .upsert_account(&Account {
            uid: uid.clone(),
            nickname: None,
            region: parsed.region.clone(),
            game_biz: parsed.game_biz.clone(),
        })
        .map_err(|err| err.to_string())?;

    let log_id = store.start_sync_log(&uid).map_err(|err| err.to_string())?;
    let added = store.insert_pulls(&all).map_err(|err| err.to_string())?;
    store.mark_synced(&uid).map_err(|err| err.to_string())?;
    store
        .finish_sync_log(log_id, added, "ok", "")
        .map_err(|err| err.to_string())?;
    let total = store.count_pulls(&uid).map_err(|err| err.to_string())?;

    let _ = app.emit(
        "sync-progress",
        SyncProgress {
            phase: "done".to_string(),
            pool: String::new(),
            gacha_type: 0,
            fetched: all.len() as u32,
        },
    );

    eprintln!(
        "同步完成 uid={} 抓取={} 新增={} 合计={} 增量命中={}",
        mask_uid(&uid),
        all.len(),
        added,
        total,
        stopped_early
    );

    Ok(SyncOutcome {
        uid,
        fetched: all.len() as u32,
        added,
        total,
        stopped_early,
    })
}

#[tauri::command]
async fn sync_records(
    app: AppHandle,
    url: Option<String>,
    auto: bool,
) -> Result<SyncOutcome, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || run_sync(&handle, url, auto))
        .await
        .map_err(|err| format!("同步线程崩溃：{err}"))?
}

#[tauri::command]
fn list_accounts(state: State<'_, AppState>) -> Result<Vec<AccountRow>, String> {
    let store = state
        .db
        .lock()
        .map_err(|_| "数据库被占用，请重启应用".to_string())?;
    store.list_accounts().map_err(|err| err.to_string())
}

fn load_records(state: &AppState, uid: &str) -> Result<Vec<PullRecord>, String> {
    let store = state
        .db
        .lock()
        .map_err(|_| "数据库被占用，请重启应用".to_string())?;
    store.load_pulls(uid).map_err(|err| err.to_string())
}

#[tauri::command]
fn get_stats(state: State<'_, AppState>, uid: Option<String>) -> Result<AccountStats, String> {
    let target_uid = {
        let store = state
            .db
            .lock()
            .map_err(|_| "数据库被占用，请重启应用".to_string())?;
        let accounts = store.list_accounts().map_err(|err| err.to_string())?;
        match uid {
            Some(uid) => uid,
            None => accounts
                .first()
                .map(|account| account.uid.clone())
                .ok_or_else(|| "本地还没有任何抽卡记录".to_string())?,
        }
    };

    let records = load_records(&state, &target_uid)?;
    Ok(analyze(&target_uid, &records))
}

#[tauri::command]
fn get_records(state: State<'_, AppState>, uid: String) -> Result<Vec<PullRecord>, String> {
    load_records(&state, &uid)
}

#[tauri::command]
fn delete_account(state: State<'_, AppState>, uid: String) -> Result<(), String> {
    let store = state
        .db
        .lock()
        .map_err(|_| "数据库被占用，请重启应用".to_string())?;
    store.delete_account(&uid).map_err(|err| err.to_string())
}

#[tauri::command]
fn rename_account(state: State<'_, AppState>, uid: String, nickname: String) -> Result<(), String> {
    let store = state
        .db
        .lock()
        .map_err(|_| "数据库被占用，请重启应用".to_string())?;
    store
        .set_nickname(&uid, &nickname)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn export_uigf(state: State<'_, AppState>, uid: String) -> Result<String, String> {
    let records = load_records(&state, &uid)?;
    let list: Vec<serde_json::Value> = records
        .iter()
        .map(|record| {
            serde_json::json!({
                "uigf_gacha_type": match record.gacha_type {
                    301 | 400 => 301,
                    other => other,
                },
                "gacha_type": record.gacha_type.to_string(),
                "id": record.id,
                "item_id": record.item_id,
                "name": record.item_name,
                "item_type": record.item_type,
                "rank_type": record.rank_type.to_string(),
                "time": record.time.format("%Y-%m-%d %H:%M:%S").to_string(),
            })
        })
        .collect();

    let payload = serde_json::json!({
        "info": {
            "uid": uid,
            "lang": "zh-cn",
            "export_timestamp": chrono::Local::now().timestamp(),
            "export_app": "teyvat-gacha",
            "export_app_version": "0.1.0",
            "uigf_version": "v4.0",
        },
        "hk4e": [{ "uid": uid, "list": list }]
    });

    serde_json::to_string_pretty(&payload).map_err(|err| err.to_string())
}

/// 直接读系统剪贴板文本。
///
/// 刻意不用前端的 `navigator.clipboard.readText()`：WebView2 会把 clipboard-read
/// 当成需要用户授权的权限，弹框打扰且容易被拒。桌面客户端自己读系统剪贴板即可。
#[tauri::command]
fn read_clipboard_text() -> Result<String, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|err| format!("剪贴板不可用：{err}"))?;
    match clipboard.get_text() {
        Ok(text) => Ok(text),
        Err(err) => Err(format!("剪贴板里没有文本：{err}")),
    }
}

/// 批量取角色档案（元素 + 头像），供出金列表渲染。
///
/// 必须是 async + spawn_blocking：`download=true` 时会同步联网取图，
/// 同步命令跑在 Tauri 主线程，一旦 CDN 挂起就会让整个窗口变成「未响应」。
#[tauri::command]
async fn character_profiles(names: Vec<String>, download: bool) -> Vec<characters::CharacterProfile> {
    tauri::async_runtime::spawn_blocking(move || characters::profiles_for(&names, download))
        .await
        .unwrap_or_default()
}

/// 调试用：设了 `TEYVAT_START_ROUTE=/analysis/<uid>` 就直接打开该页，
/// 免去自动化点击（截图验收 UI 时用），不设则正常从查询页开始。
#[tauri::command]
fn start_route() -> Option<String> {
    std::env::var("TEYVAT_START_ROUTE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| value.starts_with('/') && value.len() > 1)
}

/// 前端挂载完成后调用，仅用于诊断：桌面端没有控制台，白屏时靠这条记录判断
/// 是「前端没跑起来」还是「跑起来了但渲染异常」。
/// 写文件而不是打日志：GUI 子系统进程的 stderr 往往采不到。
#[tauri::command]
fn frontend_ready(route: String) {
    use std::io::Write;
    let path = gacha_store::app_home().join("frontend-ready.log");
    let stamp = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "[{stamp}] {route}");
    }
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            db: Mutex::new(open_store()),
        })
        .invoke_handler(tauri::generate_handler![
            scan_local_links,
            probe_pools,
            sync_records,
            list_accounts,
            get_stats,
            get_records,
            delete_account,
            rename_account,
            export_uigf,
            read_clipboard_text,
            character_profiles,
            start_route,
            frontend_ready
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 启动失败");
}
