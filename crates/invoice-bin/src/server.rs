//! 本機 HTTP 服務：內嵌前端 + JSON API。
//!
//! 為什麼自己寫 HTTP/1.1 而不引入框架：
//! 這個 App 只需要 6 條路由、單一使用者、只綁 `127.0.0.1`，
//! 用 axum/actix 會讓單一執行檔多出數 MB 與數十個依賴。
//! 這裡用 `std::net::TcpListener` + 執行緒，維持「一個小檔案就是整個應用」。

use crate::engine;
use crate::store::Store;
use invoice_core::json::{self, Json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

const INDEX_HTML: &str = include_str!("../../../web/index.html");
const APP_JS: &str = include_str!("../../../web/app.js");
const STYLE_CSS: &str = include_str!("../../../web/style.css");

/// 上傳圖片大小上限（32 MB，足以涵蓋手機原圖）。
const MAX_BODY: usize = 32 * 1024 * 1024;

/// 啟動服務，回傳實際綁定的埠號。
pub fn serve(store: Store, port: u16) -> Result<u16, String> {
    let listener = bind_with_fallback(port)?;
    let actual = listener.local_addr().map_err(|e| e.to_string())?.port();
    let shared = Arc::new(Mutex::new(store));
    let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    println!("http://127.0.0.1:{actual}");
    std::io::stdout().flush().ok();

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                use std::sync::atomic::Ordering;
                if active.fetch_add(1, Ordering::SeqCst) >= 8 {
                    active.fetch_sub(1, Ordering::SeqCst);
                    continue;
                }
                let active = Arc::clone(&active);
                let st = Arc::clone(&shared);
                std::thread::spawn(move || {
                    struct ConnectionGuard(Arc<std::sync::atomic::AtomicUsize>);
                    impl Drop for ConnectionGuard {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                    let _guard = ConnectionGuard(active);
                    if let Err(e) = handle(s, st) {
                        eprintln!("[conn] {e}");
                    }
                });
            }
            Err(e) => eprintln!("[accept] {e}"),
        }
    }
    Ok(actual)
}

fn bind_with_fallback(port: u16) -> Result<TcpListener, String> {
    for p in port..port.saturating_add(20) {
        if let Ok(l) = TcpListener::bind(("127.0.0.1", p)) {
            return Ok(l);
        }
    }
    Err(format!("連接埠 {port}~{} 都被占用", port + 19))
}

struct Request {
    method: String,
    path: String,
    query: String,
    content_type: String,
    body: Vec<u8>,
}

fn handle(mut stream: TcpStream, store: Arc<Mutex<Store>>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut line = String::new();
    if reader.by_ref().take(8193).read_line(&mut line)? == 0 {
        return Ok(());
    }
    if line.len() > 8192 {
        return json_error(&mut stream, 400, "請求行過長");
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };

    let mut content_length = 0usize;
    let mut content_type = String::new();
    let mut host = String::new();
    let mut origin = None;
    let mut headers_size = 0;
    let mut has_content_length = false;
    loop {
        let mut h = String::new();
        if reader.by_ref().take(8193).read_line(&mut h)? == 0 {
            break;
        }
        headers_size += h.len();
        if h.len() > 8192 || headers_size > 16384 {
            return json_error(&mut stream, 400, "標頭過長");
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();
            match k.as_str() {
                "content-length" => {
                    if has_content_length {
                        return json_error(&mut stream, 400, "重複長度標頭");
                    }
                    has_content_length = true;
                    content_length = match v.parse() {
                        Ok(n) => n,
                        Err(_) => return json_error(&mut stream, 400, "無效長度"),
                    };
                }
                "content-type" => content_type = v.to_string(),
                "host" => {
                    if !host.is_empty() {
                        return json_error(&mut stream, 400, "重複 Host");
                    }
                    host = v.to_string();
                }
                "origin" => origin = Some(v.to_string()),
                "transfer-encoding" => return json_error(&mut stream, 400, "不支援串流編碼"),
                _ => {}
            }
        }
    }

    if !trusted_local_request(&host, origin.as_deref(), stream.local_addr()?.port()) {
        return json_error(&mut stream, 403, "只接受本機同來源請求");
    }

    if content_length > MAX_BODY {
        return json_error(&mut stream, 413, "檔案過大");
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    let req = Request {
        method,
        path,
        query,
        content_type,
        body,
    };
    route(&mut stream, req, store)
}

fn route(stream: &mut TcpStream, req: Request, store: Arc<Mutex<Store>>) -> std::io::Result<()> {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => respond(
            stream,
            200,
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes(),
        ),
        ("GET", "/app.js") => respond(
            stream,
            200,
            "text/javascript; charset=utf-8",
            APP_JS.as_bytes(),
        ),
        ("GET", "/style.css") => {
            respond(stream, 200, "text/css; charset=utf-8", STYLE_CSS.as_bytes())
        }
        ("GET", "/api/status") => {
            let st = store.lock().unwrap();
            json_response(stream, 200, &st.status())
        }
        ("GET", "/api/check") => {
            let params = parse_query(&req.query);
            let code = params.get("code").cloned().unwrap_or_default();
            let period = params.get("period").cloned();
            run_check(stream, &store, &code, period.as_deref())
        }
        ("POST", "/api/check") => {
            let text = String::from_utf8_lossy(&req.body).to_string();
            match json::parse(&text) {
                Ok(v) => {
                    let code = v.str_at("code").unwrap_or_default().to_string();
                    let period = v.str_at("period").map(str::to_string);
                    run_check(stream, &store, &code, period.as_deref())
                }
                Err(e) => json_error(stream, 400, &format!("請求格式錯誤：{e}")),
            }
        }
        ("POST", "/api/scan") => run_scan(stream, &store, &req.body, &req.content_type),
        ("POST", "/api/update") => {
            let mut st = store.lock().unwrap();
            match st.update_dataset() {
                Ok(r) => json_response(
                    stream,
                    200,
                    &json::obj(vec![
                        ("ok", Json::Bool(true)),
                        ("latest", json::s(r.latest)),
                        ("bytes", json::n(r.bytes as f64)),
                        ("sha256", json::s(r.sha256)),
                    ]),
                ),
                Err(e) => json_error(stream, 502, &e),
            }
        }
        ("POST", "/api/cloud") => {
            let mut st = store.lock().unwrap();
            let code = st
                .dataset
                .latest()
                .map(|d| d.period.code())
                .unwrap_or_default();
            match st.update_cloud(&code) {
                Ok(r) => json_response(
                    stream,
                    200,
                    &json::obj(vec![
                        ("ok", Json::Bool(true)),
                        ("detail", json::s(r.latest)),
                        ("bytes", json::n(r.bytes as f64)),
                    ]),
                ),
                Err(e) => json_error(stream, 502, &e),
            }
        }
        _ => json_error(stream, 404, "找不到這個路徑"),
    }
}

fn run_check(
    stream: &mut TcpStream,
    store: &Arc<Mutex<Store>>,
    code: &str,
    period: Option<&str>,
) -> std::io::Result<()> {
    let mut st = store.lock().unwrap();
    match engine::check_code(&mut st, code, period) {
        Ok(r) => json_response(stream, 200, &r.to_json(None)),
        Err(e) => json_error(stream, 400, &e),
    }
}

fn run_scan(
    stream: &mut TcpStream,
    store: &Arc<Mutex<Store>>,
    body: &[u8],
    ctype: &str,
) -> std::io::Result<()> {
    run_scan_typed(stream, store, body, ctype)
}

fn run_scan_typed(
    stream: &mut TcpStream,
    store: &Arc<Mutex<Store>>,
    body: &[u8],
    content_type: &str,
) -> std::io::Result<()> {
    if body.is_empty() {
        return json_error(stream, 400, "沒有收到圖片");
    }
    if !content_type.starts_with("image/") && content_type != "application/octet-stream" {
        return json_error(stream, 415, "只接受圖片上傳（image/*）");
    }
    let qr = match invoice_qr::decode_image_bytes(body) {
        Ok(p) => p,
        Err(e) => return json_error(stream, 422, &e.to_string()),
    };
    let parsed = match invoice_qr::from_payloads(&qr) {
        Ok(p) => p,
        Err(e) => {
            return json_response(
                stream,
                422,
                &json::obj(vec![
                    ("ok", Json::Bool(false)),
                    ("error", json::s(e.to_string())),
                    ("payloads", Json::Arr(qr.into_iter().map(json::s).collect())),
                ]),
            )
        }
    };

    let period_token = parsed.period().code();
    let mut st = store.lock().unwrap();
    match engine::check_code(&mut st, &parsed.code.to_string(), Some(&period_token)) {
        Ok(r) => {
            let mut j = r.to_json(Some(parsed.has_buyer_tax_id()));
            if let Json::Obj(m) = &mut j {
                m.insert(
                    "issue_date".into(),
                    json::s(format!(
                        "{:04}-{:02}-{:02}",
                        parsed.issue_date.y, parsed.issue_date.m, parsed.issue_date.d
                    )),
                );
                m.insert("random_code".into(), json::s(&parsed.random_code));
                m.insert("total_amount".into(), json::n(parsed.total_amount as f64));
                m.insert("seller_tax_id".into(), json::s(&parsed.seller_tax_id));
            }
            json_response(stream, 200, &j)
        }
        Err(e) => json_error(stream, 400, &e),
    }
}

fn parse_query(q: &str) -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    for pair in q.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            m.insert(k.to_string(), url_decode(v));
        }
    }
    m
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let h = (b[i + 1] as char).to_digit(16);
                let l = (b[i + 2] as char).to_digit(16);
                match (h, l) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn json_response(stream: &mut TcpStream, status: u16, v: &Json) -> std::io::Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        v.to_string().as_bytes(),
    )
}

fn json_error(stream: &mut TcpStream, status: u16, msg: &str) -> std::io::Result<()> {
    let body = json::obj(vec![("ok", Json::Bool(false)), ("error", json::s(msg))]);
    json_response(stream, status, &body)
}

fn respond(stream: &mut TcpStream, status: u16, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        502 => "Bad Gateway",
        _ => "OK",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {ctype}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\n\
         X-Frame-Options: DENY\r\n\
         Content-Security-Policy: default-src 'self'; img-src 'self' blob:; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'\r\n\
         Permissions-Policy: camera=(), microphone=(), geolocation=()\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn trusted_local_request(host: &str, origin: Option<&str>, port: u16) -> bool {
    let expected = format!("127.0.0.1:{port}");
    host == expected && origin.is_none_or(|o| o == format!("http://{expected}"))
}

/// 用系統預設瀏覽器開啟網址。
pub fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_dns_rebinding_and_cross_site_requests() {
        assert!(trusted_local_request(
            "127.0.0.1:8787",
            Some("http://127.0.0.1:8787"),
            8787
        ));
        assert!(!trusted_local_request("evil.example:8787", None, 8787));
        assert!(!trusted_local_request(
            "127.0.0.1:8787",
            Some("https://evil.example"),
            8787
        ));
        assert!(!trusted_local_request("127.0.0.1:8787", Some("null"), 8787));
    }

    #[test]
    fn decodes_query() {
        let m = parse_query("code=AB12345678&period=11504");
        assert_eq!(m.get("code").unwrap(), "AB12345678");
        assert_eq!(m.get("period").unwrap(), "11504");
        assert_eq!(url_decode("%E4%B8%AD%E6%96%87"), "中文");
        assert_eq!(url_decode("a+b"), "a b");
    }

    #[test]
    fn bound_assets_are_non_empty() {
        assert!(INDEX_HTML.contains("<!doctype") || INDEX_HTML.contains("<!DOCTYPE"));
        assert!(APP_JS.len() > 500);
        assert!(STYLE_CSS.len() > 500);
    }
}
