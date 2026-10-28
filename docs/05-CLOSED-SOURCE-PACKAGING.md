# 封裝成「單檔、閉源、開程式才連網」的應用

對應需求：**純低端完全閉源封包、打開時連網路才會自動抓取必要更新資訊。**

---

## 一、「單檔」怎麼做到

### 1. 編譯設定（已經寫在 `Cargo.toml`）

```toml
[profile.release]
opt-level = "z"      # 優先縮小體積
lto = true           # 跨 crate 最佳化，順便讓反編譯更難讀
codegen-units = 1    # 最佳化更徹底
panic = "abort"      # 移除 panic unwind 表，省 ~15%
strip = true         # 移除符號表
```

實測結果：

| 項目 | 大小 |
|---|---|
| 原始 debug 建置 | 38 MB |
| release + LTO + strip | **2.5 MB** |
| 執行期依賴 | 只有 `libc`（Linux）/ `kernel32`（Windows） |

### 2. 內嵌資源

用 `include_str!` / `include_bytes!` 把前端與保底資料打進執行檔：

```rust
const INDEX_HTML: &str = include_str!("../../../web/index.html");
const EMBEDDED_DATASET: &str = include_str!("../../../data/draws.json");
```

執行檔自帶 UI，不需要額外的 `assets/` 資料夾，複製一個檔案就能跑。

### 3. Windows 不要拖 VCRuntime

MSVC 目標預設動態連結 CRT，別台電腦會缺 DLL。兩個解法：

```bash
# 方案 A：改用 GNU 目標（靜態連結，單檔最乾淨）
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu

# 方案 B：維持 MSVC 但靜態連結 CRT
# 在 .cargo/config.toml 加入：
# [target.x86_64-pc-windows-msvc]
# rustflags = ["-C", "target-feature=+crt-static"]
```

---

## 二、「閉源」怎麼做到

### 1. repo 設 Private

最乾脆。GitHub → Settings → Danger Zone → Change visibility → Private。

### 2. 只發佈 binary，不發佈原始碼

Release 只放 `twiv.exe` / `twiv` / `twiv-macos`，不放 source archive。
（GitHub 對 Public repo 會自動附 `Source code (zip)`，這是唯一需要 Private 的原因。）

### 3. 逆向門檻

| 手段 | 效果 |
|---|---|
| `strip = true` | 移除符號，函式名稱全消失 |
| `lto = true` | 跨 crate 內聯，呼叫邊界模糊 |
| `panic = "abort"` | 移除 unwind 資訊 |
| UPX 壓殼（選用） | 進一步混淆，但防毒軟體可能誤報，不建議 |

⚠️ **誠實提醒**：Rust binary 無法做到「絕對不可逆」。有心人仍可用
Ghidra / IDA 反編譯出邏輯。真正值錢的不是程式碼，而是
**資料管線與更新機制**（誰在維護每期的 PDF 解析）。
所以閉源的實際意義是「不主動給原始碼」，不是「別人一定看不懂」。

### 4. 授權條款

`LICENSE` 已用專有授權（All Rights Reserved），只允許原樣執行。
若要商業授權，另立合約。

---

## 三、「開程式才連網」怎麼做到

核心原則：**資料內嵌當保底，網路只用在「檢查有沒有更新」。**

```
啟動
 │
 ├─ 讀使用者資料目錄的 draws.json（上次更新下載的）
 │    沒有 → 用執行檔內嵌的版本
 │
 ├─ 使用者按「檢查更新」才發請求（也可設定成啟動時自動檢查一次）
 │     GET manifest.json
 │     ├─ dataset_sha256 相同 → 什麼都不做
 │     └─ 不同 → 下載 → 驗 SHA-256 → 落盤
 │
 └─ 對獎全程離線，不發任何請求
```

### 更新清單格式

```json
{
  "schema": 1,
  "app_version": "0.1.0",
  "generated_at": "2026-10-28T13:09:43Z",
  "period_latest": "115年07-08月",
  "dataset_url": "https://raw.githubusercontent.com/.../data/draws.json",
  "dataset_bytes": 674,
  "dataset_sha256": "…64 碼…",
  "cloud": [
    {
      "period": "11504",
      "label": "115年07-08月",
      "url": "https://…/cloud_11504.bin",
      "bytes": 10506240,
      "sha256": "…64 碼…",
      "counts": { "雲端發票專屬獎 500 元": 4350000 }
    }
  ]
}
```

### 完整性驗證（不可省）

下載完**先驗雜湊再落盤**，不符就丟棄：

```rust
let got = sha256_hex(&bytes);
if !got.eq_ignore_ascii_case(want) {
    return Err("資料集雜湊不符，已中止");
}
```

沒有這一步，一個被汙染的更新源就能讓使用者看到錯的中獎號碼。

### 更新頻率

每兩個月一次（單月 25 日開獎後）。
建議：App 啟動時靜默檢查一次，有新版才提示；不要每次開都跳視窗。

---

## 四、發佈流程

```bash
# 1. 更新資料（維護者）
cargo run --release -- fetch                       # 抓官方號碼 → data/draws.json
cargo run --release -- cloud                       # 下載雲端名單 → 資料目錄
# 2. 產生 manifest（見 scripts/make_manifest.sh）
# 3. 提交並推上 GitHub
git add data/draws.json && git commit -m "data: 更新至 115年09-10月期"
git push
# 4. GitHub Actions 自動三平台編譯 → Release
```

### 各平台產物

| 平台 | 產物 | 大小 |
|---|---|---|
| Windows x64 | `twiv.exe` | 約 2.6 MB |
| Linux x64 | `twiv` | 2.5 MB |
| macOS（Intel / Apple Silicon） | `twiv` | 約 2.7 MB |

macOS 未簽章會被 Gatekeeper 擋，使用者需右鍵→打開，或
`xattr -d com.apple.quarantine ./twiv`。要正式發佈請申請 Apple Developer 簽章。

---

## 五、安裝方式（給使用者）

不用安裝。下載 → 解壓 → 雙擊。

- Windows：雙擊 `twiv.exe`，瀏覽器自動開啟 `http://127.0.0.1:8787`
- macOS / Linux：`chmod +x twiv && ./twiv`

首次執行會在資料目錄建立 `draws.json` 等檔案，體積 < 1 MB。

---

## 六、要不要做成真正的「桌面視窗」

目前是「本機服務 + 瀏覽器」，優點是零依賴、體積小、手機可從同網段連。

若一定要獨立視窗（無瀏覽器痕跡），兩條路：

| 方案 | 體積 | 代價 |
|---|---|---|
| Tauri | +8~15 MB | 需要系統 WebView2 / WebKit，Windows 10+ 才有 |
| 原生 GUI（egui / iced） | +3~5 MB | 前端要重寫成 Rust，`web/` 那套作廢 |

對「純低端、單檔」這個目標，**現行方案其實最貼近需求**：
一個 2.5 MB 的檔案，沒有安裝程式、沒有背景服務、沒有額外 runtime。
