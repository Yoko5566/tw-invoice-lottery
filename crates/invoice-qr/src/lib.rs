//! 電子發票證明聯 QR Code 解析。
//!
//! 台灣的「電子發票證明聯」左下與右下各有一組 QR Code：
//!
//! - **左碼（77 字元）**：對獎所需的完整資訊，包含字軌號碼、開立日期、
//!   隨機碼、金額與買賣方統一編號。
//! - **右碼**：期別與字軌等資訊（各家 POS 印製實務略有差異，本模組採寬鬆解析）。
//!
//! 左碼欄位配置（依財政部電子發票證明聯格式，長度固定 77）：
//!
//! ```text
//!   [ 0..10]  發票字軌號碼      2 碼英文 + 8 碼數字
//!   [10..17]  開立日期          民國年 3 碼 + 月 2 碼 + 日 2 碼
//!   [17..21]  隨機碼            4 碼數字
//!   [21..29]  銷售額            8 碼十六進位
//!   [29..37]  總計額            8 碼十六進位
//!   [37..45]  買受人統一編號    8 碼（全 0 表示未打統編）
//!   [45..53]  賣方統一編號      8 碼
//!   [53..77]  防偽隨機碼        24 碼
//! ```
//!
//! > ⚠️ 實務上務必用一張真實發票驗證欄位边界。`twiv qr --raw` 會原樣印出
//! > 解出的字串，方便與紙本比對。若長度不是 77，代表該張發票採用了
//! > 較舊或特規格式，請以 OCR 或人工核對補救。

use invoice_core::civil::Ymd;
use invoice_core::code::InvoiceCode;
use invoice_core::period::Period;

/// 左碼長度。
pub const LEFT_LEN: usize = 77;

/// 解析後的電子發票證明聯。
#[derive(Debug, Clone, PartialEq)]
pub struct EInvoiceQr {
    /// 原始字串。
    pub raw: String,
    pub code: InvoiceCode,
    /// 開立日期（西元）。
    pub issue_date: Ymd,
    /// 隨機碼（4 碼）。
    pub random_code: String,
    /// 銷售額。
    pub sales_amount: u32,
    /// 總計額。
    pub total_amount: u32,
    /// 買受人統編（8 碼，全 0 表示未打統編）。
    pub buyer_tax_id: String,
    /// 賣方統編。
    pub seller_tax_id: String,
    /// 防偽隨機碼。
    pub anti_fake: String,
}

impl EInvoiceQr {
    /// 所屬期別（由開立日期推算）。
    pub fn period(&self) -> Period {
        Period::containing_date(self.issue_date)
    }

    /// 是否打了買受人統編。
    ///
    /// 依《統一發票給獎辦法》，買受人為營業人（打統編）之發票不得領獎，
    /// 這是最常見的「有中獎但不能領」原因，App 必須主動提示。
    pub fn has_buyer_tax_id(&self) -> bool {
        self.buyer_tax_id != "00000000" && !self.buyer_tax_id.trim().is_empty()
    }
}

/// 解析錯誤。
#[derive(Debug, Clone, PartialEq)]
pub enum QrError {
    NotQrPayload(String),
    WrongLength { got: usize, want: usize },
    BadField { field: &'static str, value: String },
    NoQrFound,
}

impl std::fmt::Display for QrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QrError::NotQrPayload(s) => write!(f, "不是電子發票證明聯的左碼：{s}"),
            QrError::WrongLength { got, want } => {
                write!(f, "左碼長度應為 {want}，實際 {got}（請確認是否為舊格式）")
            }
            QrError::BadField { field, value } => write!(f, "欄位 {field} 格式錯誤：{value}"),
            QrError::NoQrFound => write!(f, "圖片中找不到可辨識的 QR Code"),
        }
    }
}

impl std::error::Error for QrError {}

fn hex8(s: &str, field: &'static str) -> Result<u32, QrError> {
    u32::from_str_radix(s.trim(), 16).map_err(|_| QrError::BadField {
        field,
        value: s.to_string(),
    })
}

/// 解析左碼（77 字元）。
pub fn parse_left(raw: &str) -> Result<EInvoiceQr, QrError> {
    let s = raw.trim();
    let b = s.as_bytes();
    if b.len() != LEFT_LEN {
        return Err(QrError::WrongLength {
            got: b.len(),
            want: LEFT_LEN,
        });
    }
    let code = InvoiceCode::parse(&s[0..10]).map_err(|_| QrError::BadField {
        field: "字軌號碼",
        value: s[0..10].to_string(),
    })?;

    let dt = &s[10..17];
    let roc: i32 = dt[0..3].parse().map_err(|_| QrError::BadField {
        field: "開立日期(年)",
        value: dt.to_string(),
    })?;
    let mon: u32 = dt[3..5].parse().map_err(|_| QrError::BadField {
        field: "開立日期(月)",
        value: dt.to_string(),
    })?;
    let day: u32 = dt[5..7].parse().map_err(|_| QrError::BadField {
        field: "開立日期(日)",
        value: dt.to_string(),
    })?;
    if !(1..=12).contains(&mon) || !(1..=31).contains(&day) {
        return Err(QrError::BadField {
            field: "開立日期",
            value: dt.to_string(),
        });
    }

    Ok(EInvoiceQr {
        raw: s.to_string(),
        code,
        issue_date: Ymd::new(roc + 1911, mon, day),
        random_code: s[17..21].to_string(),
        sales_amount: hex8(&s[21..29], "銷售額")?,
        total_amount: hex8(&s[29..37], "總計額")?,
        buyer_tax_id: s[37..45].to_string(),
        seller_tax_id: s[45..53].to_string(),
        anti_fake: s[53..77].to_string(),
    })
}

/// 從一組 QR payload 中自動挑出左碼並解析。
///
/// 掃描一張證明聯通常會得到兩組字串（左碼、右碼），這裡以「長度 77 且前
/// 10 碼為合法字軌」為判準挑出左碼。
pub fn from_payloads(payloads: &[String]) -> Result<EInvoiceQr, QrError> {
    if payloads.is_empty() {
        return Err(QrError::NoQrFound);
    }
    for p in payloads {
        if let Ok(v) = parse_left(p) {
            return Ok(v);
        }
    }
    Err(QrError::NotQrPayload(
        payloads[0].chars().take(40).collect(),
    ))
}

/// 從圖片位元組（JPEG/PNG/WebP）解出所有 QR payload。
///
/// 手機翻拍常見問題：反光、歪斜、對焦不足。`rqrr` 內建多尺度掃描，
/// 失敗時建議引導使用者「拉平、避開反光、填滿畫面」。
pub fn decode_image_bytes(bytes: &[u8]) -> Result<Vec<String>, QrError> {
    use image::GenericImageView;
    let img = image::load_from_memory(bytes)
        .map_err(|_| QrError::NotQrPayload("無法解碼圖片（支援 JPEG/PNG/WebP）".into()))?;
    let luma = img.to_luma8();
    let (w, h) = img.dimensions();
    let mut prep = rqrr::PreparedImage::prepare(luma);
    let grids = prep.detect_grids();
    if grids.is_empty() {
        return Err(QrError::NoQrFound);
    }
    let mut out = Vec::new();
    for g in grids {
        if let Ok((_meta, content)) = g.decode() {
            out.push(content);
        }
    }
    if out.is_empty() {
        // 縮放過小或過大的圖常見失敗，回報明確原因
        return Err(QrError::NotQrPayload(format!(
            "偵測到 QR 但無法解碼（圖片 {w}x{h}）；請確認解析度足夠且無反光"
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一張合法的左碼：AA12345678 / 115年7月1日 / 隨機碼 1234
    fn sample_left() -> String {
        format!(
            "{}{}{}{}{}{}{}",
            "AA12345678", // 字軌
            "1150701",    // 民國年月日
            "1234",       // 隨機碼
            "000003E8",   // 銷售額 1000
            "000003E8",   // 總計額 1000
            "00000000",   // 買受人統編
            "12345678",   // 賣方統編
        ) + &"X".repeat(24) // 防偽隨機碼
    }

    #[test]
    fn sample_is_77_chars() {
        assert_eq!(sample_left().len(), LEFT_LEN);
    }

    #[test]
    fn parses_all_fields() {
        let q = parse_left(&sample_left()).unwrap();
        assert_eq!(q.code.to_string(), "AA12345678");
        assert_eq!(q.issue_date, Ymd::new(2026, 7, 1));
        assert_eq!(q.random_code, "1234");
        assert_eq!(q.sales_amount, 1000);
        assert_eq!(q.total_amount, 1000);
        assert_eq!(q.buyer_tax_id, "00000000");
        assert!(!q.has_buyer_tax_id());
        assert_eq!(q.period(), Period::new(115, 4).unwrap());
    }

    #[test]
    fn detects_buyer_tax_id() {
        let mut s = sample_left();
        s.replace_range(37..45, "12345678");
        let q = parse_left(&s).unwrap();
        assert!(q.has_buyer_tax_id());
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(matches!(
            parse_left("SHORT"),
            Err(QrError::WrongLength { got: 5, want: 77 })
        ));
    }

    #[test]
    fn picks_left_code_from_multiple() {
        let payloads = vec!["11504AA12345678".to_string(), sample_left()];
        let q = from_payloads(&payloads).unwrap();
        assert_eq!(q.code.to_string(), "AA12345678");
    }

    #[test]
    fn no_qr_payload() {
        assert!(matches!(from_payloads(&[]), Err(QrError::NoQrFound)));
        assert!(from_payloads(&["hello".into()]).is_err());
    }

    #[test]
    fn decodes_qr_from_generated_png() {
        // 用 qrcode 產生的 PNG 驗證影像解碼管線（若無測試素材則略過）
        let path = std::env::temp_dir().join("twiv_qr_test.png");
        if !path.exists() {
            eprintln!("略過：找不到測試圖片 {}", path.display());
            return;
        }
        let bytes = std::fs::read(&path).unwrap();
        let payloads = decode_image_bytes(&bytes).unwrap();
        assert!(payloads.iter().any(|p| p.len() == LEFT_LEN));
    }
}
