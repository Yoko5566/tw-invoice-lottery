//! 資料存放與更新。
//!
//! 設計原則（對應「離線優先、開程式才連網」的需求）：
//!
//! 1. 執行檔內嵌一份中獎號碼資料（建置時把 `data/draws.json` 包進去），
//!    所以**完全離線也能對獎**。
//! 2. 啟動時（或使用者按「檢查更新」）才連網抓更新清單，比對 SHA-256，
//!    有新版才下載，寫進使用者資料目錄。
//! 3. 所有下載內容都先驗雜湊再落盤，不符就丟棄，避免中間人替換。
//!
//! 資料目錄：
//! - Windows：`%APPDATA%\twiv`
//! - macOS/Linux：`$XDG_DATA_HOME/twiv` 或 `~/.local/share/twiv`

use invoice_core::cloud_index::CloudIndex;
use invoice_core::dataset::Dataset;
use invoice_core::json::{self, Json};
use invoice_scrape::{net, sha256_hex};
use std::path::{Path, PathBuf};

/// 建置時內嵌的保底資料。
pub const EMBEDDED_DATASET: &str = include_str!("../../../data/draws.json");

/// 預設更新清單位址（可用環境變數 `TWIV_MANIFEST` 或資料夾內 `config.json` 覆寫）。
pub const DEFAULT_MANIFEST: &str =
    "https://raw.githubusercontent.com/Yoko5566/tw-invoice-lottery-data/main/data/manifest.json";

/// 執行期資料。
#[derive(Debug)]
pub struct Store {
    pub dir: PathBuf,
    pub dataset: Dataset,
    pub cloud: Option<CloudIndex>,
    pub manifest_url: String,
    /// 資料來源說明：`embedded` 或檔案路徑。
    pub dataset_origin: String,
}

impl Store {
    /// 載入資料（優先使用者目錄，其次內嵌）。
    pub fn load() -> Self {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);

        let (dataset, origin) = match std::fs::read_to_string(dir.join("draws.json")) {
            Ok(text) => match json::parse(&text)
                .map_err(|e| e.to_string())
                .and_then(|v| Dataset::from_json(&v))
            {
                Ok(ds) if !ds.periods.is_empty() => {
                    (ds, dir.join("draws.json").display().to_string())
                }
                _ => (Self::embedded(), "embedded".to_string()),
            },
            Err(_) => (Self::embedded(), "embedded".to_string()),
        };

        let cloud = Self::load_cloud_from_dir(
            &dir,
            dataset
                .latest()
                .map(|d| d.period.code())
                .unwrap_or_default(),
        );

        let manifest_url = std::env::var("TWIV_MANIFEST")
            .ok()
            .or_else(|| {
                read_config(&dir).and_then(|c| c.str_at("manifest_url").map(str::to_string))
            })
            .unwrap_or_else(|| DEFAULT_MANIFEST.to_string());

        Self {
            dir,
            dataset,
            cloud,
            manifest_url,
            dataset_origin: origin,
        }
    }

    fn embedded() -> Dataset {
        json::parse(EMBEDDED_DATASET)
            .ok()
            .and_then(|v| Dataset::from_json(&v).ok())
            .filter(|d| !d.periods.is_empty())
            .unwrap_or_default()
    }

    fn load_cloud_from_dir(dir: &Path, period_code: String) -> Option<CloudIndex> {
        if period_code.is_empty() {
            return None;
        }
        let path = dir.join(format!("cloud_{period_code}.bin"));
        let bytes = std::fs::read(path).ok()?;
        let idx = CloudIndex::decode(&bytes).ok()?;
        Some(idx)
    }

    /// 載入某期的雲端索引（若檔案存在）。
    pub fn ensure_cloud(&mut self, period_code: &str) -> bool {
        if self.cloud.is_some() {
            return true;
        }
        self.cloud = Self::load_cloud_from_dir(&self.dir, period_code.to_string());
        self.cloud.is_some()
    }

    /// 檢查更新清單（只做 GET，不下載大檔）。
    pub fn fetch_manifest(&self) -> Result<Json, String> {
        let text = net::get_text(&self.manifest_url).map_err(|e| e.to_string())?;
        json::parse(&text).map_err(|e| e.to_string())
    }

    /// 依更新清單下載資料集（含雜湊驗證）。
    pub fn update_dataset(&mut self) -> Result<Report, String> {
        let m = self.fetch_manifest()?;
        let url = m.str_at("dataset_url").ok_or("更新清單缺少 dataset_url")?;
        let want = m
            .str_at("dataset_sha256")
            .ok_or("更新清單缺少 dataset_sha256")?;

        let bytes = net::get_bytes(url, 64 * 1024 * 1024).map_err(|e| e.to_string())?;
        let got = sha256_hex(&bytes);
        if !got.eq_ignore_ascii_case(want) {
            return Err(format!("資料集雜湊不符（預期 {want}，實際 {got}），已中止"));
        }
        let text = String::from_utf8(bytes.clone()).map_err(|e| e.to_string())?;
        let ds = Dataset::from_json(&json::parse(&text).map_err(|e| e.to_string())?)
            .map_err(|e| format!("資料集格式錯誤：{e}"))?;
        if ds.periods.is_empty() {
            return Err("下載到的資料集是空的".into());
        }

        std::fs::write(self.dir.join("draws.json"), &bytes).map_err(|e| e.to_string())?;
        let latest = ds.latest().unwrap().period.label();
        self.dataset = ds;
        self.dataset_origin = self.dir.join("draws.json").display().to_string();
        Ok(Report {
            latest,
            bytes: bytes.len(),
            sha256: got,
        })
    }

    /// 下載當期雲端發票專屬獎索引。
    pub fn update_cloud(&mut self, period_code: &str) -> Result<Report, String> {
        let m = self.fetch_manifest()?;
        let Some(arr) = m.get("cloud").and_then(|c| c.as_arr()) else {
            return Err("更新清單沒有雲端發票專屬獎資料".into());
        };
        let entry = arr
            .iter()
            .find(|e| e.str_at("period") == Some(period_code))
            .ok_or_else(|| format!("更新清單沒有 {period_code} 期的雲端索引"))?;
        let url = entry.str_at("url").ok_or("缺少 url")?;
        let want = entry.str_at("sha256").ok_or("缺少 sha256")?;

        let bytes = net::get_bytes(url, 512 * 1024 * 1024).map_err(|e| e.to_string())?;
        let got = sha256_hex(&bytes);
        if !got.eq_ignore_ascii_case(want) {
            return Err(format!("索引雜湊不符（預期 {want}，實際 {got}），已中止"));
        }
        let idx = CloudIndex::decode(&bytes).map_err(|e| format!("索引解碼失敗：{e}"))?;
        let total = idx.total();
        std::fs::write(self.dir.join(format!("cloud_{period_code}.bin")), &bytes)
            .map_err(|e| e.to_string())?;
        self.cloud = Some(idx);
        Ok(Report {
            latest: format!("{period_code} / {total} 筆"),
            bytes: bytes.len(),
            sha256: got,
        })
    }

    /// 資料概況。
    pub fn status(&self) -> Json {
        let periods: Vec<Json> = self
            .dataset
            .periods
            .iter()
            .map(|d| {
                let (s, e) = d.period.redeem_window();
                json::obj(vec![
                    ("period", json::s(d.period.code())),
                    ("label", json::s(d.period.label())),
                    ("special", json::s(&d.special)),
                    ("grand", json::s(&d.grand)),
                    ("first", Json::Arr(d.first.iter().map(json::s).collect())),
                    (
                        "redeem_start",
                        json::s(format!("{:04}-{:02}-{:02}", s.y, s.m, s.d)),
                    ),
                    (
                        "redeem_end",
                        json::s(format!("{:04}-{:02}-{:02}", e.y, e.m, e.d)),
                    ),
                ])
            })
            .collect();

        json::obj(vec![
            ("app_version", json::s(invoice_core::VERSION)),
            ("dataset_origin", json::s(&self.dataset_origin)),
            ("generated_at", json::s(&self.dataset.generated_at)),
            ("periods", Json::Arr(periods)),
            ("cloud_loaded", Json::Bool(self.cloud.is_some())),
            (
                "cloud_total",
                json::n(f64::from(
                    self.cloud.as_ref().map(|c| c.total()).unwrap_or(0),
                )),
            ),
            ("data_dir", json::s(self.dir.display().to_string())),
            ("manifest_url", json::s(&self.manifest_url)),
        ])
    }
}

/// 更新結果。
#[derive(Debug, Clone)]
pub struct Report {
    pub latest: String,
    pub bytes: usize,
    pub sha256: String,
}

fn read_config(dir: &Path) -> Option<Json> {
    let text = std::fs::read_to_string(dir.join("config.json")).ok()?;
    json::parse(&text).ok()
}

/// 跨平台資料目錄。
pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("TWIV_DATA_DIR") {
        return PathBuf::from(p);
    }
    #[cfg(windows)]
    {
        if let Ok(p) = std::env::var("APPDATA") {
            return PathBuf::from(p).join("twiv");
        }
    }
    if let Ok(p) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(p).join("twiv");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("twiv")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_dataset_is_usable() {
        let ds = json::parse(EMBEDDED_DATASET)
            .ok()
            .and_then(|v| Dataset::from_json(&v).ok())
            .expect("內嵌資料必須可解析");
        assert!(!ds.periods.is_empty(), "內嵌資料不得為空");
        for d in &ds.periods {
            assert!(d.validate().is_ok());
        }
        // 期別順序：新到舊
        let codes: Vec<String> = ds.periods.iter().map(|d| d.period.code()).collect();
        let mut sorted = codes.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(codes, sorted);
    }

    #[test]
    fn data_dir_is_non_empty() {
        // 注意：其他測試會設定 TWIV_DATA_DIR，這裡只驗證有回傳路徑
        let d = data_dir();
        assert!(!d.as_os_str().is_empty());
    }
}
