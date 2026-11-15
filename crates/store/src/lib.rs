//! `gacha-store`：本地 SQLite 持久层。
//!
//! 唯一的写库入口。所有写入以祈愿记录 id 为主键，重复抓取自动去重，
//! 因此「同步」本身是幂等的，不需要额外的对账逻辑。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use gacha_core::{Account, PullRecord};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use thiserror::Error;

const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("数据库错误：{0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("文件系统错误：{0}")]
    Io(#[from] std::io::Error),

    #[error("时间格式无法解析：{0}")]
    Time(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// 账号列表项，用于「历史记录」页面。
#[derive(Debug, Clone, Serialize)]
pub struct AccountRow {
    pub uid: String,
    pub nickname: Option<String>,
    pub region: String,
    pub game_biz: String,
    pub pull_count: i64,
    pub first_time: Option<String>,
    pub last_time: Option<String>,
    pub last_sync_at: Option<String>,
}

/// 应用数据根目录：**可执行文件所在目录下的 `data` 子目录**。
///
/// 不用 `%APPDATA%`、不在代码里写死盘符：谁启动它，数据就跟着谁走，
/// 整个目录拷走即完整迁移（便携式布局）。
/// 需要强制指定位置时设环境变量 `TEYVAT_GACHA_HOME`。
///
/// 注意：`target\release` 下的 CLI 与桌面端可执行文件同目录，因此共用同一个库；
/// debug 产物各自带一份，属预期行为。
pub fn app_home() -> PathBuf {
    if let Some(custom) = std::env::var_os("TEYVAT_GACHA_HOME") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }

    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .map(|dir| dir.join("data"))
        // 取不到 exe 路径时（极少数情况）退回当前工作目录，绝不写用户目录
        .unwrap_or_else(|| PathBuf::from("data"))
}

/// 默认数据库路径：`<app_home>\gacha.db`。
pub fn default_db_path() -> PathBuf {
    app_home().join("gacha.db")
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS accounts (
                uid          TEXT PRIMARY KEY,
                nickname     TEXT,
                region       TEXT NOT NULL,
                game_biz     TEXT NOT NULL,
                created_at   TEXT NOT NULL,
                last_sync_at TEXT
            );

            CREATE TABLE IF NOT EXISTS pulls (
                id         TEXT PRIMARY KEY,
                uid        TEXT NOT NULL,
                gacha_type INTEGER NOT NULL,
                item_id    TEXT NOT NULL,
                item_name  TEXT NOT NULL,
                item_type  TEXT NOT NULL,
                rank_type  INTEGER NOT NULL,
                time       TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_pulls_uid_pool_time
                ON pulls (uid, gacha_type, time);

            CREATE TABLE IF NOT EXISTS sync_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                uid         TEXT NOT NULL,
                started_at  TEXT NOT NULL,
                finished_at TEXT,
                added       INTEGER NOT NULL DEFAULT 0,
                status      TEXT NOT NULL,
                message     TEXT
            );

            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_account(&self, account: &Account) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO accounts (uid, nickname, region, game_biz, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(uid) DO UPDATE SET
                region   = excluded.region,
                game_biz = excluded.game_biz,
                nickname = COALESCE(excluded.nickname, accounts.nickname)
            "#,
            params![
                account.uid,
                account.nickname,
                account.region,
                account.game_biz,
                now_text()
            ],
        )?;
        Ok(())
    }

    pub fn set_nickname(&self, uid: &str, nickname: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE accounts SET nickname = ?1 WHERE uid = ?2",
            params![nickname, uid],
        )?;
        Ok(())
    }

    pub fn mark_synced(&self, uid: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE accounts SET last_sync_at = ?1 WHERE uid = ?2",
            params![now_text(), uid],
        )?;
        Ok(())
    }

    /// 批量写入祈愿记录，返回真正新增的条数。
    pub fn insert_pulls(&mut self, records: &[PullRecord]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut added = 0usize;
        {
            let mut statement = tx.prepare(
                r#"
                INSERT OR IGNORE INTO pulls
                    (id, uid, gacha_type, item_id, item_name, item_type, rank_type, time)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
            )?;
            for record in records {
                let changed = statement.execute(params![
                    record.id,
                    record.uid,
                    record.gacha_type,
                    record.item_id,
                    record.item_name,
                    record.item_type,
                    record.rank_type,
                    record.time.format(TIME_FORMAT).to_string()
                ])?;
                added += changed;
            }
        }
        tx.commit()?;
        Ok(added)
    }

    pub fn load_pulls(&self, uid: &str) -> Result<Vec<PullRecord>> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, uid, gacha_type, item_id, item_name, item_type, rank_type, time
            FROM pulls WHERE uid = ?1
            ORDER BY time ASC, id ASC
            "#,
        )?;
        let rows = statement.query_map(params![uid], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;

        let mut records = Vec::new();
        for row in rows {
            let (id, uid, gacha_type, item_id, item_name, item_type, rank_type, time) = row?;
            let parsed_time = NaiveDateTime::parse_from_str(&time, TIME_FORMAT)
                .map_err(|_| StoreError::Time(time.clone()))?;
            records.push(PullRecord {
                id,
                uid,
                gacha_type: gacha_type as u32,
                item_id,
                item_name,
                item_type,
                rank_type: rank_type as u8,
                time: parsed_time,
            });
        }
        Ok(records)
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountRow>> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT a.uid,
                   a.nickname,
                   a.region,
                   a.game_biz,
                   a.last_sync_at,
                   COUNT(p.id)          AS pull_count,
                   MIN(p.time)          AS first_time,
                   MAX(p.time)          AS last_time
            FROM accounts a
            LEFT JOIN pulls p ON p.uid = a.uid
            GROUP BY a.uid
            ORDER BY last_time DESC, a.created_at DESC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AccountRow {
                uid: row.get(0)?,
                nickname: row.get(1)?,
                region: row.get(2)?,
                game_biz: row.get(3)?,
                last_sync_at: row.get(4)?,
                pull_count: row.get(5)?,
                first_time: row.get(6)?,
                last_time: row.get(7)?,
            })
        })?;

        let mut accounts = Vec::new();
        for row in rows {
            accounts.push(row?);
        }
        Ok(accounts)
    }

    pub fn find_account(&self, uid: &str) -> Result<Option<AccountRow>> {
        Ok(self
            .list_accounts()?
            .into_iter()
            .find(|account| account.uid == uid))
    }

    /// 本地已入库的全部记录 id。
    ///
    /// 增量同步靠它判断「这一页是否已经抓过」：记录只增不改，
    /// 一旦命中已有 id，更早的内容必然也已在库里，可以停止翻页。
    pub fn existing_ids(&self, uid: &str) -> Result<HashSet<String>> {
        let mut statement = self
            .conn
            .prepare("SELECT id FROM pulls WHERE uid = ?1")?;
        let rows = statement.query_map(params![uid], |row| row.get::<_, String>(0))?;

        let mut ids = HashSet::new();
        for row in rows {
            ids.insert(row?);
        }
        Ok(ids)
    }

    /// 全部账号已入库的记录 id，供增量同步在拿到 uid 之前先做判断。
    /// 祈愿记录 id 全局唯一，因此跨账号合并不会误判。
    pub fn all_existing_ids(&self) -> Result<HashSet<String>> {
        let mut statement = self.conn.prepare("SELECT id FROM pulls")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;

        let mut ids = HashSet::new();
        for row in rows {
            ids.insert(row?);
        }
        Ok(ids)
    }

    pub fn count_pulls(&self, uid: &str) -> Result<i64> {
        let count: Option<i64> = self
            .conn
            .query_row("SELECT COUNT(*) FROM pulls WHERE uid = ?1", params![uid], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(count.unwrap_or(0))
    }

    pub fn delete_account(&self, uid: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM pulls WHERE uid = ?1", params![uid])?;
        tx.execute("DELETE FROM sync_log WHERE uid = ?1", params![uid])?;
        tx.execute("DELETE FROM accounts WHERE uid = ?1", params![uid])?;
        tx.commit()?;
        Ok(())
    }

    pub fn start_sync_log(&self, uid: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sync_log (uid, started_at, status) VALUES (?1, ?2, 'running')",
            params![uid, now_text()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn finish_sync_log(
        &self,
        log_id: i64,
        added: usize,
        status: &str,
        message: &str,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE sync_log
            SET finished_at = ?1, added = ?2, status = ?3, message = ?4
            WHERE id = ?5
            "#,
            params![now_text(), added as i64, status, message, log_id],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO settings (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![key, value],
        )?;
        Ok(())
    }
}

fn now_text() -> String {
    chrono::Local::now().naive_local().format(TIME_FORMAT).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u64, gacha_type: u32, name: &str, rank: u8) -> PullRecord {
        PullRecord {
            id: id.to_string(),
            uid: "114709792".to_string(),
            gacha_type,
            item_id: format!("item{id}"),
            item_name: name.to_string(),
            item_type: if rank == 5 { "角色" } else { "武器" }.to_string(),
            rank_type: rank,
            time: NaiveDateTime::parse_from_str("2023-02-13 12:00:00", TIME_FORMAT).unwrap(),
        }
    }

    #[test]
    fn insert_is_idempotent() {
        let mut store = Store::open_in_memory().expect("open");
        let batch = vec![record(1, 301, "纳西妲", 5), record(2, 301, "三星武器", 3)];
        assert_eq!(store.insert_pulls(&batch).expect("first"), 2);
        assert_eq!(store.insert_pulls(&batch).expect("second"), 0);
        assert_eq!(store.count_pulls("114709792").expect("count"), 2);
    }

    #[test]
    fn account_roundtrip_and_delete() {
        let mut store = Store::open_in_memory().expect("open");
        store
            .upsert_account(&Account {
                uid: "114709792".to_string(),
                nickname: None,
                region: "cn_gf01".to_string(),
                game_biz: "hk4e_cn".to_string(),
            })
            .expect("upsert");
        store
            .insert_pulls(&[record(1, 301, "纳西妲", 5)])
            .expect("insert");

        let accounts = store.list_accounts().expect("list");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].pull_count, 1);

        store.delete_account("114709792").expect("delete");
        assert!(store.list_accounts().expect("list").is_empty());
    }

    #[test]
    fn loaded_records_keep_time_order() {
        let mut store = Store::open_in_memory().expect("open");
        let mut older = record(1, 301, "旧", 3);
        older.time = NaiveDateTime::parse_from_str("2022-01-01 08:00:00", TIME_FORMAT).unwrap();
        let newer = record(2, 301, "新", 5);
        store.insert_pulls(&[newer, older]).expect("insert");

        let loaded = store.load_pulls("114709792").expect("load");
        assert_eq!(loaded[0].item_name, "旧");
        assert_eq!(loaded[1].item_name, "新");
    }
}
