//! MCP（Model Context Protocol）server：stdio + JSON-RPC 2.0。
//!
//! 讓 ChatGPT Desktop / Codex / Claude 等支援 MCP 的用戶端，直接把
//! 「台灣統一發票對獎」當成一個工具來呼叫。訊息以換行分隔的 JSON 傳輸。
//!
//! 對外提供的工具：
//!
//! | 工具 | 用途 |
//! |---|---|
//! | `list_periods` | 列出資料集內的期別與中獎號碼 |
//! | `check_invoice` | 輸入字軌號碼（或紙本 8 碼）對獎 |
//! | `check_receipt_image` | 上傳發票照片（base64）→ 解 QR → 對獎 |
//! | `data_status` | 目前資料版本與來源 |

use crate::engine;
use crate::store::Store;
use invoice_core::json::{self, Json};
use std::io::{BufRead, Write};

/// 啟動 MCP server（阻塞在 stdin）。
pub fn run(store: Store) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut store = store;

    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req = match json::parse(line) {
            Ok(v) => v,
            Err(e) => {
                let resp = error_response(Json::Null, -32700, &format!("Parse error: {e}"));
                writeln!(stdout, "{resp}").map_err(|e| e.to_string())?;
                stdout.flush().ok();
                continue;
            }
        };

        let id = req.get("id").cloned().unwrap_or(Json::Null);
        let method = req.str_at("method").unwrap_or("").to_string();

        // 通知（沒有 id）不需要回應
        let is_notification = req.get("id").is_none();

        let outcome = dispatch(&mut store, &method, req.get("params"));

        if is_notification {
            continue;
        }

        let resp = match outcome {
            Ok(result) => json::obj(vec![
                ("jsonrpc", json::s("2.0")),
                ("id", id),
                ("result", result),
            ]),
            Err((code, msg)) => error_response(id, code, &msg),
        };
        writeln!(stdout, "{resp}").map_err(|e| e.to_string())?;
        stdout.flush().ok();
    }
    Ok(())
}

fn error_response(id: Json, code: i32, msg: &str) -> Json {
    json::obj(vec![
        ("jsonrpc", json::s("2.0")),
        ("id", id),
        (
            "error",
            json::obj(vec![
                ("code", json::n(code as f64)),
                ("message", json::s(msg)),
            ]),
        ),
    ])
}

fn dispatch(store: &mut Store, method: &str, params: Option<&Json>) -> Result<Json, (i32, String)> {
    match method {
        "initialize" => Ok(json::obj(vec![
            ("protocolVersion", json::s("2024-11-05")),
            (
                "capabilities",
                json::obj(vec![("tools", json::obj(vec![]))]),
            ),
            (
                "serverInfo",
                json::obj(vec![
                    ("name", json::s("tw-invoice-lottery")),
                    ("version", json::s(invoice_core::VERSION)),
                ]),
            ),
        ])),
        "ping" => Ok(json::obj(vec![])),
        "tools/list" => Ok(json::obj(vec![("tools", Json::Arr(tool_defs()))])),
        "tools/call" => {
            let p = params.ok_or((-32602, "缺少 params".to_string()))?;
            let name = p
                .str_at("name")
                .ok_or((-32602, "缺少工具名稱".to_string()))?;
            let empty = json::obj(vec![]);
            let args = p.get("arguments").unwrap_or(&empty);
            let text = call_tool(store, name, args).map_err(|e| (-32603, e))?;
            Ok(json::obj(vec![
                (
                    "content",
                    Json::Arr(vec![json::obj(vec![
                        ("type", json::s("text")),
                        ("text", json::s(text)),
                    ])]),
                ),
                ("isError", Json::Bool(false)),
            ]))
        }
        other => Err((-32601, format!("不支援的方法：{other}"))),
    }
}

fn tool_defs() -> Vec<Json> {
    vec![
        json::obj(vec![
            ("name", json::s("list_periods")),
            (
                "description",
                json::s("列出目前資料庫內所有統一發票期別，以及各期的特別獎、特獎、頭獎中獎號碼。"),
            ),
            (
                "inputSchema",
                json::obj(vec![
                    ("type", json::s("object")),
                    ("properties", json::obj(vec![])),
                ]),
            ),
        ]),
        json::obj(vec![
            ("name", json::s("check_invoice")),
            (
                "description",
                json::s("比對一張統一發票是否中獎。輸入 10 碼字軌（如 AB12345678）或紙本發票的 8 碼數字，可指定期別（如 11504）；未指定則用最新一期。"),
            ),
            (
                "inputSchema",
                json::obj(vec![
                    ("type", json::s("object")),
                    (
                        "properties",
                        json::obj(vec![
                            (
                                "code",
                                json::obj(vec![
                                    ("type", json::s("string")),
                                    ("description", json::s("發票字軌號碼：10 碼含英文字軌，或 8 碼純數字")),
                                ]),
                            ),
                            (
                                "period",
                                json::obj(vec![
                                    ("type", json::s("string")),
                                    ("description", json::s("期別代碼，如 11504 代表 115 年 07-08 月；可省略")),
                                ]),
                            ),
                        ]),
                    ),
                    ("required", Json::Arr(vec![json::s("code")])),
                ]),
            ),
        ]),
        json::obj(vec![
            ("name", json::s("check_receipt_image")),
            (
                "description",
                json::s("上傳電子發票證明聯照片（base64 編碼的 JPEG/PNG/WebP），自動讀取 QR Code 並對獎。"),
            ),
            (
                "inputSchema",
                json::obj(vec![
                    ("type", json::s("object")),
                    (
                        "properties",
                        json::obj(vec![(
                            "image_base64",
                            json::obj(vec![
                                ("type", json::s("string")),
                                ("description", json::s("圖片的 base64 內容（可含 data:image/... 前綴）")),
                            ]),
                        )]),
                    ),
                    ("required", Json::Arr(vec![json::s("image_base64")])),
                ]),
            ),
        ]),
        json::obj(vec![
            ("name", json::s("data_status")),
            (
                "description",
                json::s("回報目前中獎號碼資料的版本、來源與是否已載入雲端發票專屬獎名單。"),
            ),
            (
                "inputSchema",
                json::obj(vec![
                    ("type", json::s("object")),
                    ("properties", json::obj(vec![])),
                ]),
            ),
        ]),
    ]
}

fn call_tool(store: &mut Store, name: &str, args: &Json) -> Result<String, String> {
    match name {
        "list_periods" => {
            let status = store.status();
            Ok(status.to_pretty())
        }
        "check_invoice" => {
            let code = args.str_at("code").ok_or("缺少 code 參數")?;
            let period = args.str_at("period");
            let r = engine::check_code(store, code, period)?;
            Ok(r.to_json(None).to_pretty())
        }
        "check_receipt_image" => {
            let b64 = args.str_at("image_base64").ok_or("缺少 image_base64")?;
            let bytes = base64_decode(b64)?;
            let payloads = invoice_qr::decode_image_bytes(&bytes).map_err(|e| e.to_string())?;
            let parsed = invoice_qr::from_payloads(&payloads).map_err(|e| e.to_string())?;
            let period = parsed.period().code();
            let r = engine::check_code(store, &parsed.code.to_string(), Some(&period))?;
            let mut j = r.to_json(Some(parsed.has_buyer_tax_id()));
            if let Json::Obj(m) = &mut j {
                m.insert(
                    "issue_date".into(),
                    json::s(format!(
                        "{:04}-{:02}-{:02}",
                        parsed.issue_date.y, parsed.issue_date.m, parsed.issue_date.d
                    )),
                );
                m.insert("total_amount".into(), json::n(parsed.total_amount as f64));
            }
            Ok(j.to_pretty())
        }
        "data_status" => Ok(store.status().to_pretty()),
        other => Err(format!("未知工具：{other}")),
    }
}

/// 標準 base64 解碼（可容忍 `data:` 前綴與換行）。
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = match s.find("base64,") {
        Some(i) => &s[i + 7..],
        None => s,
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\n' | b'\r' | b' ' | b'\t' => continue,
            _ => return Err(format!("base64 含非法字元：{}", c as char)),
        };
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGVs\nbG8=").unwrap(), b"hello");
        assert_eq!(
            base64_decode("data:image/png;base64,aGVsbG8=").unwrap(),
            b"hello"
        );
        assert!(base64_decode("!!!").is_err());
    }

    #[test]
    fn initialize_shape() {
        let mut store = crate::testutil::temp_store("mcp_init");
        let r = dispatch(&mut store, "initialize", None).unwrap();
        assert_eq!(r.str_at("protocolVersion").unwrap(), "2024-11-05");
        assert_eq!(
            r.get("serverInfo").unwrap().str_at("name").unwrap(),
            "tw-invoice-lottery"
        );
    }

    #[test]
    fn tools_list_has_core_tools() {
        let mut store = crate::testutil::temp_store("mcp_tools");
        let r = dispatch(&mut store, "tools/list", None).unwrap();
        let names: Vec<&str> = r
            .get("tools")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .map(|t| t.str_at("name").unwrap())
            .collect();
        assert!(names.contains(&"check_invoice"));
        assert!(names.contains(&"check_receipt_image"));
        assert!(names.contains(&"list_periods"));
    }

    #[test]
    fn check_invoice_tool_works() {
        let mut store = crate::testutil::temp_store("mcp_check");
        let args = json::obj(vec![
            ("code", json::s("ZZ89996565")),
            ("period", json::s("11504")),
        ]);
        let text = call_tool(&mut store, "check_invoice", &args).unwrap();
        let v = json::parse(&text).unwrap();
        assert_eq!(v.get("is_winner").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn unknown_method_and_tool() {
        let mut store = crate::testutil::temp_store("mcp_unknown");
        assert!(dispatch(&mut store, "no/such", None).is_err());
        assert!(call_tool(&mut store, "nope", &json::obj(vec![])).is_err());
    }
}
