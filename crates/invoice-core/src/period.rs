//! 統一發票「期別」模型。
//!
//! 台灣統一發票每兩個月一期，全年六期：
//! 01-02、03-04、05-06、07-08、09-10、11-12 月。
//! 每逢單月 25 日開獎；領獎期間自開獎日之次月 6 日起三個月。
//!
//! 例：115 年 07-08 月期 → 開獎 115/09/25 → 領獎 115/10/06 ~ 116/01/05。

use crate::civil::Ymd;

/// 一期統一發票。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Period {
    /// 民國年（例：115）。
    pub roc_year: u16,
    /// 期別 1..=6：1=01-02月、2=03-04月、3=05-06月、4=07-08月、5=09-10月、6=11-12月。
    pub slot: u8,
}

impl Period {
    pub fn new(roc_year: u16, slot: u8) -> Option<Self> {
        (1..=6).contains(&slot).then_some(Self { roc_year, slot })
    }

    /// 西元年。
    pub fn year(&self) -> i32 {
        self.roc_year as i32 + 1911
    }

    /// 起始月份（1、3、5、7、9、11）。
    pub fn start_month(&self) -> u32 {
        (self.slot as u32 - 1) * 2 + 1
    }

    /// 結束月份（2、4、6、8、10、12）。
    pub fn end_month(&self) -> u32 {
        self.start_month() + 1
    }

    /// 財政部 5 碼期別代碼，例：115 年 07-08 月 = `11504`。
    pub fn code(&self) -> String {
        format!("{:03}{:02}", self.roc_year, self.slot)
    }

    /// 官方 PDF 檔名前綴使用的期別標記，例：115 年 07-08 月 = `20260708`。
    pub fn pdf_tag(&self) -> String {
        format!(
            "{}{:02}{:02}",
            self.year(),
            self.start_month(),
            self.end_month()
        )
    }

    /// 人類可讀標題，例：`115年07-08月`。
    pub fn label(&self) -> String {
        format!(
            "{:03}年{:02}-{:02}月",
            self.roc_year,
            self.start_month(),
            self.end_month()
        )
    }

    /// 開獎日：該期結束月的次月 25 日（必為單月）。
    ///
    /// 例：07-08 月期 → 09/25 開獎；11-12 月期 → 次年 01/25 開獎。
    pub fn draw_date(&self) -> Ymd {
        Ymd::new(self.year(), self.end_month(), 25).add_months(1)
    }

    /// 領獎期間（起, 迄），含頭尾。
    ///
    /// 起 = 開獎日之次月 6 日；迄 = 起 + 3 個月 - 1 天（即開獎月 +4 月的 5 日）。
    pub fn redeem_window(&self) -> (Ymd, Ymd) {
        let d = self.draw_date().add_months(1);
        let start = Ymd::new(d.y, d.m, 6);
        let end = start.add_months(3).add_days(-1);
        (start, end)
    }

    /// 由「發票開立日期」（西元）推算所屬期別。
    pub fn containing_date(d: Ymd) -> Self {
        let roc = (d.y - 1911) as u16;
        let slot = ((d.m - 1) / 2 + 1) as u8;
        Self {
            roc_year: roc,
            slot,
        }
    }

    /// 指定日期當天已經公布的最新一期。
    ///
    /// 開獎日當天即視為已公布；這個函式供更新檢查使用，避免抓取程序
    /// 靜默交付上一期資料。
    pub fn latest_drawn_on(d: Ymd) -> Self {
        let mut period = Self::containing_date(d);
        while period.draw_date() > d {
            period = period.prev();
        }
        period
    }

    /// 由 5 碼期別代碼還原，例：`11504` → 115 年 07-08 月。
    pub fn from_code(code: &str) -> Option<Self> {
        let c = code.trim();
        if c.len() != 5 || !c.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let roc: u16 = c[0..3].parse().ok()?;
        let slot: u8 = c[3..5].parse().ok()?;
        Self::new(roc, slot)
    }

    /// 下一期。
    pub fn next(&self) -> Self {
        if self.slot == 6 {
            Self {
                roc_year: self.roc_year + 1,
                slot: 1,
            }
        } else {
            Self {
                roc_year: self.roc_year,
                slot: self.slot + 1,
            }
        }
    }

    /// 上一期。
    pub fn prev(&self) -> Self {
        if self.slot == 1 {
            Self {
                roc_year: self.roc_year.saturating_sub(1),
                slot: 6,
            }
        } else {
            Self {
                roc_year: self.roc_year,
                slot: self.slot - 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_and_redeem_window_matches_official() {
        // 官方：115年07-08月期，領獎期間 115/10/06 起至 116/01/05 止
        let p = Period::new(115, 4).unwrap();
        assert_eq!(p.label(), "115年07-08月");
        assert_eq!(p.draw_date(), Ymd::new(2026, 9, 25));
        let (s, e) = p.redeem_window();
        assert_eq!(s, Ymd::new(2026, 10, 6));
        assert_eq!(e, Ymd::new(2027, 1, 5));

        // 官方：115年05-06月期，領獎期間 115/08/06 起至 115/11/05 止
        let p = Period::new(115, 3).unwrap();
        let (s, e) = p.redeem_window();
        assert_eq!(s, Ymd::new(2026, 8, 6));
        assert_eq!(e, Ymd::new(2026, 11, 5));

        // 115年11-12月期：開獎 116/01/25 → 領獎 116/02/06 ~ 116/05/05
        let p = Period::new(115, 6).unwrap();
        assert_eq!(p.draw_date(), Ymd::new(2027, 1, 25));
        let (s, e) = p.redeem_window();
        assert_eq!(s, Ymd::new(2027, 2, 6));
        assert_eq!(e, Ymd::new(2027, 5, 5));
    }

    #[test]
    fn codes_and_tags() {
        let p = Period::new(115, 4).unwrap();
        assert_eq!(p.code(), "11504");
        assert_eq!(p.pdf_tag(), "20260708");
        assert_eq!(Period::from_code("11504"), Some(p));
        assert_eq!(Period::from_code("11507"), None);
    }

    #[test]
    fn period_from_date() {
        assert_eq!(
            Period::containing_date(Ymd::new(2026, 7, 1)),
            Period::new(115, 4).unwrap()
        );
        assert_eq!(
            Period::containing_date(Ymd::new(2026, 8, 31)),
            Period::new(115, 4).unwrap()
        );
        assert_eq!(
            Period::containing_date(Ymd::new(2026, 9, 1)),
            Period::new(115, 5).unwrap()
        );
    }

    #[test]
    fn neighbour_periods() {
        let p = Period::new(115, 6).unwrap();
        assert_eq!(p.next(), Period::new(116, 1).unwrap());
        assert_eq!(p.prev(), Period::new(115, 5).unwrap());
        assert_eq!(
            Period::new(115, 1).unwrap().prev(),
            Period::new(114, 6).unwrap()
        );
    }

    #[test]
    fn latest_drawn_period_follows_the_official_calendar() {
        assert_eq!(
            Period::latest_drawn_on(Ymd::new(2026, 9, 24)),
            Period::new(115, 3).unwrap()
        );
        assert_eq!(
            Period::latest_drawn_on(Ymd::new(2026, 9, 25)),
            Period::new(115, 4).unwrap()
        );
        assert_eq!(
            Period::latest_drawn_on(Ymd::new(2027, 1, 24)),
            Period::new(115, 5).unwrap()
        );
        assert_eq!(
            Period::latest_drawn_on(Ymd::new(2027, 1, 25)),
            Period::new(115, 6).unwrap()
        );
    }
}
