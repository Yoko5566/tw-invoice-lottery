# tw-invoice-lottery

台灣統一發票對獎工具。一個 Rust 執行檔同時是**桌面／網頁 App**、**命令列工具**、
**MCP server**（給 ChatGPT / Codex / Claude 呼叫），並附一份 **ChatGPT Skill** 封裝。

資料直接來自財政部稅務入口網公告，每兩個月開獎後自動抓取。

```
┌──────────────────────────────────────────────────────────┐
│  twiv（單一執行檔，2.5 MB，無執行期依賴）                  │
├──────────────────────────────────────────────────────────┤
│  twiv           → 本機 HTTP 服務 + 內嵌網頁 UI（自動開瀏覽器）│
│  twiv check ... → CLI 對獎                                 │
│  twiv scan ...  → 發票照片 → QR → 對獎                     │
│  twiv mcp       → MCP server（stdio JSON-RPC）             │
└──────────────────────────────────────────────────────────┘
```

## 為什麼這樣設計

| 需求 | 對應設計 |
|---|---|
| 自動抓每期中獎號碼 | `invoice-scrape` 解析官方靜態頁 + PDF，無 API key |
| 上傳收據照片就能兌獎 | 解析電子發票證明聯 QR（不靠 OCR，準確率 100%） |
| 網頁 + 電腦應用 | 同一支執行檔：內嵌 UI，`127.0.0.1` 提供服務 |
| 純單檔、完全閉源 | 靜態連結、LTO、strip、零執行期依賴；只發佈 binary |
| 開程式才連網更新 | 資料內嵌當保底，啟動時才比對更新清單與 SHA-256 |

## 功能

- 特別獎／特獎／頭獎／二～六獎／增開六獎比對（依《統一發票給獎辦法》）
- 雲端發票專屬獎（100 萬／2000／800／500 元）比對
  - 官方公布的是數百萬筆字軌的 PDF（單檔 129 MB）
  - 本專案壓成約 10 MB 的差分索引，每筆 2.42 bytes，查詢 O(log n + 128)
- 期別 ↔ 開獎日 ↔ 領獎期間換算，並與官方頁面文字**交叉驗證**
- 稅務提示：20% 所得稅、0.4% 印花稅、打統編不得領獎
- 領獎期限倒數

## 快速開始

```bash
git clone https://github.com/Yoko5566/tw-invoice-lottery.git && cd tw-invoice-lottery
cargo build --release
./target/release/twiv                # 開本機 App
./target/release/twiv check AB54348835 --period 11504
./target/release/twiv scan receipt.jpg
```

## 專案結構

```
crates/
  invoice-core/    期別換算、字軌壓縮、獎別規則、對獎引擎、雲端索引、JSON（零依賴）
  invoice-scrape/  官方頁面與 PDF 抓取解析、更新清單、SHA-256
  invoice-qr/      電子發票證明聯 QR 解析（左碼 77 字元）
  invoice-bin/     單一執行檔：本機服務 + CLI + MCP
web/               內嵌前端（深色維運台風格，無 CDN）
skill/             ChatGPT Skill 封裝（SKILL.md）
openapi/           Custom GPT Actions 用的 OpenAPI 3.1 schema
docs/              架構、資料來源、建置步驟、整合、封裝、提示詞包
data/draws.json    各期中獎號碼（會隨開獎更新，可進版控）
```

## 測試

```bash
cargo test --workspace            # 75 項單元測試
cargo test -- --ignored           # 需要網路的整合測試（會抓官方頁面）
```

期別換算、獎別比對、PDF 解析、QR 解析、MCP 協定都有測試覆蓋，
並包含「領獎期間與官方網頁文字一致」的交叉驗證。

## 維護

官方頁面、法規、真實發票 QR、雲端 PDF 與更新通道都屬外部變動面。
開獎後監控、失敗通知、回歸樣本與固定複核節奏請見
[維護與營運手冊](docs/07-MAINTENANCE.md)。

## 授權

專有授權，保留所有權利。允許原樣執行與個人使用；禁止修改、再散布、商業利用。
詳見 [LICENSE](LICENSE)。

號碼資料本身屬於政府公開資訊，不受本授權限制。
