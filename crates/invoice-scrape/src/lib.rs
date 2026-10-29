//! 官方資料抓取與索引建置。
//!
//! # 資料來源
//!
//! | 資料 | 來源 |
//! |---|---|
//! | 當期一般獎別中獎號碼 | `https://invoice.etax.nat.gov.tw/index.html` |
//! | 上一期一般獎別中獎號碼 | `https://invoice.etax.nat.gov.tw/lastNumber.html` |
//! | 雲端發票專屬獎名單 | `https://invoice.etax.nat.gov.tw/cloudNowNumber.html`（PDF） |
//!
//! 這些頁面都是靜態 HTML，不需要 JavaScript、不需要登入，也沒有 API key。
//! 抓取時務必遵守 `robots.txt`、控制頻率（每次開獎週期抓一次即可）。

pub mod html;
pub mod net;
pub mod pdf;

use invoice_core::civil::Ymd;
use invoice_core::cloud_index::{CloudIndex, CloudTier};
use invoice_core::code::InvoiceCode;
use invoice_core::dataset::Dataset;
use invoice_core::json::{self, Json};
use invoice_core::period::Period;
use invoice_core::rules::Prize;
use std::collections::BTreeMap;

/// 官方站台根網址。
pub const BASE: &str = "https://invoice.etax.nat.gov.tw/";

/// 抓取錯誤。
#[derive(Debug)]
pub struct FetchError(pub String);

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for FetchError {}

fn fe<T>(m: impl Into<String>) -> Result<T, FetchError> {
    Err(FetchError(m.into()))
}

/// 一次抓取結果（含官方頁面自報的領獎期間，用於交叉驗證）。
#[derive(Debug, Clone)]
pub struct DrawPage {
    pub draw: invoice_core::check::Draw,
    pub redeem_window: Option<(Ymd, Ymd)>,
    pub source: String,
}

/// 抓取並解析某一頁中獎號碼單。
pub fn fetch_draw_page(rel: &str) -> Result<DrawPage, FetchError> {
    let url = format!("{BASE}{rel}");
    let html = net::get_text(&url).map_err(|e| FetchError(e.to_string()))?;
    // 官方每頁導覽列都列出當期與上一期，必須用本頁檔名定位期別
    let page_rel = rel.rsplit('/').next().unwrap_or(rel);
    let period = html::parse_period_title_for(&html, page_rel)
        .map_err(|e| FetchError(format!("{url}: {e}")))?;
    let mut draw = html::parse_prize_page(&html).map_err(|e| FetchError(format!("{url}: {e}")))?;
    if draw.period != period {
        draw.period = period;
    }
    draw.validate().map_err(FetchError)?;
    let redeem_window = html::parse_redeem_window(&html);

    // 交叉驗證：官方頁面寫的領獎期間，必須與我們推算的一致。
    if let Some((s, e)) = redeem_window {
        let computed = draw.period.redeem_window();
        if computed != (s, e) {
            return fe(format!(
                "{} 領獎期間交叉驗證失敗：官方 {s:?}~{e:?}，推算 {:?}~{:?}",
                draw.period.label(),
                computed.0,
                computed.1
            ));
        }
    }

    Ok(DrawPage {
        draw,
        redeem_window,
        source: url,
    })
}

/// 抓取當期與上一期的一般獎別號碼。
pub fn fetch_current_and_last() -> Result<Vec<DrawPage>, FetchError> {
    let mut out = Vec::new();
    for rel in ["index.html", "lastNumber.html"] {
        match fetch_draw_page(rel) {
            Ok(p) => out.push(p),
            Err(e) => {
                // 上一期頁面在極少數情況下會是空的，不應讓整批失敗
                if rel == "lastNumber.html" {
                    eprintln!("[warn] 略過 lastNumber.html：{e}");
                } else {
                    return Err(e);
                }
            }
        }
    }
    if out.is_empty() {
        return fe("兩期都抓不到中獎號碼");
    }
    Ok(out)
}

/// 單一獎級的抓取統計。
#[derive(Debug, Clone)]
pub struct TierStat {
    pub prize: Prize,
    pub count: u32,
    pub pdf_bytes: usize,
    pub index_bytes: usize,
}

/// 雲端發票專屬獎抓取結果。
#[derive(Debug, Clone)]
pub struct CloudFetch {
    pub period: Period,
    pub index: CloudIndex,
    pub stats: Vec<TierStat>,
    pub sources: Vec<String>,
}

/// 獎級代碼 → 獎別。
fn tier_prize(c: char) -> Option<Prize> {
    Some(match c {
        'C' => Prize::CloudMillion,
        'B' => Prize::Cloud2000,
        'E' => Prize::Cloud800,
        'D' => Prize::Cloud500,
        _ => return None,
    })
}

/// 抓取雲端發票專屬獎名單並建立壓縮索引。
///
/// 只下載「已排序」版本（`_sorted_`），因為建立差分索引本來就需要排序；
/// 未排序版本檔案更大且無助於查詢。
pub fn fetch_cloud_index(page_rel: &str) -> Result<CloudFetch, FetchError> {
    let url = format!("{BASE}{page_rel}");
    let page = net::get_text(&url).map_err(|e| FetchError(e.to_string()))?;
    let page_rel = page_rel.rsplit('/').next().unwrap_or(page_rel);

    let period = html::parse_period_title_for(&page, page_rel)
        .map_err(|e| FetchError(format!("{url}: 無法判斷期別：{e}")))?;

    let links = html::parse_cloud_links(&page);
    let mut by_tier: BTreeMap<char, html::CloudLink> = BTreeMap::new();
    for l in links {
        if l.sorted {
            by_tier.entry(l.tier).or_insert(l);
        }
    }
    if by_tier.is_empty() {
        return fe("頁面上找不到任何『已排序』的雲端發票專屬獎 PDF 連結");
    }

    let mut index = CloudIndex::default();
    let mut stats = Vec::new();
    let mut sources = Vec::new();

    for (code, link) in by_tier {
        let Some(prize) = tier_prize(code) else {
            eprintln!("[warn] 未知的獎級代碼 AI_{code}，略過（{}）", link.href);
            continue;
        };
        let pdf_url = format!("{BASE}{}", link.href);
        // 500 元獎清單可達 130 MB 以上，上限放寬到 512 MB
        let bytes = net::get_bytes(&pdf_url, 512 * 1024 * 1024)
            .map_err(|e| FetchError(format!("{pdf_url}: {e}")))?;
        let text = pdf::extract_text(&bytes).map_err(|e| FetchError(format!("{pdf_url}: {e}")))?;
        let codes = pdf::extract_codes(&text);
        if codes.is_empty() {
            return fe(format!("{pdf_url}: 解析不到任何字軌號碼，PDF 格式可能已變"));
        }

        let mut raw = Vec::with_capacity(codes.len());
        for c in &codes {
            match InvoiceCode::parse(c) {
                Ok(v) => raw.push(v.to_raw()),
                Err(e) => eprintln!("[warn] {} 字軌無效（{e}），略過", c),
            }
        }
        let tier = CloudTier::build_from_unsorted(prize, raw);
        stats.push(TierStat {
            prize,
            count: tier.count,
            pdf_bytes: bytes.len(),
            index_bytes: tier.size_bytes(),
        });
        sources.push(pdf_url);
        eprintln!(
            "  · {} : {} 筆（PDF {:.1} MB → 索引 {:.2} MB）",
            prize.label(),
            tier.count,
            bytes.len() as f64 / 1_048_576.0,
            tier.size_bytes() as f64 / 1_048_576.0
        );
        index.add(tier);
    }

    Ok(CloudFetch {
        period,
        index,
        stats,
        sources,
    })
}

/// 由抓取結果組出資料集。
pub fn build_dataset(pages: &[DrawPage]) -> Dataset {
    let mut ds = Dataset {
        generated_at: now_iso8601(),
        source: format!("{BASE}index.html"),
        ..Default::default()
    };
    for p in pages {
        ds.upsert(p.draw.clone());
    }
    ds
}

/// 確認抓取結果包含「截至指定日期已開獎的最新一期」及其上一期。
///
/// 這是排程的失敗閘門：官方頁面改版或抓取器只拿到舊資料時，不允許
/// 靜默發布。
pub fn validate_dataset_freshness(dataset: &Dataset, today: Ymd) -> Result<(), FetchError> {
    let expected = Period::latest_drawn_on(today);
    let actual = dataset.latest().map(|d| d.period);
    if actual != Some(expected) {
        return fe(format!(
            "資料不是最新一期：預期 {}（{}），實際 {}",
            expected.label(),
            expected.code(),
            actual
                .map(|p| format!("{}（{}）", p.label(), p.code()))
                .unwrap_or_else(|| "無資料".to_string())
        ));
    }
    let previous = expected.prev();
    if dataset.by_period(previous).is_none() {
        return fe(format!(
            "資料缺少上一期 {}（{}），拒絕發布不完整資料",
            previous.label(),
            previous.code()
        ));
    }
    Ok(())
}

/// 系統目前 UTC 日期。
pub fn today_utc() -> Ymd {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() as i64).div_euclid(86_400))
        .unwrap_or(0);
    Ymd::from_days(days)
}

/// SHA-256（十六進位小寫）。
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// 目前時間（UTC ISO-8601）。不引入時間 crate，直接讀系統時鐘。
pub fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let d = Ymd::from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        d.y,
        d.m,
        d.d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// 產生給 App 用的更新清單。
pub fn build_manifest(
    dataset: &Dataset,
    dataset_url: &str,
    dataset_bytes: &[u8],
    cloud: Option<(&CloudFetch, &str, &[u8])>,
    base_version: &str,
) -> Json {
    let cloud_json = match cloud {
        None => Json::Arr(vec![]),
        Some((c, url, bytes)) => Json::Arr(vec![json::obj(vec![
            ("period", json::s(c.period.code())),
            ("label", json::s(c.period.label())),
            ("url", json::s(url)),
            ("bytes", json::n(bytes.len() as f64)),
            ("sha256", json::s(sha256_hex(bytes))),
            (
                "counts",
                Json::Obj(
                    c.stats
                        .iter()
                        .map(|s| (s.prize.label().to_string(), json::n(s.count as f64)))
                        .collect(),
                ),
            ),
        ])]),
    };

    json::obj(vec![
        ("schema", json::n(1.0)),
        ("app_version", json::s(base_version)),
        ("generated_at", json::s(&dataset.generated_at)),
        (
            "period_latest",
            json::s(
                dataset
                    .latest()
                    .map(|d| d.period.label())
                    .unwrap_or_default(),
            ),
        ),
        ("dataset_url", json::s(dataset_url)),
        ("dataset_bytes", json::n(dataset_bytes.len() as f64)),
        ("dataset_sha256", json::s(sha256_hex(dataset_bytes))),
        ("cloud", cloud_json),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_reference() {
        // 標準測試向量
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn iso8601_shape() {
        let t = now_iso8601();
        assert_eq!(t.len(), 20);
        assert!(t.ends_with('Z'));
        assert_eq!(&t[4..5], "-");
    }

    #[test]
    fn manifest_contains_hash() {
        let mut ds = Dataset::default();
        ds.upsert(invoice_core::check::Draw::new(
            Period::new(115, 4).unwrap(),
            "89996565".into(),
            "91098182".into(),
            vec!["54348835".into(), "44991397".into(), "06595111".into()],
            vec![],
        ));
        let bytes = ds.to_json().to_string().into_bytes();
        let m = build_manifest(&ds, "https://example.com/d.json", &bytes, None, "0.1.0");
        assert_eq!(m.str_at("dataset_sha256").unwrap().len(), 64);
        assert_eq!(m.str_at("period_latest").unwrap(), "115年07-08月");
    }

    #[test]
    fn freshness_check_rejects_stale_or_incomplete_data() {
        let current = Period::new(115, 4).unwrap();
        let previous = current.prev();
        let make = |period| {
            invoice_core::check::Draw::new(
                period,
                "89996565".into(),
                "91098182".into(),
                vec!["54348835".into(), "44991397".into(), "06595111".into()],
                vec![],
            )
        };

        let mut good = Dataset::default();
        good.upsert(make(current));
        good.upsert(make(previous));
        assert!(validate_dataset_freshness(&good, Ymd::new(2026, 10, 28)).is_ok());

        let mut stale = Dataset::default();
        stale.upsert(make(previous));
        stale.upsert(make(previous.prev()));
        assert!(validate_dataset_freshness(&stale, Ymd::new(2026, 10, 28))
            .unwrap_err()
            .0
            .contains("11504"));

        let mut incomplete = Dataset::default();
        incomplete.upsert(make(current));
        assert!(
            validate_dataset_freshness(&incomplete, Ymd::new(2026, 10, 28))
                .unwrap_err()
                .0
                .contains("上一期")
        );
    }

    #[test]
    #[ignore = "需要網路"]
    fn fetches_real_pages() {
        let pages = fetch_current_and_last().unwrap();
        assert!(pages.len() >= 2);
        assert!(pages[0].draw.validate().is_ok());
        assert!(pages[0].redeem_window.is_some());
    }

    #[test]
    #[ignore = "需要網路，會下載數百 MB"]
    fn fetches_real_cloud_index() {
        let c = fetch_cloud_index("cloudNowNumber.html").unwrap();
        assert!(c.index.total() > 1000);
    }
}
