//! 測試輔助：建立指向暫存目錄的 Store（使用內嵌資料）。

use crate::store::Store;

/// 建立一個使用內嵌資料、指向暫存目錄的 Store。
pub fn temp_store(name: &str) -> Store {
    let dir = std::env::temp_dir().join(format!("twiv_{name}_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::env::set_var("TWIV_DATA_DIR", &dir);
    let mut s = Store::load();
    s.dir = dir;
    s.cloud = None;
    s
}
