//! 查詢引擎：把使用者輸入（字軌號碼 / 發票照片）轉成結構化對獎結果。
//!
//! CLI、本機網頁 App、MCP server 三種入口共用這支，確保行為一致。

use crate::store::Store;
use invoice_core::check::Hit;
use invoice_core::code::InvoiceCode;
use invoice_core::dataset::CloudOutcome;
use invoice_core::json::{self, Json};
use invoice_core::period::Period;
use invoice_core::Ymd;

/// 一次查詢的完整結果。
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub code: InvoiceCode,
    pub period: Period,
    pub hits: Vec<Hit>,
    pub cloud: CloudOutcome,
    pub redeem_start: Ymd,
    pub redeem_end: Ymd,
    /// 今日（UTC）距領獎截止還有幾天；負數表示已逾期。
    pub days_left: i64,
    pub notes: Vec<String>,
}

impl CheckResult {
    pub fn best(&self) -> Option<&Hit> {
        self.hits.first()
    }

    pub fn is_winner(&self) -> bool {
        !self.hits.is_empty()
    }
}

/// 解析期別：明確指定優先，否則用資料集中最新的一期。
pub fn resolve_period(store: &Store, period_token: Option<&str>) -> Result<Period, String> {
    match period_token {
        Some(t) if !t.trim().is_empty() => {
            let t = t.trim();
            if let Some(p) = Period::from_code(t) {
                return Ok(p);
            }
            // 也接受 "115年07-08月"
            let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() >= 5 {
                if let Some(p) = Period::from_code(&digits[..5]) {
                    return Ok(p);
                }
            }
            Err(format!("無法辨識期別：{t}（可用 11504 或 115年07-08月）"))
        }
        _ => store
            .dataset
            .latest()
            .map(|d| d.period)
            .ok_or_else(|| "資料集中沒有任何期別".to_string()),
    }
}

/// 對獎主流程。
pub fn check_code(
    store: &mut Store,
    code_str: &str,
    period_token: Option<&str>,
) -> Result<CheckResult, String> {
    let code = parse_code(code_str)?;
    let period = resolve_period(store, period_token)?;

    let cloud_ready = store.ensure_cloud(&period.code());
    let (hits, cloud) = store.dataset.check(code, period, store.cloud.as_ref())?;

    let (redeem_start, redeem_end) = period.redeem_window();
    let today = today_utc();
    let days_left = redeem_end.to_days() - today.to_days();

    let mut notes = Vec::new();
    if !cloud_ready {
        notes.push("未載入本期雲端發票專屬獎名單，無法判斷雲端專屬獎（可在「資料」頁下載）".into());
    }
    if let Some(h) = hits.first() {
        if h.prize.subject_to_income_tax() {
            notes.push(format!(
                "{}屬三獎（含）以上，依規定扣繳 20% 所得稅，實領約 NT${}",
                h.prize.label(),
                h.prize.net_amount()
            ));
        }
        if h.prize.subject_to_stamp_tax() {
            notes.push(
                "五獎及雲端發票專屬獎 500 元以上需繳 0.4% 印花稅；已於電子發票平台設定領獎帳戶者免繳"
                    .into(),
            );
        }
        if hits.len() > 1 {
            notes.push("同時對中多個獎別時，依規定以兌領一個獎項為限（此處列示最高獎項）".into());
        }
        if days_left < 0 {
            notes.push(format!(
                "領獎期限已過（截止 {:04}-{:02}-{:02}），逾期不得領獎",
                redeem_end.y, redeem_end.m, redeem_end.d
            ));
        } else if days_left <= 30 {
            notes.push(format!(
                "距領獎截止僅剩 {days_left} 天（{:04}-{:02}-{:02} 止）",
                redeem_end.y, redeem_end.m, redeem_end.d
            ));
        }
    }

    Ok(CheckResult {
        code,
        period,
        hits,
        cloud,
        redeem_start,
        redeem_end,
        days_left,
        notes,
    })
}

fn parse_code(s: &str) -> Result<InvoiceCode, String> {
    let t = s.trim();
    let has_alpha = t.chars().any(|c| c.is_ascii_alphabetic());
    if has_alpha {
        InvoiceCode::parse(t).map_err(|e| e.to_string())
    } else {
        // 紙本發票只有 8 碼數字
        InvoiceCode::from_digits(t).map_err(|e| e.to_string())
    }
}

/// 今天（UTC）。
pub fn today_utc() -> Ymd {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ymd::from_days(secs.div_euclid(86_400))
}

impl CheckResult {
    /// 轉成 API 回應 JSON。
    pub fn to_json(&self, has_buyer_tax_id: Option<bool>) -> Json {
        let hits: Vec<Json> = self
            .hits
            .iter()
            .map(|h| {
                json::obj(vec![
                    ("prize", json::s(h.prize.label())),
                    ("amount", json::n(h.prize.amount() as f64)),
                    ("net_amount", json::n(h.prize.net_amount() as f64)),
                    ("matched", json::s(&h.matched)),
                    ("detail", json::s(&h.detail)),
                ])
            })
            .collect();

        let mut notes = self.notes.clone();
        if has_buyer_tax_id == Some(true) && self.is_winner() {
            notes.insert(
                0,
                "此張發票打了買受人統一編號（營業人），依規定不得領獎".to_string(),
            );
        }

        let cloud_state = match self.cloud {
            CloudOutcome::NotLoaded => "not_loaded",
            CloudOutcome::Miss => "miss",
            CloudOutcome::Hit(_) => "hit",
        };

        json::obj(vec![
            ("ok", Json::Bool(true)),
            ("code", json::s(self.code.to_string())),
            ("period", json::s(self.period.label())),
            ("period_code", json::s(self.period.code())),
            ("draw_date", json::s(fmt(self.period.draw_date()))),
            ("redeem_start", json::s(fmt(self.redeem_start))),
            ("redeem_end", json::s(fmt(self.redeem_end))),
            ("days_left", json::n(self.days_left as f64)),
            ("is_winner", Json::Bool(self.is_winner())),
            (
                "best_prize",
                match self.best() {
                    Some(h) => json::s(h.prize.label()),
                    None => Json::Null,
                },
            ),
            (
                "best_amount",
                match self.best() {
                    Some(h) => json::n(h.prize.amount() as f64),
                    None => json::n(0.0),
                },
            ),
            (
                "winner_tax_id",
                Json::Bool(has_buyer_tax_id.unwrap_or(false)),
            ),
            ("cloud_state", json::s(cloud_state)),
            ("hits", Json::Arr(hits)),
            ("notes", Json::Arr(notes.into_iter().map(json::s).collect())),
        ])
    }
}

fn fmt(d: Ymd) -> String {
    format!("{:04}-{:02}-{:02}", d.y, d.m, d.d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EMBEDDED_DATASET;
    use invoice_core::dataset::Dataset;
    use invoice_core::rules::Prize;

    fn store_with(dir: &std::path::Path) -> Store {
        let ds = Dataset::from_json(&json::parse(EMBEDDED_DATASET).unwrap()).unwrap();
        std::fs::create_dir_all(dir).unwrap();
        Store {
            dir: dir.to_path_buf(),
            dataset: ds,
            cloud: None,
            manifest_url: "http://127.0.0.1/none".into(),
            dataset_origin: "test".into(),
        }
    }

    #[test]
    fn finds_special_prize() {
        let dir = std::env::temp_dir().join("twiv_test_engine_a");
        let mut s = store_with(&dir);
        let r = check_code(&mut s, "ZZ89996565", Some("11504")).unwrap();
        assert!(r.is_winner());
        assert_eq!(r.best().unwrap().prize, Prize::Special);
        assert_eq!(r.period, Period::new(115, 4).unwrap());
    }

    #[test]
    fn accepts_paper_digits() {
        let dir = std::env::temp_dir().join("twiv_test_engine_b");
        let mut s = store_with(&dir);
        let r = check_code(&mut s, "89996565", Some("11504")).unwrap();
        assert_eq!(r.best().unwrap().prize, Prize::Special);
    }

    #[test]
    fn defaults_to_latest_period() {
        let dir = std::env::temp_dir().join("twiv_test_engine_c");
        let mut s = store_with(&dir);
        let r = check_code(&mut s, "AB00000000", None).unwrap();
        assert_eq!(r.period, s.dataset.latest().unwrap().period);
    }

    #[test]
    fn rejects_bad_input() {
        let dir = std::env::temp_dir().join("twiv_test_engine_d");
        let mut s = store_with(&dir);
        assert!(check_code(&mut s, "ABC", None).is_err());
        assert!(check_code(&mut s, "AB12345678", Some("9999")).is_err());
    }

    #[test]
    fn json_shape() {
        let dir = std::env::temp_dir().join("twiv_test_engine_e");
        let mut s = store_with(&dir);
        let r = check_code(&mut s, "AB54348835", Some("11504")).unwrap();
        let j = r.to_json(Some(false));
        assert_eq!(j.str_at("period_code").unwrap(), "11504");
        assert_eq!(j.get("is_winner").unwrap().as_bool(), Some(true));
        assert_eq!(j.get("best_amount").unwrap().as_f64(), Some(200_000.0));
    }
}
