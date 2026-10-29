//! 最小 HTTPS 取用層（包一層 ureq，加上 UA、逾時與重試）。

use std::io::Read;
use std::time::Duration;

const UA: &str = concat!(
    "tw-invoice-lottery/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Yoko5566/tw-invoice-lottery)"
);

/// 取用錯誤。
#[derive(Debug)]
pub enum NetError {
    Http(String),
    Io(String),
    TooLarge(usize),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Http(m) => write!(f, "HTTP 失敗：{m}"),
            NetError::Io(m) => write!(f, "IO 失敗：{m}"),
            NetError::TooLarge(n) => write!(f, "回應超過上限（{n} bytes）"),
        }
    }
}

impl std::error::Error for NetError {}

/// 取得網址內容（純文字）。
pub fn get_text(url: &str) -> Result<String, NetError> {
    let bytes = get_bytes(url, 32 * 1024 * 1024)?;
    String::from_utf8(bytes).map_err(|e| NetError::Io(format!("非 UTF-8 內容：{e}")))
}

/// 取得網址內容（二進位），`max` 為位元組上限。
pub fn get_bytes(url: &str, max: usize) -> Result<Vec<u8>, NetError> {
    let mut last_err = String::new();
    for attempt in 0..3u32 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(600 * u64::from(attempt)));
        }
        match try_get(url, max) {
            Ok(v) => return Ok(v),
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(NetError::Http(last_err))
}

fn try_get(url: &str, max: usize) -> Result<Vec<u8>, NetError> {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(120))
        .user_agent(UA)
        .build();

    let resp = agent
        .get(url)
        .call()
        .map_err(|e| NetError::Http(format!("{url}: {e}")))?;

    let mut buf = Vec::new();
    let mut reader = resp.into_reader().take(max as u64 + 1);
    reader
        .read_to_end(&mut buf)
        .map_err(|e| NetError::Io(e.to_string()))?;
    if buf.len() > max {
        return Err(NetError::TooLarge(buf.len()));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "需要網路"]
    fn fetches_official_page() {
        let html = get_text("https://invoice.etax.nat.gov.tw/index.html").unwrap();
        assert!(html.contains("特別獎"));
    }
}
