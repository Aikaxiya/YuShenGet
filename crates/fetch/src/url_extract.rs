//! 从任意文本/二进制里提取祈愿记录链接。
//!
//! 来源不再是游戏本地缓存，而是外部取址工具（HoYoGet 等）的 stdout。
//! 工具输出格式不可控，因此这里按「扫字符串」处理：先找 `getGachaLog`，
//! 再向前回溯到最近的 `https://`，向后吃到非法字符为止。

fn is_url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'_'
                | b'.'
                | b'~'
                | b':'
                | b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b'%'
        )
}

/// 提取所有包含 `getGachaLog` 的完整 URL，保持出现顺序。
pub fn extract_gacha_urls(bytes: &[u8]) -> Vec<String> {
    const NEEDLE: &[u8] = b"getGachaLog";
    let mut urls = Vec::new();
    let mut cursor = 0usize;

    while cursor + NEEDLE.len() <= bytes.len() {
        let found = bytes[cursor..]
            .windows(NEEDLE.len())
            .position(|window| window == NEEDLE);
        let hit = match found {
            Some(offset) => cursor + offset,
            None => break,
        };

        // 向前找最近的 https:// 起点
        let search_start = hit.saturating_sub(8192);
        let window = &bytes[search_start..hit];
        let scheme = b"https://";
        let mut start_abs = None;
        let mut index = 0usize;
        while index + scheme.len() <= window.len() {
            if &window[index..index + scheme.len()] == scheme {
                start_abs = Some(search_start + index);
            }
            index += 1;
        }

        if let Some(start) = start_abs {
            let mut end = hit;
            while end < bytes.len() && is_url_byte(bytes[end]) {
                end += 1;
            }
            urls.push(String::from_utf8_lossy(&bytes[start..end]).to_string());
            cursor = end;
        } else {
            cursor = hit + NEEDLE.len();
        }
    }

    urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_url_embedded_in_noise() {
        let mut payload = vec![0u8, 1, 2, 255, b'x'];
        payload.extend_from_slice(
            b"https://public-operation-hk4e.mihoyo.com/gacha_info/api/getGachaLog?authkey=KEY&gacha_type=301&page=1&end_id=0",
        );
        payload.extend_from_slice(&[0u8, 7, 9]);
        let urls = extract_gacha_urls(&payload);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].contains("authkey=KEY"));
    }

    #[test]
    fn extracts_multiple_urls_in_order() {
        let text = b"first https://a.com/gacha_info/api/getGachaLog?authkey=ONE&end_id=0 \
                     second https://b.com/gacha_info/api/getGachaLog?authkey=TWO&end_id=0";
        let urls = extract_gacha_urls(text);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("authkey=ONE"));
        assert!(urls[1].contains("authkey=TWO"));
    }

    #[test]
    fn ignores_text_without_gacha_url() {
        assert!(extract_gacha_urls(b"no url here https://example.com/foo").is_empty());
    }
}
