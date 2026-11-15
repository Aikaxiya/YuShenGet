//! 极简 HTTP GET，只用于下载公开资源（角色头像）。
//!
//! 与抓取层共用同一套 TLS 配置（Windows 走 SChannel，不引 rustls）。
//! 头像下载属于「锦上添花」，必须短超时，绝不能让慢 CDN 拖住任意线程太久。

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use crate::error::FetchError;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

fn agent_with_timeout(connect_secs: u64, read_secs: u64) -> ureq::Agent {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .ok()
        .map(Arc::new);

    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_secs))
        .timeout(Duration::from_secs(read_secs));

    if let Some(connector) = tls {
        builder = builder.tls_connector(connector);
    }
    builder.build()
}

fn fetch(url: &str, connect_secs: u64, read_secs: u64) -> Result<Vec<u8>, FetchError> {
    let response = agent_with_timeout(connect_secs, read_secs)
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|err| match err {
            ureq::Error::Status(code, _) => {
                FetchError::Rejected(code as i64, format!("HTTP {code}"))
            }
            ureq::Error::Transport(transport) => {
                FetchError::Network(format!("连接失败（{}）", transport.kind()))
            }
        })?;

    let mut buffer = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut buffer)
        .map_err(|err| FetchError::Network(format!("读取响应失败：{err}")))?;

    Ok(buffer)
}

/// 下载二进制资源。失败时只回报错误类型，不把 URL 带进消息。
pub fn get_bytes(url: &str) -> Result<Vec<u8>, FetchError> {
    fetch(url, 8, 20)
}

/// 头像下载专用：短超时。缺图只是显示降级，不值得为它等半分钟。
pub fn get_bytes_fast(url: &str) -> Result<Vec<u8>, FetchError> {
    fetch(url, 3, 5)
}
