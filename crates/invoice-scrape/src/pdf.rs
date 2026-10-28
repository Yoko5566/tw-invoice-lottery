//! PDF 文字抽取（針對財政部「雲端發票專屬獎中獎號碼清單」PDF）。
//!
//! 官方 PDF 為文字型（非掃描影像），因此不需要 OCR：
//! 只要把 `/FlateDecode` 壓縮的內容流解壓，依序取出所有字串運算元
//! （`(...)Tj`、`[(..)-2(..)]TJ`、`<hex>`），串接後掃描字軌樣式即可。
//!
//! 一份 500 元獎清單可達 130 MB、兩百萬筆，這裡全程串流處理、不做額外複製。

use flate2::read::ZlibDecoder;
use std::io::Read;

/// PDF 解析錯誤。
#[derive(Debug)]
pub struct PdfError(pub String);

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PdfError {}

/// 從 PDF 位元組取出所有可見文字（已串接）。
pub fn extract_text(pdf: &[u8]) -> Result<String, PdfError> {
    if !pdf.starts_with(b"%PDF") {
        return Err(PdfError("不是 PDF 檔（缺少 %PDF 標頭）".into()));
    }
    let mut out = String::with_capacity(pdf.len() / 4);
    let mut pos = 0usize;
    let mut streams = 0usize;
    let mut inflated_ok = 0usize;

    while let Some(rel) = find(&pdf[pos..], b"stream") {
        let s = pos + rel;
        // 排除 "endstream" 內的 "stream"
        if s >= 3 && &pdf[s - 3..s] == b"end" {
            pos = s + 6;
            continue;
        }
        let mut data_start = s + 6;
        if data_start < pdf.len() && pdf[data_start] == b'\r' {
            data_start += 1;
        }
        if data_start < pdf.len() && pdf[data_start] == b'\n' {
            data_start += 1;
        }
        let Some(rel_end) = find(&pdf[data_start..], b"endstream") else {
            break;
        };
        let data_end = data_start + rel_end;
        let chunk = &pdf[data_start..data_end];
        streams += 1;

        if let Ok(text) = inflate_to_text(chunk) {
            inflated_ok += 1;
            out.push_str(&text);
        }
        pos = data_end + 9;
    }

    if streams == 0 {
        return Err(PdfError("找不到任何內容流（stream）".into()));
    }
    if inflated_ok == 0 {
        return Err(PdfError(format!(
            "{streams} 個內容流都無法解壓，可能不是 FlateDecode 或檔案已加密"
        )));
    }
    Ok(out)
}

fn inflate_to_text(chunk: &[u8]) -> Result<String, PdfError> {
    let mut d = ZlibDecoder::new(chunk);
    let mut buf = Vec::new();
    d.read_to_end(&mut buf)
        .map_err(|e| PdfError(e.to_string()))?;
    Ok(collect_strings(&buf))
}

/// 從已解壓的內容流中收集所有字串運算元。
fn collect_strings(buf: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < buf.len() {
        match buf[i] {
            b'(' => {
                i += 1;
                let mut depth = 1;
                while i < buf.len() && depth > 0 {
                    match buf[i] {
                        b'\\' => {
                            i += 1;
                            if i >= buf.len() {
                                break;
                            }
                            match buf[i] {
                                b'n' | b'r' | b't' => {}
                                b'(' => out.push('('),
                                b')' => out.push(')'),
                                b'\\' => out.push('\\'),
                                b'0'..=b'7' => {
                                    // 八進位轉義，最多三位
                                    let mut v = 0u32;
                                    let mut k = 0;
                                    while k < 3 && i < buf.len() && (b'0'..=b'7').contains(&buf[i])
                                    {
                                        v = v * 8 + u32::from(buf[i] - b'0');
                                        i += 1;
                                        k += 1;
                                    }
                                    if let Some(c) = char::from_u32(v) {
                                        out.push(c);
                                    }
                                    continue;
                                }
                                c => out.push(c as char),
                            }
                            i += 1;
                        }
                        b'(' => {
                            depth += 1;
                            out.push('(');
                            i += 1;
                        }
                        b')' => {
                            depth -= 1;
                            if depth > 0 {
                                out.push(')');
                            }
                            i += 1;
                        }
                        c => {
                            out.push(c as char);
                            i += 1;
                        }
                    }
                }
            }
            b'<' if i + 1 < buf.len() && buf[i + 1] != b'<' => {
                // 十六進位字串
                let Some(rel) = buf[i + 1..].iter().position(|&c| c == b'>') else {
                    break;
                };
                let hex = &buf[i + 1..i + 1 + rel];
                let mut hi: Option<u8> = None;
                for &c in hex {
                    let v = match c {
                        b'0'..=b'9' => c - b'0',
                        b'a'..=b'f' => c - b'a' + 10,
                        b'A'..=b'F' => c - b'A' + 10,
                        _ => continue,
                    };
                    match hi {
                        None => hi = Some(v),
                        Some(h) => {
                            out.push(char::from(h << 4 | v));
                            hi = None;
                        }
                    }
                }
                i += rel + 2;
            }
            b'\n' | b'\r' => {
                out.push('\n');
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// 從抽出的文字掃描統一發票字軌號碼（2 碼大寫英文 + 8 碼數字）。
///
/// 只回傳「前後不是英數字」的完整樣式，避免把相鄰欄位黏在一起。
pub fn extract_codes(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut i = 0usize;
    while i + 10 <= b.len() {
        if b[i].is_ascii_uppercase()
            && b[i + 1].is_ascii_uppercase()
            && b[i + 2..i + 10].iter().all(|c| c.is_ascii_digit())
        {
            let prev_ok = i == 0 || !b[i - 1].is_ascii_alphanumeric();
            let next_ok = i + 10 >= b.len() || !b[i + 10].is_ascii_alphanumeric();
            if prev_ok && next_ok {
                let code = &text[i..i + 10];
                if seen.insert(code.to_string()) {
                    out.push(code.to_string());
                }
                i += 10;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let first = needle[0];
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i] == first && &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_codes_from_text() {
        let text = "CG33998265   CG34952794\nCH04343932 2026";
        let codes = extract_codes(text);
        assert_eq!(codes, vec!["CG33998265", "CG34952794", "CH04343932"]);
    }

    #[test]
    fn dedups_and_rejects_partial() {
        assert_eq!(extract_codes("AB12345678 AB12345678").len(), 1);
        assert!(extract_codes("XAB12345678").is_empty());
        assert!(extract_codes("AB1234567").is_empty());
        assert!(extract_codes("ab12345678").is_empty());
    }

    #[test]
    fn collects_pdf_string_operators() {
        let content = b"BT /F1 9 Tf (CG3399) Tj [(8265)-120(CG34952794)] TJ ET";
        let s = collect_strings(content);
        assert!(s.contains("CG33998265"));
        assert!(s.contains("CG34952794"));
    }

    #[test]
    fn handles_escapes_and_hex() {
        let s = collect_strings(b"(a\\(b\\)c\\101) <414243>");
        assert!(s.contains("a(b)cA"));
        assert!(s.contains("ABC"));
    }

    #[test]
    fn rejects_non_pdf() {
        assert!(extract_text(b"hello world").is_err());
    }
}
