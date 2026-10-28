# Windows 完成與整合待辦

## Task 1: 安裝 Windows Rust 建置環境

**Description:** 安裝 MSVC C++ Build Tools、Windows SDK 與 rustup stable，讓目前電腦能編譯 `x86_64-pc-windows-msvc`。

**Acceptance criteria:**
- [x] Visual Studio Installer 已選取 Desktop development with C++。
- [x] 新開 PowerShell 後 `rustc --version`、`cargo --version`、`rustup show` 均成功。

**Verification:**
- [x] `rustc --version`
- [x] `cargo --version`
- [x] `rustup show active-toolchain`

**Dependencies:** None  
**Files likely touched:** None  
**Estimated scope:** S

## Task 2: 完成基準測試與 Release 編譯

**Description:** 在不修改功能的前提下，先確認原始碼能格式檢查、靜態檢查、測試及編譯。

**Acceptance criteria:**
- [x] 格式與 clippy 無錯誤。
- [x] 非網路測試通過；實際測試數以 Cargo 輸出為準。
- [x] 產生 `target/release/twiv.exe`。

**Verification:**
- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] `cargo build --release -p invoice-bin`

**Dependencies:** Task 1  
**Files likely touched:** `Cargo.lock`（僅依賴解析有變化時）  
**Estimated scope:** S

## Task 3: 驗證 CLI 與本機網頁 App

**Description:** 使用內嵌 `data/draws.json` 驗證版本、資料狀態、一般獎對獎及本機 UI。

**Acceptance criteria:**
- [x] `twiv.exe status --json` 顯示 11504、11503 兩期。
- [x] 已知中獎與未中獎號碼均回傳合理結果，未載入雲端索引時明確顯示 `not_loaded`。
- [x] 直接執行本機服務後可開啟 UI。

**Verification:**
- [x] `./target/release/twiv.exe version`
- [x] `./target/release/twiv.exe status --json`
- [x] `./target/release/twiv.exe check 89996565 --period 11504 --json`
- [x] 瀏覽器手動檢查 `http://127.0.0.1:8787`

**Dependencies:** Task 2  
**Files likely touched:** None  
**Estimated scope:** S

## Checkpoint: Windows 基準版

- [x] Task 1–3 全部完成。
- [x] 格式、Clippy、75 項非網路測試與 Release 建置全部通過；3 項連網測試仍為 ignored。

## Task 4: 決定資料下載位置並移除 CHANGE_ME

**Description:** 決定 GitHub owner/repository 與公開資料通道，更新所有實際使用的 URL。Private source repository 不直接當匿名更新來源。

**Acceptance criteria:**
- [x] `rg -n "CHANGE_ME|your-domain|your-repo" .` 只剩刻意保留的文件範例，或完全沒有結果。
- [ ] `DEFAULT_MANIFEST_URL` 指向可匿名 HTTPS 存取的位置。
- [x] `data/manifest.json` 與產生腳本使用相同基底 URL。

**Verification:**
- [ ] 在無 GitHub 登入狀態下，以瀏覽器開啟 manifest URL 成功。
- [ ] URL 回傳的不是登入頁或 404。

**Dependencies:** Task 3；GitHub owner 與 repository 名稱已確認，等待建立 Private 原始碼 repository 與 Public 資料 repository  
**Files likely touched:** `crates/invoice-bin/src/store.rs`, `data/manifest.json`, `scripts/make_manifest.sh`, `README.md`  
**Estimated scope:** M

## Task 5: 更新一般獎資料與 manifest

**Description:** 從財政部抓取當期與上一期資料，重新產生資料雜湊及更新清單。

**Acceptance criteria:**
- [ ] `data/draws.json` 包含預期期別且能被程式載入。
- [ ] manifest 的 bytes 與 SHA-256 對應實際檔案。
- [ ] App 可下載並接受更新資料。

**Verification:**
- [ ] `cargo run --release -p invoice-bin -- fetch --out data/draws.json`
- [ ] 執行 manifest 產生流程後重新計算 SHA-256 比對。
- [ ] `twiv.exe status --json`

**Dependencies:** Task 4  
**Files likely touched:** `data/draws.json`, `data/manifest.json`  
**Estimated scope:** S

## Task 6: 建立並發佈雲端專屬獎索引

**Description:** 補上目前交付物缺少的 `cloud_<period>.bin` 產生與上傳流程，並把 URL、大小、SHA-256 寫入 manifest。現有 `twiv cloud` 只會下載 manifest 已列出的索引，不會自行從官方 PDF 產生索引。

**Acceptance criteria:**
- [ ] 可重複產生 `cloud_<period>.bin`。
- [ ] manifest 的 `cloud` 陣列至少包含目前期別。
- [ ] `twiv.exe cloud --period <period>` 下載並通過 SHA-256，狀態改為 loaded。

**Verification:**
- [ ] 產物解碼成功且筆數符合來源 PDF。
- [ ] `twiv.exe cloud --period 11504`
- [ ] `twiv.exe status --json`

**Dependencies:** Task 4、Task 5  
**Files likely touched:** `crates/invoice-scrape/examples/` 或新維護者工具、`data/manifest.json`、文件  
**Estimated scope:** M

## Task 7: 真實發票 QR 驗收

**Description:** 使用真實電子發票證明聯照片驗證圖片解碼、欄位解析、期別判定與對獎。

**Acceptance criteria:**
- [ ] 建立 5～10 張不同來源的去識別化回歸樣本。
- [ ] 字軌、日期、隨機碼、金額與紙面內容一致。
- [ ] 反光／斜拍、舊格式、無 QR 發票提供清楚錯誤，不 panic。

**Verification:**
- [ ] `twiv.exe qr <receipt.jpg>`
- [ ] `twiv.exe scan <receipt.jpg> --json`
- [ ] 手動比對紙面欄位。

**Dependencies:** Task 3；需要使用者提供測試照片  
**Files likely touched:** 測試樣本與必要的 QR 修正檔案  
**Estimated scope:** M

## Task 8: 建立 Private Git repository 並推送

**Description:** 將原始碼放入 Private GitHub repository，保留公開政府資料的獨立下載通道。

**Acceptance criteria:**
- [ ] 本地 main 分支有初始 commit。
- [ ] `origin` 指向正確的 Private repository。
- [ ] GitHub 頁面可看到完整來源與 workflow。

**Verification:**
- [ ] `git status`
- [ ] `git remote -v`
- [ ] `git push -u origin main`

**Dependencies:** Task 4  
**Files likely touched:** `.gitignore`、repository metadata  
**Estimated scope:** S

## Task 9: 通過 CI 並建立第一個 Release

**Description:** 讓三平台測試通過，再以 `v0.1.0` tag 觸發 Windows／Linux／macOS Release。

**Acceptance criteria:**
- [ ] GitHub Actions test matrix 全綠。
- [ ] Release workflow 上傳四個平台產物。
- [ ] Windows 產物在乾淨電腦可啟動。

**Verification:**
- [ ] GitHub Actions workflow 頁面。
- [ ] Release assets 含 `twiv-windows-x64.exe`。
- [ ] 乾淨 Windows 手動開啟與 CLI 對獎。

**Dependencies:** Task 8；建議先完成 Task 5  
**Files likely touched:** `.github/workflows/build.yml`（僅 CI 失敗時）  
**Estimated scope:** S

## Checkpoint: 可交付版本

- [ ] Task 4–9 完成。
- [ ] 下載成品不需要 Rust 或 VC++ Redistributable。
- [ ] 更新通道不暴露私有原始碼。

## Task 10: 設定本機 MCP server

**Description:** 以 Release 的絕對路徑執行 `twiv.exe mcp`，接到 Codex／桌面 AI 工具。

**Acceptance criteria:**
- [ ] MCP client 可完成 initialize。
- [ ] `tools/list` 能看到對獎工具。
- [ ] `tools/call` 可取得真實對獎結果。

**Verification:**
- [ ] MCP 設定的 command 是存在的絕對路徑。
- [ ] 測試 `check_invoice` 或等效工具。

**Dependencies:** Task 3 或 Task 9  
**Files likely touched:** 使用者的 MCP client 設定  
**Estimated scope:** S

## Task 11: 安裝並驗證 Skill

**Description:** 把 `skill/` 放入目標技能目錄，並把 Windows binary 放進 `skill/scripts/` 或加入 PATH。

**Acceptance criteria:**
- [ ] `run.ps1 status --json` 找得到 `twiv.exe`。
- [ ] 自然語言提到統一發票時可觸發 Skill。
- [ ] Skill 回答一律以工具輸出為準。

**Verification:**
- [ ] `./skill/scripts/run.ps1 status --json`
- [ ] `./skill/scripts/run.ps1 check 89996565 --period 11504`

**Dependencies:** Task 3 或 Task 9  
**Files likely touched:** `skill/scripts/twiv.exe` 或使用者 PATH  
**Estimated scope:** S

## Task 12: 部署 Custom GPT Action（選配）

**Description:** 將 HTTP server 放到公開 HTTPS 環境，更新 OpenAPI server URL 後匯入 Custom GPT。

**Acceptance criteria:**
- [ ] 公開 HTTPS `/api/status` 與 `/api/check` 可用。
- [ ] `openapi.yaml` 不再使用 `your-domain.example.com`。
- [ ] Custom GPT 能完成一次 Action 對獎。

**Verification:**
- [ ] 從外部網路呼叫 HTTPS API。
- [ ] 在 Custom GPT 測試 operation。

**Dependencies:** Task 4、Task 9；需要部署主機與網域  
**Files likely touched:** `openapi/openapi.yaml`、部署設定  
**Estimated scope:** M

## Checkpoint: AI 整合完成

- [ ] 所有需要的任務與驗收條件完成。
- [ ] 文件已按實際指令輸出修正。
- [ ] 可清楚區分一般獎、雲端獎 `not_loaded` 與真正未中獎。

## Task 13: 實作 fetch 驗證模式與開獎後告警

**Description:** 新增真正的 `fetch --check` 或等效驗證命令，並在 GitHub Actions 安排開獎後重試。只抓取不通知失敗不算完成。

**Acceptance criteria:**
- [ ] 抓取、解析、領獎期間交叉驗證任一步失敗即回傳非零 exit code。
- [ ] 開獎後最新期別未前進時也回傳失敗。
- [ ] 每個開獎月 25～27 日自動重試，最終失敗會通知維護者。

**Verification:**
- [ ] 以舊 HTML fixture 模擬結構改版，workflow 必須失敗。
- [ ] 以仍停在前一期的候選資料驗證 stale-data 告警。
- [ ] 人工觸發 `workflow_dispatch` 並確認通知通道。

**Dependencies:** Task 5、Task 8  
**Files likely touched:** `crates/invoice-bin/src/main.rs`, `crates/invoice-scrape/src/`, `.github/workflows/`, fixtures  
**Estimated scope:** M

## Task 14: 建立日期與法規 golden fixtures

**Description:** 把官方案例固定成測試資料，防止期別、跨年、獎別或稅率靜默算錯。

**Acceptance criteria:**
- [ ] 測試期別結束月次月 25 日及 11～12 月跨年案例。
- [ ] 測試 115→2026、11601 等年度切換。
- [ ] 獎別、所得稅、印花稅、領獎期限與統編限制有來源及查核日期。

**Verification:**
- [ ] `cargo test -p invoice-core`
- [ ] 每次發版依官方公告人工複核；至少每六個月一次。

**Dependencies:** Task 2  
**Files likely touched:** `crates/invoice-core/src/period.rs`, `civil.rs`, `rules.rs`, fixtures、文件  
**Estimated scope:** M

## Task 15: 建立真實發票 QR 回歸樣本集

**Description:** 收集 5～10 張不同通路、不同拍攝條件的去識別化發票，驗證真實世界 QR 流程。

**Acceptance criteria:**
- [ ] 包含超商、全聯、加油站、餐飲等來源。
- [ ] 包含清晰、斜拍、反光、裁切與不支援格式。
- [ ] fixture 存放位置有隱私與存取控制，不直接公開敏感發票內容。

**Verification:**
- [ ] 每個樣本都有預期欄位與結果。
- [ ] `twiv qr`、`twiv scan` 回歸結果可自動比對。

**Dependencies:** Task 7  
**Files likely touched:** Private fixtures、`crates/invoice-qr/` 測試與文件  
**Estimated scope:** M

## Task 16: 建立 PDF 解析監控與索引保留政策

**Description:** 監控 PDF 連結、命名、排版與解析筆數，並限制使用者端只保留當期與上一期索引。

**Acceptance criteria:**
- [ ] 每期記錄各獎別來源 PDF、檔案大小、解析筆數及 SHA-256。
- [ ] encode／decode round-trip 與抽樣號碼查詢一致。
- [ ] 舊索引歸檔，不讓使用者端資料每年無限增加。

**Verification:**
- [ ] 連結或欄位排版改變時測試失敗。
- [ ] 使用者端僅保留兩期索引，舊資料可從歸檔取得。

**Dependencies:** Task 6  
**Files likely touched:** `crates/invoice-scrape/src/pdf.rs`, 索引工具、manifest、文件  
**Estimated scope:** M

## Task 17: 強化更新供應鏈與資料新鮮度顯示

**Description:** 限制 manifest 可指定的下載位置，使用安全寫入，並讓 UI 永遠顯示資料新鮮度。

**Acceptance criteria:**
- [ ] dataset／cloud URL 僅允許 HTTPS allowlist host 與 path。
- [ ] 下載後先驗 SHA-256，再以暫存檔原子替換；失敗保留舊資料。
- [ ] UI 顯示期別、`generated_at`、來源及 cloud loaded/not_loaded。

**Verification:**
- [ ] 惡意 host、HTTP URL、錯誤雜湊與截斷下載均被拒絕。
- [ ] 失敗後原本資料仍可正常對獎。
- [ ] 瀏覽器狀態列可直接看出資料時間。

**Dependencies:** Task 4、Task 5  
**Files likely touched:** `crates/invoice-bin/src/store.rs`, `web/app.js`, `web/index.html`, 測試  
**Estimated scope:** M

## Task 18: 固定工具鏈與半年維護檢查

**Description:** 建立可重現建置與半年一次的依賴、協定、法規檢查。

**Acceptance criteria:**
- [ ] `Cargo.lock` 納入版控，`rust-toolchain.toml` 鎖定已驗證的確切 Rust 版本。
- [ ] 每六個月執行 `cargo audit` 並檢查 MCP／OpenAPI 相容性。
- [ ] Linux 產物聲明最低 glibc，或另外提供 musl 版本。

**Verification:**
- [ ] 乾淨環境可重現相同版本建置。
- [ ] 舊版支援 Linux 或 musl 測試成功。
- [ ] 半年檢查結果有日期、負責人與處理紀錄。

**Dependencies:** Task 9  
**Files likely touched:** `rust-toolchain.toml`, `Cargo.lock`, `.github/workflows/`, 維護紀錄  
**Estimated scope:** M

## Checkpoint: 可持續維護

- [ ] Task 13–18 全部完成。
- [ ] 開獎後監控有明確的失敗通知。
- [ ] 規則錯誤、真實 QR、PDF 改版與供應鏈異常都有可重現測試。
- [ ] 固定維護節奏記錄於 `docs/07-MAINTENANCE.md`。
