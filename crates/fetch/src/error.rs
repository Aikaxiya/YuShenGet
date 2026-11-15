//! 错误类型。所有面向用户的文案都在这里，UI 层直接展示，不再二次拼装。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("链接里没有 authkey。请在游戏内打开「祈愿 → 祈愿记录」，复制整条链接后再试。")]
    MissingAuthKey,

    #[error("authkey 已失效（retcode={0}）。authkey 只有约 24 小时有效期，请重新在游戏内打开祈愿记录获取新链接。")]
    AuthKeyExpired(i64),

    #[error("请求过于频繁（retcode={0}）。请等待 10 分钟后再试，期间不要重复点击查询。")]
    RateLimited(i64),

    #[error("接口拒绝请求（retcode={0}）：{1}")]
    Rejected(i64, String),

    #[error("网络请求失败：{0}")]
    Network(String),

    #[error("响应解析失败：{0}")]
    Decode(String),

    #[error("本地缓存目录不存在：{0}")]
    CacheMissing(String),

    #[error("数据库错误：{0}")]
    Db(String),
}
