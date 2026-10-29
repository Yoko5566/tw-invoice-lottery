//! 純公曆日期算術（無外部依賴）。
//!
//! 使用 Howard Hinnant 的 `days_from_civil` / `civil_from_days` 演算法，
//! 只處理 1970±數百年範圍，足以涵蓋統一發票所有期別。

/// 年-月-日。月份 1..=12，日 1..=31。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ymd {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

impl Ymd {
    pub const fn new(y: i32, m: u32, d: u32) -> Self {
        Self { y, m, d }
    }

    /// 距 1970-01-01 的天數。
    pub fn to_days(self) -> i64 {
        let (mut y, m, d) = (self.y as i64, self.m as i64, self.d as i64);
        y -= (m <= 2) as i64;
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// 從 1970-01-01 起算的天數還原日期。
    pub fn from_days(z: i64) -> Self {
        let z = z + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        Self {
            y: (y + (m <= 2) as i64) as i32,
            m: m as u32,
            d: d as u32,
        }
    }

    /// 加減天數。
    pub fn add_days(self, n: i64) -> Self {
        Self::from_days(self.to_days() + n)
    }

    /// 月份加減並自動跨年。`n` 可為負。
    pub fn add_months(self, n: i32) -> Self {
        let total = self.y * 12 + (self.m as i32 - 1) + n;
        let y = total.div_euclid(12);
        let m = total.rem_euclid(12) as u32 + 1;
        Self { y, m, d: self.d }
    }
}

#[cfg(test)]
mod tests {
    use super::Ymd;

    #[test]
    fn epoch_roundtrip() {
        assert_eq!(Ymd::new(1970, 1, 1).to_days(), 0);
        assert_eq!(Ymd::from_days(0), Ymd::new(1970, 1, 1));
    }

    #[test]
    fn known_dates() {
        // 2026-07-25（固定日期測試向量；115年07-08期實際開獎日為 2026-09-25）
        assert_eq!(Ymd::new(2026, 7, 25).to_days(), 20_659);
        assert_eq!(Ymd::from_days(20_659), Ymd::new(2026, 7, 25));
    }

    #[test]
    fn month_rollover() {
        assert_eq!(Ymd::new(2026, 11, 5).add_months(3), Ymd::new(2027, 2, 5));
        assert_eq!(Ymd::new(2026, 12, 6).add_months(3), Ymd::new(2027, 3, 6));
    }
}
