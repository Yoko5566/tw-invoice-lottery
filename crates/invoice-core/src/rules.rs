//! 統一發票獎別與獎金規則。
//!
//! 依據《統一發票給獎辦法》現行級距：
//!
//! | 獎別 | 條件 | 獎金 |
//! |---|---|---|
//! | 特別獎 | 8 碼全同 | 10,000,000 |
//! | 特獎 | 8 碼全同 | 2,000,000 |
//! | 頭獎 | 8 碼全同（每期 3 組） | 200,000 |
//! | 二獎 | 末 7 碼與頭獎相同 | 40,000 |
//! | 三獎 | 末 6 碼與頭獎相同 | 10,000 |
//! | 四獎 | 末 5 碼與頭獎相同 | 4,000 |
//! | 五獎 | 末 4 碼與頭獎相同 | 1,000 |
//! | 六獎 | 末 3 碼與頭獎相同 | 200 |
//! | 增開六獎 | 末 3 碼與增開號碼相同 | 200 |
//! | 雲端發票百萬元獎 | 字軌 10 碼全同 | 1,000,000 |
//! | 雲端發票 2000 元獎 | 字軌 10 碼全同 | 2,000 |
//! | 雲端發票 800 元獎 | 字軌 10 碼全同 | 800 |
//! | 雲端發票 500 元獎 | 字軌 10 碼全同 | 500 |
//!
//! 同一張發票同時對中多個獎別時，**以兌領一個獎項為限**，實務上取最高金額。

use std::fmt;

/// 獎別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Prize {
    Cloud500,
    Cloud800,
    Cloud2000,
    CloudMillion,
    Sixth,
    ExtraSixth,
    Fifth,
    Fourth,
    Third,
    Second,
    First,
    Grand,
    Special,
}

impl Prize {
    /// 獎金（新台幣元）。
    pub fn amount(self) -> u64 {
        match self {
            Prize::Cloud500 => 500,
            Prize::Cloud800 => 800,
            Prize::Cloud2000 => 2_000,
            Prize::CloudMillion => 1_000_000,
            Prize::Sixth => 200,
            Prize::ExtraSixth => 200,
            Prize::Fifth => 1_000,
            Prize::Fourth => 4_000,
            Prize::Third => 10_000,
            Prize::Second => 40_000,
            Prize::First => 200_000,
            Prize::Grand => 2_000_000,
            Prize::Special => 10_000_000,
        }
    }

    /// 中文獎別名稱。
    pub fn label(self) -> &'static str {
        match self {
            Prize::Cloud500 => "雲端發票專屬獎 500 元",
            Prize::Cloud800 => "雲端發票專屬獎 800 元",
            Prize::Cloud2000 => "雲端發票專屬獎 2000 元",
            Prize::CloudMillion => "雲端發票專屬獎 100 萬元",
            Prize::Sixth => "六獎",
            Prize::ExtraSixth => "增開六獎",
            Prize::Fifth => "五獎",
            Prize::Fourth => "四獎",
            Prize::Third => "三獎",
            Prize::Second => "二獎",
            Prize::First => "頭獎",
            Prize::Grand => "特獎",
            Prize::Special => "特別獎",
        }
    }

    /// 是否屬於「雲端發票專屬獎」（紙本發票不適用）。
    pub fn is_cloud(self) -> bool {
        matches!(
            self,
            Prize::Cloud500 | Prize::Cloud800 | Prize::Cloud2000 | Prize::CloudMillion
        )
    }

    /// 是否需扣繳 20% 所得稅（三獎含以上）。
    pub fn subject_to_income_tax(self) -> bool {
        !self.is_cloud() && self.amount() >= 10_000
    }

    /// 是否需繳 0.4% 印花稅（五獎及雲端 500 元以上）。
    pub fn subject_to_stamp_tax(self) -> bool {
        self.amount() >= 1_000 || self.is_cloud()
    }

    /// 扣繳後之實領金額（僅計所得稅；印花稅另計，且設定帳戶可免）。
    pub fn net_amount(self) -> u64 {
        if self.subject_to_income_tax() {
            self.amount() - self.amount() / 5
        } else {
            self.amount()
        }
    }
}

impl fmt::Display for Prize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}（NT${}）", self.label(), self.amount())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tax_rules() {
        assert_eq!(Prize::Third.net_amount(), 8_000);
        assert_eq!(Prize::Fourth.net_amount(), 4_000);
        assert!(Prize::Third.subject_to_income_tax());
        assert!(!Prize::Fourth.subject_to_income_tax());
        assert!(Prize::Fifth.subject_to_stamp_tax());
        assert!(Prize::Cloud500.subject_to_stamp_tax());
    }

    #[test]
    fn cloud_flag() {
        assert!(Prize::CloudMillion.is_cloud());
        assert!(!Prize::Special.is_cloud());
    }
}
