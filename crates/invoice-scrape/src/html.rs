//! 官方頁面解析。
//!
//! 目標頁面（皆為靜態 HTML，無需 JavaScript）：
//!
//! - `index.html`         當期一般獎別中獎號碼單
//! - `lastNumber.html`    上一期一般獎別中獎號碼單
//! - `cloudNowNumber.html` 當期雲端發票專屬獎（號碼以 PDF 公布）
//!
//! 這裡刻意不引入 HTML 解析器：頁面結構固定（`<tr><td>獎別</td><td>號碼</td></tr>`），
//! 用一個容錯的小型抽取器即可，也讓建置流程保持零重量級依賴。

use invoice_core::check::Draw;
use invoice_core::civil::Ymd;
use invoice_core::period::Period;

/// 抽取錯誤。
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseError {}

fn err<T>(m: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(m.into()))
}

/// 去除 HTML 標籤並解碼常見實體；`<br>` 轉為換行。
pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'<' {
            // 找結尾 '>'
            let mut j = i + 1;
            while j < b.len() && b[j] != b'>' {
                j += 1;
            }
            let tag = &s[i + 1..j.min(s.len())];
            let name = tag
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if name == "br" {
                out.push('\n');
            }
            i = j + 1;
        } else {
            let ch_len = utf8_len(b[i]);
            let end = (i + ch_len).min(b.len());
            out.push_str(&s[i..end]);
            i = end;
        }
    }
    decode_entities(&out)
}

fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// 解碼 `&nbsp;` `&amp;` `&#NN;` 等實體。
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let tail = &rest[p..];
        let Some(semi) = tail.find(';').filter(|&e| e <= 12) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let ent = &tail[1..semi];
        let decoded = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// 取出所有 `<tr>` 內的 `<td>` 文字（已去標籤）。
pub fn table_rows(html: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(open) = lower[pos..].find("<tr") {
        let start = pos + open;
        let Some(close) = lower[start..].find("</tr>") else {
            break;
        };
        let row_html = &html[start..start + close];
        let mut cells = Vec::new();
        let row_lower = row_html.to_ascii_lowercase();
        let mut cpos = 0;
        while let Some(td) = row_lower[cpos..].find("<td") {
            let tstart = cpos + td;
            let Some(tclose) = row_lower[tstart..].find("</td>") else {
                break;
            };
            let cell = &row_html[tstart..tstart + tclose];
            // 略過 <td ...> 標籤本身
            let after = cell.find('>').map(|i| &cell[i + 1..]).unwrap_or("");
            cells.push(strip_tags(after).trim().to_string());
            cpos = tstart + tclose + 5;
        }
        rows.push(cells);
        pos = start + close + 5;
    }
    rows
}

/// 從文字中抓出指定長度的數字串（`len` 碼）。
pub fn digit_runs(s: &str, len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let run = &s[start..i];
            if run.len() == len {
                out.push(run.to_string());
            }
        } else {
            i += 1;
        }
    }
    out
}

/// 由「中獎號碼單」頁面解析出一期的 [`Draw`]。
pub fn parse_prize_page(html: &str) -> Result<Draw, ParseError> {
    let period = parse_period_title(html)?;
    let rows = table_rows(html);
    if rows.is_empty() {
        return err("頁面裡找不到任何表格列，格式可能已改版");
    }

    let mut special: Option<String> = None;
    let mut grand: Option<String> = None;
    let mut first: Vec<String> = Vec::new();
    let mut extra_sixth: Vec<String> = Vec::new();

    for r in &rows {
        if r.len() < 2 {
            continue;
        }
        let label = r[0].replace(' ', "");
        let body = &r[1];
        match label.as_str() {
            l if l.contains("特別獎") && !l.contains("特獎") => {
                special = digit_runs(body, 8).into_iter().next();
            }
            l if l.contains("特獎") && !l.contains("特別") => {
                grand = digit_runs(body, 8).into_iter().next();
            }
            l if l.contains("頭獎") => {
                first = digit_runs(body, 8);
            }
            l if l.contains("增開") && l.contains("六獎") => {
                extra_sixth = digit_runs(body, 3);
            }
            _ => {}
        }
    }

    let Some(special) = special else {
        return err("解析不到特別獎號碼");
    };
    let Some(grand) = grand else {
        return err("解析不到特獎號碼");
    };
    if first.is_empty() {
        return err("解析不到頭獎號碼");
    }

    Ok(Draw::new(period, special, grand, first, extra_sixth))
}

/// 抽出頁面上所有 `<a href="...">文字</a>`。
pub fn anchors(html: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("<a ") {
        let start = pos + i;
        let Some(end) = lower[start..].find("</a>") else {
            break;
        };
        let seg = &html[start..start + end];
        if let Some(h) = href_of(seg) {
            out.push((h, strip_tags(seg).trim().to_string()));
        }
        pos = start + end + 4;
    }
    out
}

/// 依「頁面本身檔名」定位當前期別。
///
/// 官方每頁的導覽列都會同時列出當期與上一期標題（例：`index.html` 與
/// `lastNumber.html` 的選單長得一樣），因此不能只取第一個符合的標題，
/// 必須挑 `href` 等於本頁檔名的那個連結。
pub fn parse_period_title_for(html: &str, page_rel: &str) -> Result<Period, ParseError> {
    for (href, text) in anchors(html) {
        if href.trim_end_matches('/') == page_rel
            || href.ends_with(page_rel) && !page_rel.is_empty()
        {
            if let Ok(p) = parse_period_title(&text) {
                return Ok(p);
            }
        }
    }
    // 退路：頁面上只出現一次期別標題時，直接取第一個
    let all = anchors(html);
    let mut hits: Vec<Period> = all
        .iter()
        .filter_map(|(_, t)| parse_period_title(t).ok())
        .collect();
    hits.dedup();
    let distinct: Vec<Period> = {
        let mut v = hits.clone();
        v.sort();
        v.dedup();
        v
    };
    match distinct.len() {
        1 => Ok(distinct[0]),
        0 => err("解析不到期別（找不到形如 115年07-08月 的標題）"),
        _ => err(format!(
            "頁面上出現多個期別（{:?}），需指定頁面檔名才能判斷",
            distinct.iter().map(|p| p.label()).collect::<Vec<_>>()
        )),
    }
}

/// 由頁面標題解析期別，例：「115年07-08月中獎號碼單」→ 115 年 07-08 月。
pub fn parse_period_title(html: &str) -> Result<Period, ParseError> {
    let text = strip_tags(html);
    // 找 "NNN年MM-MM月"
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i + 8 < chars.len() {
        // 三位數字 + 年
        if chars[i].is_ascii_digit()
            && chars[i + 1].is_ascii_digit()
            && chars[i + 2].is_ascii_digit()
            && chars[i + 3] == '年'
        {
            let roc: u16 = chars[i..i + 3]
                .iter()
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            let rest: String = chars[i + 4..(i + 9).min(chars.len())].iter().collect();
            // 期待 "MM-MM"
            let b: Vec<char> = rest.chars().collect();
            if b.len() >= 5 && b[2] == '-' {
                let m1: u32 = b[0..2].iter().collect::<String>().parse().unwrap_or(0);
                if matches!(m1, 1 | 3 | 5 | 7 | 9 | 11) {
                    let slot = ((m1 - 1) / 2 + 1) as u8;
                    if let Some(p) = Period::new(roc, slot) {
                        return Ok(p);
                    }
                }
            }
        }
        i += 1;
    }
    err("解析不到期別（找不到形如 115年07-08月 的標題）")
}

/// 解析「領獎期間自 115年10月06日起至 116年01月05日止」。
pub fn parse_redeem_window(html: &str) -> Option<(Ymd, Ymd)> {
    let text = strip_tags(html).replace([' ', '\n', '\t'], "");
    let key = "領獎期間自";
    let i = text.find(key)?;
    let tail = &text[i + key.len()..];
    let j = tail.find('止')?;
    let seg = &tail[..j];
    // 期待 a年a月a日起至b年b月b日
    let parts: Vec<&str> = seg.split("起至").collect();
    if parts.len() != 2 {
        return None;
    }
    let a = parse_roc_date(parts[0])?;
    let b = parse_roc_date(parts[1])?;
    Some((a, b))
}

fn parse_roc_date(s: &str) -> Option<Ymd> {
    let y_end = s.find('年')?;
    let m_end = s.find('月')?;
    let d_end = s.find('日')?;
    let roc: i32 = s[..y_end].parse().ok()?;
    let m: u32 = s[y_end + 3..m_end].parse().ok()?;
    let d: u32 = s[m_end + 3..d_end].parse().ok()?;
    Some(Ymd::new(roc + 1911, m, d))
}

/// 雲端發票專屬獎 PDF 連結。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudLink {
    /// 獎級代碼字母（官方檔名中的 `_AI_X` 最後一碼）。
    pub tier: char,
    /// 檔案路徑（相對 `https://invoice.etax.nat.gov.tw/`）。
    pub href: String,
    /// 是否為「已排序」清單。
    pub sorted: bool,
    /// 錨點文字。
    pub text: String,
}

/// 抽出雲端發票專屬獎頁面上的所有 PDF 連結。
pub fn parse_cloud_links(html: &str) -> Vec<CloudLink> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("<a ") {
        let start = pos + i;
        let Some(end) = lower[start..].find("</a>") else {
            break;
        };
        let seg = &html[start..start + end];
        if let Some(h) = href_of(seg) {
            if h.contains("pdf/") && h.ends_with(".pdf") {
                let text = strip_tags(seg).trim().to_string();
                out.push(CloudLink {
                    tier: tier_of(&h),
                    href: h.trim_start_matches("./").to_string(),
                    sorted: h.contains("_sorted_"),
                    text,
                });
            }
        }
        pos = start + end + 4;
    }
    out
}

fn href_of(seg: &str) -> Option<String> {
    let l = seg.to_ascii_lowercase();
    let i = l.find("href=\"")?;
    let rest = &seg[i + 6..];
    let j = rest.find('"')?;
    Some(rest[..j].to_string())
}

fn tier_of(href: &str) -> char {
    // 例：pdf/20260708_20260925124717_sorted_AI_D.pdf
    let stem = href.rsplit('/').next().unwrap_or("");
    let stem = stem.strip_suffix(".pdf").unwrap_or(stem);
    stem.rsplit("_AI_")
        .next()
        .and_then(|s| s.chars().next())
        .unwrap_or('?')
}

/// 由檔名前綴取期別標記，例：`20260708` + 期別 → 驗證是否相符。
pub fn tag_of(href: &str) -> Option<String> {
    let name = href.rsplit('/').next()?;
    let tag = name.split('_').next()?;
    (tag.len() == 8 && tag.bytes().all(|b| b.is_ascii_digit())).then(|| tag.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    <table>
      <tr><td>獎別</td><td>中獎號碼</td></tr>
      <tr><td>特別獎</td><td>89996565<br>同期統一發票收執聯8位數號碼與特別獎號碼相同者獎金1,000萬元</td></tr>
      <tr><td>特獎</td><td>91098182<br>同期統一發票收執聯8位數號碼與特獎號碼相同者獎金200萬元</td></tr>
      <tr><td>頭獎</td><td>54348835<br> 44991397<br> 06595111<br>同期統一發票收執聯8位數號碼與頭獎號碼相同者獎金20萬元</td></tr>
      <tr><td>二獎</td><td>同期統一發票收執聯末7位數號碼與頭獎中獎號碼末7位相同者各得獎金4萬元</td></tr>
    </table>
    <div>115年07-08月中獎號碼單</div>
    <div>領獎期間自115年10月06日起至116年01月05日止</div>
    "#;

    #[test]
    fn parses_prize_table() {
        let d = parse_prize_page(SAMPLE).unwrap();
        assert_eq!(d.period, Period::new(115, 4).unwrap());
        assert_eq!(d.special, "89996565");
        assert_eq!(d.grand, "91098182");
        assert_eq!(d.first, vec!["54348835", "44991397", "06595111"]);
        assert!(d.extra_sixth.is_empty());
        assert!(d.validate().is_ok());
    }

    #[test]
    fn parses_redeem_window() {
        let (s, e) = parse_redeem_window(SAMPLE).unwrap();
        assert_eq!(s, Ymd::new(2026, 10, 6));
        assert_eq!(e, Ymd::new(2027, 1, 5));
        // 與期別推算一致
        let p = Period::new(115, 4).unwrap();
        assert_eq!(p.redeem_window(), (s, e));
    }

    #[test]
    fn parses_period_title_other_slots() {
        for (t, want) in [
            ("115年05-06月中獎號碼單", Period::new(115, 3).unwrap()),
            ("115年11-12月中獎號碼單", Period::new(115, 6).unwrap()),
            ("113年01-02月中獎號碼單", Period::new(113, 1).unwrap()),
        ] {
            assert_eq!(parse_period_title(t).unwrap(), want);
        }
    }

    #[test]
    fn parses_cloud_links() {
        let html = r#"<a href="pdf/20260708_20260925124717_sorted_AI_D.pdf">五百元獎中獎號碼清單PDF檔(已排序)</a>
                      <a href="pdf/20260708_20260925124559_AI_C.pdf">百萬元獎中獎清單PDF檔(依開獎順序)</a>"#;
        let links = parse_cloud_links(html);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].tier, 'D');
        assert!(links[0].sorted);
        assert_eq!(links[1].tier, 'C');
        assert!(!links[1].sorted);
        assert_eq!(tag_of(&links[0].href).unwrap(), "20260708");
    }

    #[test]
    fn entity_and_tag_handling() {
        assert_eq!(strip_tags("<b>a&amp;b</b>&nbsp;c"), "a&b c");
        assert_eq!(strip_tags("x<br>y"), "x\ny");
    }

    #[test]
    fn picks_period_by_page_name() {
        // 模擬 lastNumber.html：選單同時有當期與上一期，只有 href 對得上的才是本頁
        let html = r#"
        <ul>
          <li><a href="index.html" title="x">115年07-08月中獎號碼單</a></li>
          <li><a class="etw-on" href="lastNumber.html">115年05-06月中獎號碼單</a></li>
        </ul>"#;
        assert_eq!(
            parse_period_title_for(html, "lastNumber.html").unwrap(),
            Period::new(115, 3).unwrap()
        );
        assert_eq!(
            parse_period_title_for(html, "index.html").unwrap(),
            Period::new(115, 4).unwrap()
        );
        // 無法定位時，若只有單一期別才放行
        assert!(parse_period_title_for(html, "unknown.html").is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_prize_page("<html>nothing</html>").is_err());
    }
}
