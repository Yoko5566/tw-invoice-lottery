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
    pub cloud_period: Option<String>,
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
            cloud_period: cloud
                .as_ref()
                .and_then(|_| dataset.latest().map(|d| d.period.code())),
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
        if self.cloud.is_some() && self.cloud_period.as_deref() == Some(period_code) {
            return true;
        }
        self.cloud = Self::load_cloud_from_dir(&self.dir, period_code.to_string());
        self.cloud_period = self.cloud.as_ref().map(|_| period_code.to_string());
        self.cloud.is_some()
    }

    /// 檢查更新清單（只做 GET，不下載大檔）。
    pub fn fetch_manifest(&self) -> Result<Json, String> {
        if !self.manifest_url.starts_with("https://") {
            return Err("更新清單必須使用 HTTPS".into());
        }
        let text = net::get_text(&self.manifest_url).map_err(|e| e.to_string())?;
        json::parse(&text).map_err(|e| e.to_string())
    }

    /// 依更新清單下載資料集（含雜湊驗證）。
    pub fn update_dataset(&mut self) -> Result<Report, String> {
        let m = self.fetch_manifest()?;
        let url = m.str_at("dataset_url").ok_or("更新清單缺少 dataset_url")?;
        validate_download_url(&self.manifest_url, url)?;
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

        atomic_write(&self.dir.join("draws.json"), &bytes)?;
        let latest = ds.latest().unwrap().period.label();
        self.dataset = ds;
        self.cloud = None;
        self.cloud_period = None;
        self.dataset_origin = self.dir.join("draws.json").display().to_string();
        Ok(Report {
            latest,
            bytes: bytes.len(),
            sha256: got,
        })
    }

    /// 下載當期雲端發票專屬獎索引。
    pub fn update_cloud(&mut self, period_code: &str) -> Result<Report, String> {
        let latest_period = self
            .dataset
            .latest()
            .map(|d| d.period)
            .ok_or("資料集中沒有期別")?;
        let keep = retained_cloud_periods(latest_period);
        if !keep.iter().any(|p| p == period_code) {
            return Err(format!(
                "只保留當期與上一期雲端索引（可用：{}、{}）",
                keep[0], keep[1]
            ));
        }
        let m = self.fetch_manifest()?;
        let Some(arr) = m.get("cloud").and_then(|c| c.as_arr()) else {
            return Err("更新清單沒有雲端發票專屬獎資料".into());
        };
        let entry = arr
            .iter()
            .find(|e| e.str_at("period") == Some(period_code))
            .ok_or_else(|| format!("更新清單沒有 {period_code} 期的雲端索引"))?;
        let url = entry.str_at("url").ok_or("缺少 url")?;
        validate_download_url(&self.manifest_url, url)?;
        let want = entry.str_at("sha256").ok_or("缺少 sha256")?;

        let bytes = net::get_bytes(url, 512 * 1024 * 1024).map_err(|e| e.to_string())?;
        let got = sha256_hex(&bytes);
        if !got.eq_ignore_ascii_case(want) {
            return Err(format!("索引雜湊不符（預期 {want}，實際 {got}），已中止"));
        }
        let idx = CloudIndex::decode(&bytes).map_err(|e| format!("索引解碼失敗：{e}"))?;
        let total = idx.total();
        atomic_write(&self.dir.join(format!("cloud_{period_code}.bin")), &bytes)?;
        prune_cloud_cache(&self.dir, &keep)?;
        self.cloud = Some(idx);
        self.cloud_period = Some(period_code.to_string());
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

/// 遠端清單只能指向與清單相同 HTTPS 目錄下的檔案。
///
/// 因為更新清單內容本身是遠端資料，不能讓它任意指定下一個下載主機。
fn validate_download_url(manifest_url: &str, download_url: &str) -> Result<(), String> {
    if !manifest_url.starts_with("https://") || !download_url.starts_with("https://") {
        return Err("更新下載位址必須使用 HTTPS".into());
    }
    let base = manifest_url
        .rsplit_once('/')
        .map(|(base, _)| format!("{base}/"))
        .ok_or("更新清單網址格式錯誤")?;
    let lower = download_url.to_ascii_lowercase();
    if !download_url.starts_with(&base)
        || download_url.contains("..")
        || lower.contains("%2e")
        || download_url.contains('?')
        || download_url.contains('#')
    {
        return Err(format!("拒絕清單目錄以外的下載位址：{download_url}"));
    }
    Ok(())
}

/// 先完整寫入同目錄暫存檔並同步，再以重新命名取代正式檔。
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;

    let parent = path.parent().ok_or("輸出路徑沒有父目錄")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(&backup);

    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);

    if std::fs::rename(&tmp, path).is_ok() {
        return Ok(());
    }

    // Windows 不允許 rename 覆蓋現有檔案；保留可回復的舊檔後再換入。
    std::fs::rename(path, &backup).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::rename(&backup, path);
        return Err(format!("套用更新失敗，已回復舊檔：{e}"));
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}

fn retained_cloud_periods(latest: invoice_core::period::Period) -> [String; 2] {
    [latest.code(), latest.prev().code()]
}

fn prune_cloud_cache(dir: &Path, keep: &[String; 2]) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(code) = name
            .strip_prefix("cloud_")
            .and_then(|n| n.strip_suffix(".bin"))
        else {
            continue;
        };
        if !keep.iter().any(|p| p == code) {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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

    #[test]
    fn download_urls_must_stay_beside_the_configured_manifest() {
        let manifest = "https://raw.githubusercontent.com/Yoko5566/tw-invoice-lottery-data/main/data/manifest.json";
        assert!(validate_download_url(
            manifest,
            "https://raw.githubusercontent.com/Yoko5566/tw-invoice-lottery-data/main/data/draws.json"
        )
        .is_ok());
        assert!(validate_download_url(manifest, "http://example.com/draws.json").is_err());
        assert!(validate_download_url(
            manifest,
            "https://raw.githubusercontent.com/attacker/repo/main/data/draws.json"
        )
        .is_err());
        assert!(validate_download_url(
            manifest,
            "https://raw.githubusercontent.com/Yoko5566/tw-invoice-lottery-data/main/data/../secret"
        )
        .is_err());
    }

    #[test]
    fn atomic_write_replaces_complete_file() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("twiv-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("draws.json");
        std::fs::write(&path, b"old").unwrap();
        atomic_write(&path, b"new-complete-content").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new-complete-content");
        assert!(!dir.join("draws.json.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cloud_cache_keeps_only_latest_two_periods() {
        let latest = invoice_core::period::Period::new(115, 4).unwrap();
        let keep = retained_cloud_periods(latest);
        assert_eq!(keep, ["11504".to_string(), "11503".to_string()]);
    }
}
