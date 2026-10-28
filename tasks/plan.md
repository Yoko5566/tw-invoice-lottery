# Implementation Plan: Windows 可執行版與 ChatGPT 整合

## Overview

先把目前的 Rust 原始碼在 Windows 編譯成可執行、可驗證的 `twiv.exe`，再補齊資料更新通道、GitHub 私有原始碼與跨平台 Release，最後接上 MCP／Skill。Custom GPT Action 需要公開 HTTPS，列為選配，不阻塞本機使用。

## Architecture Decisions

- 第一個里程碑只要求「Windows 本機 App、CLI 對獎、內嵌資料」可用；雲端專屬獎索引和公開 API 不阻塞第一版。
- Windows 採專案既有的 `x86_64-pc-windows-msvc` 設定；`.cargo/config.toml` 已要求靜態 CRT。
- 原始碼 repository 建議維持 Private；政府公開資料的 manifest、`draws.json` 和雲端索引改放可匿名下載的公開資料 repository、Release assets 或物件儲存空間。
- GitHub owner 已確認為 `Yoko5566`；原始碼使用 `Yoko5566/tw-invoice-lottery`，公開資料使用 `Yoko5566/tw-invoice-lottery-data`。
- 先完成 MCP，因為它只需本機 `twiv.exe`；Skill 次之；Custom GPT Action 最後做，因為它需要公開 HTTPS 主機。
- 不把「生成 QR 圖測試通過」等同於「真實發票辨識率 100%」；必須用實體發票照片驗收。

## Dependency Order

```text
Windows C++ Build Tools + rustup
  -> 本機編譯與測試
    -> CLI / 網頁 App 冒煙測試
      -> 更新網址與資料通道
        -> 一般獎資料更新 / 雲端索引交付
          -> GitHub CI / Release
            -> MCP -> Skill -> Custom GPT Action（選配）
```

## Task List

### Phase 1: 先讓 Windows 本機版能用

- [x] Task 1：安裝 Windows Rust 建置環境
- [x] Task 2：完成基準測試與 Release 編譯
- [x] Task 3：驗證 CLI 與本機網頁 App

### Checkpoint: Windows 基準版

- [x] `target/release/twiv.exe` 存在且可執行
- [x] 一般獎 CLI 對獎結果正確
- [x] `http://127.0.0.1:8787` 可開啟並完成手動輸入對獎
- [x] 尚未載入雲端索引時，介面明確顯示 `not_loaded`，不誤報未中獎

### Phase 2: 補齊資料更新與真實情境驗證

- [ ] Task 4：決定公開資料下載位置並移除 `CHANGE_ME`
- [ ] Task 5：更新一般獎資料並建立可驗證 manifest
- [ ] Task 6：建立／發佈雲端專屬獎索引
- [ ] Task 7：使用真實發票照片驗證 QR 流程

### Checkpoint: 資料與正確性

- [ ] App 的「檢查並更新」可從匿名 HTTPS 下載資料並通過 SHA-256
- [ ] `status --json` 顯示資料來源、期別與雲端索引狀態
- [ ] 真實發票掃描成功，或已記錄可重現的失敗樣本與待修問題

### Phase 3: GitHub 與可下載成品

- [ ] Task 8：建立 Private Git repository 並推送
- [ ] Task 9：通過 GitHub Actions 並建立第一個 Release

### Checkpoint: 可交付版本

- [ ] Windows、Linux、macOS CI 測試成功
- [ ] `v0.1.0` Release 含 `twiv-windows-x64.exe`
- [ ] 在另一台 Windows 電腦不安裝 Rust也能執行

### Phase 4: ChatGPT／Codex 整合

- [ ] Task 10：設定本機 MCP server
- [ ] Task 11：安裝並驗證 Skill
- [ ] Task 12（選配）：部署公開 HTTPS 與 Custom GPT Action

### Checkpoint: 整合完成

- [ ] MCP 的 `initialize`、`tools/list`、`tools/call` 可用
- [ ] 使用自然語言可觸發 Skill 或 MCP 對獎
- [ ] 若做 Custom GPT Action，OpenAPI `servers.url` 已替換且 HTTPS API 可存取

### Phase 5: 長期維護與失敗監控

- [ ] Task 13：實作 fetch 驗證模式與開獎後告警
- [ ] Task 14：建立日期／法規 golden fixtures
- [ ] Task 15：建立真實發票 QR 回歸樣本集
- [ ] Task 16：建立 PDF 解析監控與索引保留政策
- [ ] Task 17：強化更新供應鏈與資料新鮮度顯示
- [ ] Task 18：固定工具鏈與半年維護檢查

### Checkpoint: 可持續維護

- [ ] 開獎後 24 小時內自動驗證；失敗或期別未前進會通知
- [ ] UI 顯示資料期別、`generated_at`、來源與雲端索引狀態
- [ ] 真實 QR、舊 HTML、PDF 與跨年日期都有回歸 fixture
- [ ] 維護節奏與責任記錄於 `docs/07-MAINTENANCE.md`

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Windows 缺少 MSVC linker／SDK | 無法編譯 | 安裝 Visual Studio Build Tools 的 Desktop development with C++ workload 與 Windows SDK |
| Private GitHub raw URL 無法匿名讀取 | 更新按鈕失效 | 原始碼 Private，政府公開資料另放公開資料通道；或實作授權下載 |
| `manifest.json` 的 `cloud` 目前為空 | 雲端獎永遠 `not_loaded` | 新增索引產生／上傳流程，填入 URL、bytes、SHA-256 |
| 文件宣稱的測試數與目前原始碼不一致 | 驗收誤判 | 以實際 `cargo test` 輸出為準，不以文件數字為準 |
| QR 只有生成圖片測試 | 真實照片可能失敗 | 用不同角度、反光、裁切的實體發票建立測試樣本 |
| `fetch --check` 尚未實作且 CI 沒有 cron | 改版或舊資料可能靜默交付 | 實作驗證模式、期別新鮮度檢查、排程重試與失敗通知 |
| manifest 可指定任意下載 URL | 更新通道被重新導向惡意來源 | 對 scheme、host 與 path 實施 allowlist，所有內容驗 SHA-256 |
| UI 未顯示 `generated_at` | 使用者無法判斷資料是否過期 | 在狀態列固定顯示期別、抓取時間與來源 |
| Smart App Control／防毒阻擋未簽章 EXE | 使用者無法啟動 | 內部測試先允許檔案；正式散布考慮程式碼簽章 |

## Open Questions

- 第一階段是否只需要 Codex／桌面 ChatGPT 的 MCP，還是一定要支援手機版 Custom GPT？
- 是否已有可供測試的真實電子發票照片？
- 開獎後告警要用 GitHub 通知、issue、Email，還是 webhook？
- 去識別化 QR fixture 要存放在 Private repository、加密檔案庫，還是本機測試目錄？

## Definition of Done

- Windows 使用者可下載單一 `twiv.exe`，雙擊後開啟本機 UI。
- CLI、HTTP API、MCP 對相同輸入回傳一致結果。
- 一般獎與雲端獎資料的來源、雜湊及更新時間可稽核。
- 原始碼可維持私有，公開資料不包含私有程式碼。
- 文件中的操作指令已用乾淨環境實際重跑。
