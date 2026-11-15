//! 祈愿记录抓取客户端。
//!
//! 三条硬约束，任何一条破都会触发风控：
//! 1. **串行**：绝不允许并发请求。
//! 2. **限速**：每次请求之间至少间隔 400ms。
//! 3. **游标翻页**：用 `end_id` 而不是 `page`，记录是倒序的。

use std::cell::RefCell;
use std::collections::HashSet;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use gacha_core::PullRecord;
use serde::Deserialize;

use crate::error::FetchError;
use crate::url::GachaUrl;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const REFERER: &str = "https://webstatic.mihoyo.com/";
pub const PAGE_SIZE: u32 = 20;
/// 单池抓取上限，防止接口异常时无限翻页。
const MAX_RECORDS_PER_POOL: usize = 20_000;

#[derive(Debug, Deserialize)]
struct ApiEnvelope {
    retcode: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<ApiData>,
}

#[derive(Debug, Deserialize)]
struct ApiData {
    #[serde(default)]
    list: Vec<ApiRecord>,
}

#[derive(Debug, Deserialize)]
struct ApiRecord {
    #[serde(default)]
    id: String,
    #[serde(default)]
    uid: String,
    #[serde(default)]
    gacha_type: String,
    #[serde(default)]
    item_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    item_type: String,
    #[serde(default)]
    rank_type: String,
    #[serde(default)]
    time: String,
}

impl ApiRecord {
    fn into_record(self) -> Option<PullRecord> {
        let time = NaiveDateTime::parse_from_str(&self.time, "%Y-%m-%d %H:%M:%S").ok()?;
        let rank_type = self.rank_type.parse::<u8>().ok()?;
        let gacha_type = self.gacha_type.parse::<u32>().ok()?;
        if self.id.is_empty() {
            return None;
        }
        Some(PullRecord {
            id: self.id,
            uid: self.uid,
            gacha_type,
            item_id: self.item_id,
            item_name: self.name,
            item_type: self.item_type,
            rank_type,
            time,
        })
    }
}

fn head_of(text: &str) -> String {
    text.chars().take(120).collect()
}

fn classify_retcode(code: i64, message: &str) -> FetchError {
    match code {
        -100 | -101 | -109 | -1101 => FetchError::AuthKeyExpired(code),
        -110 | -1301 => FetchError::RateLimited(code),
        _ => FetchError::Rejected(code, message.to_string()),
    }
}

/// 单池抓取结果。
pub struct FetchOutcome {
    pub records: Vec<PullRecord>,
    /// 翻页次数，用于日志与「这次是不是增量命中」的判断。
    pub pages: u32,
    /// 是否因为命中本地已有记录而提前停止。
    pub stopped_early: bool,
}

pub struct GachaClient {
    agent: ureq::Agent,
    url: GachaUrl,
    request_delay: Duration,
    last_request_at: RefCell<Option<Instant>>,
}

impl GachaClient {
    pub fn new(url: GachaUrl) -> Self {
        // 显式装配 SChannel 连接器：ureq 只认 `tls`(rustls) 特性来生成默认配置，
        // 单开 native-tls 会得到一个「拒绝一切 https」的空实现。
        let tls = native_tls::TlsConnector::builder()
            .build()
            .map(std::sync::Arc::new);

        let mut builder = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(25));

        if let Ok(connector) = tls {
            builder = builder.tls_connector(connector);
        }

        GachaClient {
            agent: builder.build(),
            url,
            request_delay: Duration::from_millis(400),
            last_request_at: RefCell::new(None),
        }
    }

    pub fn url(&self) -> &GachaUrl {
        &self.url
    }

    fn throttle(&self) {
        if let Some(last) = *self.last_request_at.borrow() {
            let elapsed = last.elapsed();
            if elapsed < self.request_delay {
                sleep(self.request_delay - elapsed);
            }
        }
        *self.last_request_at.borrow_mut() = Some(Instant::now());
    }

    fn build_request_url(&self, gacha_type: u32, end_id: &str, size: u32) -> String {
        format!(
            "{base}/gacha_info/api/getGachaLog\
             ?authkey_ver={authkey_ver}&sign_type={sign_type}&auth_appid=webview_gacha\
             &init_type={gacha_type}&game_biz={game_biz}&region={region}&lang={lang}\
             &gacha_type={gacha_type}&size={size}&end_id={end_id}&authkey={authkey}",
            base = self.url.api_base(),
            authkey_ver = self.url.authkey_ver,
            sign_type = self.url.sign_type,
            gacha_type = gacha_type,
            game_biz = self.url.game_biz,
            region = self.url.region,
            lang = self.url.lang,
            size = size,
            end_id = end_id,
            authkey = self.url.authkey,
        )
    }

    /// 抓取单页（最多 20 条）。
    pub fn fetch_page(
        &self,
        gacha_type: u32,
        end_id: &str,
        size: u32,
    ) -> Result<Vec<PullRecord>, FetchError> {
        self.throttle();
        let request_url = self.build_request_url(gacha_type, end_id, size);

        let body = match self
            .agent
            .get(&request_url)
            .set("User-Agent", USER_AGENT)
            .set("Referer", REFERER)
            .call()
        {
            Ok(response) => response
                .into_string()
                .map_err(|err| FetchError::Network(err.to_string()))?,
            Err(ureq::Error::Status(code, response)) => {
                let text = response.into_string().unwrap_or_default();
                if let Ok(envelope) = serde_json::from_str::<ApiEnvelope>(&text) {
                    return Err(classify_retcode(envelope.retcode, &envelope.message));
                }
                return Err(FetchError::Rejected(
                    code as i64,
                    format!("HTTP {code}，可能是链接所属区服与账号不匹配"),
                ));
            }
            Err(ureq::Error::Transport(err)) => {
                // 绝不能用 err.to_string()：ureq 的 Transport 错误会把完整请求 URL
                // 拼进消息，而 URL 里带着 authkey，等于把凭据写进日志。
                return Err(FetchError::Network(format!(
                    "无法连接服务器（{}），请检查网络或代理设置",
                    err.kind()
                )));
            }
        };

        let envelope: ApiEnvelope = serde_json::from_str(&body).map_err(|err| {
            FetchError::Decode(format!("{err}；响应开头：{}", head_of(&body)))
        })?;

        if envelope.retcode != 0 {
            return Err(classify_retcode(envelope.retcode, &envelope.message));
        }

        let list = envelope.data.map(|data| data.list).unwrap_or_default();
        Ok(list.into_iter().filter_map(ApiRecord::into_record).collect())
    }

    /// 抓取整池历史（全量）。`on_progress` 每次翻页后回调当前累计条数。
    pub fn fetch_pool<F>(&self, gacha_type: u32, on_progress: F) -> Result<Vec<PullRecord>, FetchError>
    where
        F: FnMut(u32),
    {
        Ok(self
            .fetch_pool_incremental(gacha_type, &HashSet::new(), on_progress)?
            .records)
    }

    /// 增量抓取：祈愿记录只增不改，页面按时间倒序返回。
    ///
    /// 因此只要某一页里出现「本地已有的记录 id」，说明这一页之后的内容全都入库过了，
    /// 立刻停止翻页。首次同步（`known` 为空）时退化为全量抓取。
    ///
    /// 这样日常同步通常只需 1–2 个请求，而不是几十上百个，既快又避开风控。
    pub fn fetch_pool_incremental<F>(
        &self,
        gacha_type: u32,
        known: &HashSet<String>,
        mut on_progress: F,
    ) -> Result<FetchOutcome, FetchError>
    where
        F: FnMut(u32),
    {
        let mut end_id = "0".to_string();
        let mut collected: Vec<PullRecord> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut pages: u32 = 0;
        let mut stopped_early = false;

        loop {
            let batch = self.fetch_page(gacha_type, &end_id, PAGE_SIZE)?;
            if batch.is_empty() {
                break;
            }
            pages += 1;

            let next_end_id = batch.last().map(|record| record.id.clone());
            let mut added = 0usize;
            let mut hit_known = false;

            for record in batch {
                if known.contains(&record.id) {
                    // 命中旧记录：更早的内容一定也已经在库里
                    hit_known = true;
                    continue;
                }
                if seen.insert(record.id.clone()) {
                    collected.push(record);
                    added += 1;
                }
            }

            on_progress(collected.len() as u32);

            if hit_known {
                stopped_early = true;
                break;
            }

            // 一整页都是本轮的重复数据说明游标没推进，立刻停，避免死循环。
            if added == 0 {
                break;
            }

            match next_end_id {
                Some(id) => {
                    end_id = id;
                }
                None => {
                    break;
                }
            }

            if collected.len() >= MAX_RECORDS_PER_POOL {
                break;
            }
        }

        Ok(FetchOutcome {
            records: collected,
            pages,
            stopped_early,
        })
    }

    /// 探测候选池类型中哪些真实存在。用于确定「千星奇域」的 gacha_type。
    pub fn probe_pool_types(&self, candidates: &[u32]) -> Vec<(u32, usize)> {
        let mut hits = Vec::new();
        for &gacha_type in candidates {
            if let Ok(page) = self.fetch_page(gacha_type, "0", 1) {
                if !page.is_empty() {
                    hits.push((gacha_type, page.len()));
                }
            }
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retcode_mapping_covers_expiry_and_rate_limit() {
        assert!(matches!(
            classify_retcode(-101, ""),
            FetchError::AuthKeyExpired(-101)
        ));
        assert!(matches!(
            classify_retcode(-110, ""),
            FetchError::RateLimited(-110)
        ));
        assert!(matches!(
            classify_retcode(-1, "boom"),
            FetchError::Rejected(-1, _)
        ));
    }

    #[test]
    fn request_url_contains_cursor_and_authkey() {
        let url = GachaUrl::parse("authkey=KEY&game_biz=hk4e_cn&region=cn_gf01").expect("parse");
        let client = GachaClient::new(url);
        let built = client.build_request_url(301, "12345", 20);
        assert!(built.contains("end_id=12345"));
        assert!(built.contains("gacha_type=301"));
        assert!(built.contains("size=20"));
        assert!(built.starts_with("https://public-operation-hk4e.mihoyo.com/"));
    }
}
