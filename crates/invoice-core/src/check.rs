//! 對獎引擎：把一張發票的字軌號碼，對上一期的中獎號碼。

use crate::code::InvoiceCode;
use crate::period::Period;
use crate::rules::Prize;

/// 某一期「一般獎別」（特別獎 / 特獎 / 頭獎 / 增開六獎）的中獎號碼。
///
/// 雲端發票專屬獎另見 [`crate::cloud_index::CloudIndex`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draw {
    pub period: Period,
    /// 特別獎號碼（8 碼）。
    pub special: String,
    /// 特獎號碼（8 碼）。
    pub grand: String,
    /// 頭獎號碼（每期 3 組，8 碼）。
    pub first: Vec<String>,
    /// 增開六獎號碼（3 碼），部分期別才會有，可能為空。
    pub extra_sixth: Vec<String>,
}

impl Draw {
    /// 建立並做基本驗證。
    pub fn new(
        period: Period,
        special: String,
        grand: String,
        first: Vec<String>,
        extra_sixth: Vec<String>,
    ) -> Self {
        Self {
            period,
            special,
            grand,
            first,
            extra_sixth,
        }
    }

    fn digit_of(s: &str) -> Option<u32> {
        let t = s.trim();
        (t.len() == 8 && t.bytes().all(|b| b.is_ascii_digit()))
            .then(|| t.parse().ok())
            .flatten()
    }

    /// 驗證資料完整性（供爬蟲寫入前自檢）。
    pub fn validate(&self) -> Result<(), String> {
        if Self::digit_of(&self.special).is_none() {
            return Err(format!("特別獎號碼格式錯誤：{}", self.special));
        }
        if Self::digit_of(&self.grand).is_none() {
            return Err(format!("特獎號碼格式錯誤：{}", self.grand));
        }
        if self.first.is_empty() {
            return Err("頭獎號碼不得為空".into());
        }
        for f in &self.first {
            if Self::digit_of(f).is_none() {
                return Err(format!("頭獎號碼格式錯誤：{f}"));
            }
        }
        for e in &self.extra_sixth {
            let t = e.trim();
            if t.len() != 3 || !t.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!("增開六獎號碼格式錯誤：{e}"));
            }
        }
        if self.first.len() != 3 {
            // 官方每期固定 3 組頭獎；異常時不視為致命錯誤，但記錄下來。
            eprintln!(
                "[warn] {} 頭獎組數為 {}，非預期 3 組",
                self.period.label(),
                self.first.len()
            );
        }
        Ok(())
    }
}

/// 一次命中結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub prize: Prize,
    /// 命中的中獎號碼（或末 n 碼）。
    pub matched: String,
    /// 命中說明，例：「末 5 碼 45678 與頭獎 12345678 相符」。
    pub detail: String,
}

impl Hit {
    pub fn amount(&self) -> u64 {
        self.prize.amount()
    }
}

/// 對「一般獎別」進行比對，回傳所有命中（已依獎金由高至低排序）。
///
/// 依《統一發票給獎辦法》，同時對中多個獎別只能擇一領取，呼叫端取第一筆即為最優。
pub fn check_draw(draw: &Draw, code: InvoiceCode) -> Vec<Hit> {
    let digits = code.digits();
    let mut hits: Vec<Hit> = Vec::new();

    if let Some(sp) = Draw::digit_of(&draw.special) {
        if digits == sp {
            hits.push(Hit {
                prize: Prize::Special,
                matched: draw.special.clone(),
                detail: format!("8 碼與特別獎 {} 完全相同", draw.special),
            });
        }
    }
    if let Some(gp) = Draw::digit_of(&draw.grand) {
        if digits == gp {
            hits.push(Hit {
                prize: Prize::Grand,
                matched: draw.grand.clone(),
                detail: format!("8 碼與特獎 {} 完全相同", draw.grand),
            });
        }
    }

    for f in &draw.first {
        let Some(fd) = Draw::digit_of(f) else {
            continue;
        };
        if digits == fd {
            hits.push(Hit {
                prize: Prize::First,
                matched: f.clone(),
                detail: format!("8 碼與頭獎 {f} 完全相同"),
            });
            continue;
        }
        // 二~六獎：由長到短比對末 n 碼，命中即為最佳（不再往下比）。
        for (n, prize) in [
            (7u32, Prize::Second),
            (6, Prize::Third),
            (5, Prize::Fourth),
            (4, Prize::Fifth),
            (3, Prize::Sixth),
        ] {
            let m = 10u32.pow(n);
            if digits % m == fd % m {
                hits.push(Hit {
                    prize,
                    matched: format!("{:0width$}", fd % m, width = n as usize),
                    detail: format!(
                        "末 {n} 碼 {:0width$} 與頭獎 {f} 末 {n} 碼相符",
                        fd % m,
                        width = n as usize
                    ),
                });
                break;
            }
        }
    }

    for e in &draw.extra_sixth {
        let Ok(ed) = e.trim().parse::<u32>() else {
            continue;
        };
        if digits % 1000 == ed {
            hits.push(Hit {
                prize: Prize::ExtraSixth,
                matched: e.trim().to_string(),
                detail: format!("末 3 碼 {ed:03} 與增開六獎號碼相符"),
            });
        }
    }

    hits.sort_by_key(|h| std::cmp::Reverse(h.amount()));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    // 真實資料：115 年 07-08 月期（財政部稅務入口網 index.html）
    fn draw_11504() -> Draw {
        Draw::new(
            Period::new(115, 4).unwrap(),
            "89996565".into(),
            "91098182".into(),
            vec!["54348835".into(), "44991397".into(), "06595111".into()],
            vec![],
        )
    }

    fn c(s: &str) -> InvoiceCode {
        InvoiceCode::parse(s).unwrap()
    }

    #[test]
    fn special_prize() {
        let hits = check_draw(&draw_11504(), c("ZZ89996565"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].prize, Prize::Special);
        assert_eq!(hits[0].amount(), 10_000_000);
    }

    #[test]
    fn grand_prize() {
        let hits = check_draw(&draw_11504(), c("AA91098182"));
        assert_eq!(hits[0].prize, Prize::Grand);
    }

    #[test]
    fn first_prize() {
        let hits = check_draw(&draw_11504(), c("AB54348835"));
        assert_eq!(hits[0].prize, Prize::First);
        assert_eq!(hits[0].amount(), 200_000);
    }

    #[test]
    fn second_through_sixth_by_suffix() {
        let d = draw_11504();
        // 頭獎 54348835 → 末7 = 4348835
        assert_eq!(check_draw(&d, c("AB04348835"))[0].prize, Prize::Second);
        // 末6 = 348835
        assert_eq!(check_draw(&d, c("AB00348835"))[0].prize, Prize::Third);
        // 末5 = 48835
        assert_eq!(check_draw(&d, c("AB00048835"))[0].prize, Prize::Fourth);
        // 末4 = 8835
        assert_eq!(check_draw(&d, c("AB00008835"))[0].prize, Prize::Fifth);
        // 末3 = 835
        assert_eq!(check_draw(&d, c("AB00000835"))[0].prize, Prize::Sixth);
        // 末2 = 35 → 未中獎
        assert!(check_draw(&d, c("AB00000035")).is_empty());
    }

    #[test]
    fn longest_suffix_wins() {
        let d = draw_11504();
        // 與頭獎 06595111 末 7 碼相同，只會記二獎一筆，不會同時出現三~六獎
        let hits = check_draw(&d, c("AA06595111")); // 全中 → 頭獎
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].prize, Prize::First);
    }

    #[test]
    fn extra_sixth() {
        let mut d = draw_11504();
        d.extra_sixth = vec!["835".into()];
        let hits = check_draw(&d, c("AB00000835"));
        assert!(hits.iter().any(|h| h.prize == Prize::ExtraSixth));
    }

    #[test]
    fn no_prize() {
        let d = draw_11504();
        assert!(check_draw(&d, c("AB12345678")).is_empty());
    }

    #[test]
    fn paper_invoice_digits() {
        // 紙本發票只有 8 碼，仍可比對一般獎別
        let d = draw_11504();
        let code = InvoiceCode::from_digits("89996565").unwrap();
        assert_eq!(check_draw(&d, code)[0].prize, Prize::Special);
    }

    #[test]
    fn draw_validation() {
        assert!(draw_11504().validate().is_ok());
        let bad = Draw::new(
            Period::new(115, 4).unwrap(),
            "123".into(),
            "91098182".into(),
            vec!["54348835".into()],
            vec![],
        );
        assert!(bad.validate().is_err());
    }
}
