//! 統一發票字軌號碼（Invoice Track Number）。
//!
//! 格式固定 10 碼：2 碼大寫英文字軌 + 8 碼阿拉伯數字，例如 `AB12345678`。
//!
//! 為了讓「雲端發票專屬獎」數百萬組號碼能在數 MB 內塞進單一執行檔並支援
//! O(log n) 查詢，這裡把字軌壓成 37 bit 整數：
//!
//! ```text
//!   bits 36..27 : 英文字軌索引（0..675 = 26*26，A=0, Z=25；AA=0, ZZ=675）
//!   bits 26..0  : 8 位數字（0..99_999_999）
//! ```

use std::fmt;

/// 壓縮後的統一發票字軌號碼。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvoiceCode(u64);

/// 字軌解析錯誤。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeError {
    BadLength(usize),
    NonAsciiAlnum,
    NotUppercaseLetters,
    NotDigits,
}

impl fmt::Display for CodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeError::BadLength(n) => write!(f, "字軌號碼長度必須為 10 碼，實際 {n} 碼"),
            CodeError::NonAsciiAlnum => write!(f, "字軌號碼只能含英文字母與數字"),
            CodeError::NotUppercaseLetters => write!(f, "前 2 碼必須為大寫英文字母"),
            CodeError::NotDigits => write!(f, "後 8 碼必須為數字"),
        }
    }
}

impl std::error::Error for CodeError {}

impl InvoiceCode {
    /// 由 10 碼字串解析。
    pub fn parse(s: &str) -> Result<Self, CodeError> {
        // 容忍空白與連字號，例如 "AB-12345678" 或 "ab12345678"。
        let cleaned: String = s
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .flat_map(|c| c.to_uppercase())
            .collect();
        let b = cleaned.as_bytes();
        if b.len() != 10 {
            return Err(CodeError::BadLength(b.len()));
        }
        if !b.iter().all(|c| c.is_ascii_alphanumeric()) {
            return Err(CodeError::NonAsciiAlnum);
        }
        if !(b[0].is_ascii_uppercase() && b[1].is_ascii_uppercase()) {
            return Err(CodeError::NotUppercaseLetters);
        }
        if !b[2..].iter().all(|c| c.is_ascii_digit()) {
            return Err(CodeError::NotDigits);
        }
        let li = (b[0] - b'A') as u64 * 26 + (b[1] - b'A') as u64;
        let num: u64 = cleaned[2..].parse().map_err(|_| CodeError::NotDigits)?;
        Ok(Self((li << 27) | num))
    }

    /// 由 8 位純數字（傳統紙本發票）建立，字軌索引固定為 0（`AA`）。
    ///
    /// 紙本長條發票沒有英文字軌，只能對「期別 + 8 碼」的獎別（特別獎、特獎、
    /// 頭獎、二~六獎）；雲端發票專屬獎則無法以此比對（本質上不適用）。
    pub fn from_digits(digits: &str) -> Result<Self, CodeError> {
        let d = digits.trim();
        if d.len() != 8 || !d.bytes().all(|b| b.is_ascii_digit()) {
            return Err(CodeError::NotDigits);
        }
        let num: u64 = d.parse().map_err(|_| CodeError::NotDigits)?;
        Ok(Self(num))
    }

    /// 壓縮值（37 bit 有效）。
    pub fn to_raw(self) -> u64 {
        self.0
    }

    /// 由壓縮值還原。
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// 8 位數字部分。
    pub fn digits(self) -> u32 {
        (self.0 & 0x07FF_FFFF) as u32
    }

    /// 8 位數字部分補零字串。
    pub fn digits_str(self) -> String {
        format!("{:08}", self.digits())
    }

    /// 2 碼英文字軌。
    pub fn letters(self) -> String {
        let li = self.0 >> 27;
        let a = (li / 26) as u8 + b'A';
        let b = (li % 26) as u8 + b'A';
        String::from_utf8(vec![a, b]).unwrap_or_else(|_| "??".into())
    }

    /// 末 `n` 碼數字（n <= 8），用於二~六獎比對。
    pub fn suffix(self, n: u32) -> u32 {
        debug_assert!(n <= 8);
        self.digits() % 10u32.pow(n)
    }
}

impl fmt::Display for InvoiceCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.letters(), self.digits_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        let c = InvoiceCode::parse("AB12345678").unwrap();
        assert_eq!(c.to_string(), "AB12345678");
        assert_eq!(c.letters(), "AB");
        assert_eq!(c.digits(), 12_345_678);
        assert_eq!(c.digits_str(), "12345678");
    }

    #[test]
    fn normalises_input() {
        assert_eq!(
            InvoiceCode::parse("ab-1234 5678").unwrap().to_string(),
            "AB12345678"
        );
    }

    #[test]
    fn rejects_bad() {
        assert!(InvoiceCode::parse("A12345678").is_err());
        assert!(InvoiceCode::parse("AB1234567X").is_err());
        assert!(InvoiceCode::parse("112345678").is_err());
    }

    #[test]
    fn suffix_math() {
        let c = InvoiceCode::parse("AB12345678").unwrap();
        assert_eq!(c.suffix(3), 678);
        assert_eq!(c.suffix(7), 2_345_678);
        assert_eq!(c.suffix(8), 12_345_678);
    }

    #[test]
    fn ordering_is_stable() {
        let a = InvoiceCode::parse("AA00000001").unwrap();
        let b = InvoiceCode::parse("AA00000002").unwrap();
        let z = InvoiceCode::parse("ZZ99999999").unwrap();
        assert!(a < b && b < z);
    }

    #[test]
    fn paper_digits() {
        let c = InvoiceCode::from_digits("03345678").unwrap();
        assert_eq!(c.digits(), 3_345_678);
        assert_eq!(c.suffix(3), 678);
        assert!(InvoiceCode::from_digits("1234567").is_err());
    }
}
