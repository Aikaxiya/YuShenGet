//! 祈愿分析链接解析。
//!
//! 兼容三种输入：
//! 1. 网页版祈愿记录页地址（参数在 `?` 后，尾部带 `#/log` 片段）
//! 2. 直接粘贴的 `getGachaLog?...` 接口地址
//! 3. 只有查询串的裸参数
//!
//! **关键约束：`authkey` 必须保留原始百分号编码，绝不能解码后再拼回去。**
//! authkey 是 base64，含 `+ / =`，原始链接里写作 `%2B %2F %3D`；
//! 一旦解码成裸字符再拼回查询串，`+` 会被服务端当成空格，key 直接被破坏，
//! 接口就会返回「authkey 失效」——浏览器能请求而程序不行，正是这个原因。
//!
//! 注意：authkey 等同于账号凭据，本模块只做内存解析，绝不落盘、不日志输出。

use url::Url;

use crate::error::FetchError;

#[derive(Debug, Clone)]
pub struct GachaUrl {
    /// 原始（未解码）的 authkey，直接拼进请求 URL。
    pub authkey: String,
    pub region: String,
    pub game_biz: String,
    pub lang: String,
    pub authkey_ver: String,
    pub sign_type: String,
    /// 原始主机名，仅用于诊断，不参与请求。
    pub source_host: String,
}

/// 按 `&`/`=` 切分查询串，**不做百分号解码**。
fn raw_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once('=') {
            Some((key, value)) => (key.to_string(), value.to_string()),
            None => (part.to_string(), String::new()),
        })
        .collect()
}

/// 从原始输入里取查询串：`?` 之后、`#` 之前；若参数写在 `#` 片段里的 `?` 之后，也一并收集。
fn collect_raw_pairs(input: &str) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = Vec::new();

    let before_fragment = input.split('#').next().unwrap_or("");
    if let Some((_, query)) = before_fragment.split_once('?') {
        pairs.extend(raw_pairs(query));
    }

    if let Some((_, fragment)) = input.split_once('#') {
        if let Some((_, query)) = fragment.split_once('?') {
            pairs.extend(raw_pairs(query));
        }
    }

    pairs
}

impl GachaUrl {
    pub fn parse(input: &str) -> Result<Self, FetchError> {
        let cleaned = input
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string();

        if cleaned.is_empty() {
            return Err(FetchError::MissingAuthKey);
        }

        let full = if cleaned.starts_with("http://") || cleaned.starts_with("https://") {
            cleaned.clone()
        } else if cleaned.contains("authkey=") {
            format!(
                "https://public-operation-hk4e.mihoyo.com/gacha_info/api/getGachaLog?{cleaned}"
            )
        } else {
            return Err(FetchError::MissingAuthKey);
        };

        let parsed = Url::parse(&full)
            .map_err(|err| FetchError::Decode(format!("链接格式无法解析：{err}")))?;

        let pairs = collect_raw_pairs(&full);
        let find = |key: &str| -> Option<String> {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };

        let authkey = match find("authkey") {
            Some(value) if !value.is_empty() => value,
            _ => return Err(FetchError::MissingAuthKey),
        };

        let game_biz = find("game_biz").unwrap_or_else(|| "hk4e_cn".to_string());
        let region = find("region").unwrap_or_else(|| {
            if game_biz == "hk4e_global" {
                "os_asia".to_string()
            } else {
                "cn_gf01".to_string()
            }
        });

        Ok(GachaUrl {
            authkey,
            region,
            game_biz,
            lang: find("lang").unwrap_or_else(|| "zh-cn".to_string()),
            authkey_ver: find("authkey_ver").unwrap_or_else(|| "1".to_string()),
            sign_type: find("sign_type").unwrap_or_else(|| "2".to_string()),
            source_host: parsed.host_str().unwrap_or_default().to_string(),
        })
    }

    /// 按区服选择接口主机。选错主机一律 403，这是最常见的失败原因。
    pub fn api_base(&self) -> &'static str {
        if self.game_biz == "hk4e_global" {
            "https://public-operation-hk4e-sg.hoyoverse.com"
        } else {
            "https://public-operation-hk4e.mihoyo.com"
        }
    }

    /// 脱敏后的 authkey，只用于日志与调试展示。
    /// 注意入参是未解码的原始值，`%2F` 之类的转义序列本身也要遮掉。
    pub fn masked_authkey(&self) -> String {
        let len = self.authkey.chars().count();
        if len <= 8 {
            return "****".to_string();
        }
        let head: String = self.authkey.chars().take(4).collect();
        format!("{head}****")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_web_page_url_with_fragment() {
        let input = "https://webstatic.mihoyo.com/hk4e/event/e20190909gacha/index.html?authkey_ver=1&sign_type=2&auth_appid=webview_gacha&authkey=ABC123XYZ&game_biz=hk4e_cn&lang=zh-cn&region=cn_gf01#/log";
        let url = GachaUrl::parse(input).expect("should parse");
        assert_eq!(url.authkey, "ABC123XYZ");
        assert_eq!(url.region, "cn_gf01");
        assert_eq!(url.game_biz, "hk4e_cn");
        assert_eq!(url.api_base(), "https://public-operation-hk4e.mihoyo.com");
    }

    #[test]
    fn global_account_uses_overseas_host() {
        let input = "https://public-operation-hk4e-sg.hoyoverse.com/gacha_info/api/getGachaLog?authkey=KEY&game_biz=hk4e_global&region=os_asia&lang=en";
        let url = GachaUrl::parse(input).expect("should parse");
        assert_eq!(url.api_base(), "https://public-operation-hk4e-sg.hoyoverse.com");
        assert_eq!(url.lang, "en");
    }

    #[test]
    fn bare_query_string_is_accepted() {
        let url = GachaUrl::parse("authkey=KEY&game_biz=hk4e_cn").expect("should parse");
        assert_eq!(url.authkey, "KEY");
        assert_eq!(url.region, "cn_gf01");
    }

    #[test]
    fn missing_authkey_is_rejected() {
        assert!(matches!(
            GachaUrl::parse("https://example.com/foo"),
            Err(FetchError::MissingAuthKey)
        ));
    }

    #[test]
    fn authkey_is_masked_for_display() {
        let url = GachaUrl::parse("authkey=ABCDEFGHIJK&game_biz=hk4e_cn").expect("should parse");
        assert_eq!(url.masked_authkey(), "ABCD****");
    }

    /// 回归用例：authkey 里的 %2B/%2F/%3D 必须原样保留。
    /// 之前解码后拼回 URL，`+` 被服务端当空格，接口报 authkey 失效。
    #[test]
    fn authkey_percent_encoding_is_preserved() {
        let input = "https://public-operation-hk4e.mihoyo.com/gacha_info/api/getGachaLog?authkey_ver=1&authkey=Ab%2Bcd%2Fef%3Dg&game_biz=hk4e_cn&region=cn_gf01";
        let url = GachaUrl::parse(input).expect("should parse");
        assert_eq!(url.authkey, "Ab%2Bcd%2Fef%3Dg");
    }

    #[test]
    fn raw_plus_in_authkey_is_not_turned_into_space() {
        let url = GachaUrl::parse("authkey=a+b&game_biz=hk4e_cn").expect("should parse");
        assert_eq!(url.authkey, "a+b");
    }
}
