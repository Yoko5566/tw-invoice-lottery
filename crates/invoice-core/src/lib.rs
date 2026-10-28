//! # invoice-core
//!
//! 台灣統一發票對獎的核心運算，零外部依賴：
//!
//! - [`period`]：期別 ↔ 開獎日 ↔ 領獎期間的換算
//! - [`code`]：10 碼字軌號碼 ↔ 37 bit 整數的壓縮
//! - [`rules`]：獎別級距與稅務規則
//! - [`check`]：對獎引擎（特別獎／特獎／頭獎／二~六獎／增開六獎）
//! - [`cloud_index`]：雲端發票專屬獎的百萬級壓縮索引
//! - [`dataset`]：多期資料集與 JSON 序列化
//! - [`json`]：極簡 JSON 實作
//!
//! 這個 crate 不碰網路、不碰檔案系統，可直接在 `no_std`-ish 環境重用。

pub mod check;
pub mod civil;
pub mod cloud_index;
pub mod code;
pub mod dataset;
pub mod json;
pub mod period;
pub mod rules;

pub use check::{check_draw, Draw, Hit};
pub use civil::Ymd;
pub use cloud_index::{CloudIndex, CloudTier};
pub use code::{CodeError, InvoiceCode};
pub use dataset::{CloudOutcome, Dataset};
pub use period::Period;
pub use rules::Prize;

/// 版本字串（給 `--version` 與 API 使用）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
