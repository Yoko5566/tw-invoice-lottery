# 提示詞包（要自己請 GPT / Codex 重做或接手時用）

> 用途：這份專案已經實作完成。若你想在別的環境重建、或讓 AI 助手接手改寫，
> 直接照下面順序一段一段貼給 GPT / Codex。**一段 = 一輪對話**，
> 不要一次全部貼，模型會漏掉後面的約束。

---

## 使用方式

1. 開一個新對話（GPT-5 / Claude / Codex 皆可），貼「第 0 段」建立角色與約束。
2. 依序貼第 1～9 段，每段執行完、確認測試通過，再貼下一段。
3. 每段結尾都有「驗收」條件。沒過就不要往下走。

---

## 第 0 段：角色與硬性約束

```
你是資深系統工程師，要幫我實作一個「台灣統一發票對獎」工具。

硬性約束，任何情況都不得違反：
1. 主要語言用 Rust（系統語言，能編出單一執行檔、無執行期依賴）。
2. 核心運算層（期別換算、獎別比對、字軌編碼）不准引入外部 crate，只准用 std。
   理由是這些邏輯有法律後果，程式碼要能被逐行審閱。
3. 不准用 OCR 讀發票。台灣的「電子發票證明聯」左側有 QR Code，
   內含完整對獎資訊，準確率比 OCR 高。
4. 所有中獎號碼必須從財政部官方網站取得，不准寫死、不准憑記憶。
5. 領獎期間必須用「官方頁面自己寫的文字」與「程式推算的結果」交叉驗證，
   不一致就中止，不准放行。
6. 最終產物：一個約 2-3 MB 的單一執行檔，同時提供網頁 UI、CLI 與 MCP server。
7. 每個模組都要有單元測試。測試失敗時改實作，不改測試。

回答我時，先確認你理解這 7 點，再開始。
```

---

## 第 1 段：先驗證資料源（不要寫程式）

```
在我開始寫任何程式之前，先帶我驗證資料源。請給我實際可以貼進終端機執行的命令，
用來確認下列四件事：

1. 財政部稅務入口網 https://invoice.etax.nat.gov.tw/index.html 的當期中獎號碼單，
   能不能在停用 JavaScript 的情況下拿到「特別獎 / 特獎 / 頭獎」三段號碼。
2. 這個頁面的 HTML 結構長什麼樣（我要用什麼選擇器抓？表格是 <tr><td> 嗎？）。
3. 雲端發票專屬獎頁面 cloudNowNumber.html 上的中獎號碼是不是 PDF 檔？
   檔名規則是什麼？共有幾種獎級？
4. 那個 PDF 是「文字型」還是「掃描影像型」？用 pdftotext 能不能直接抽出號碼？

最後請告訴我：如果 PDF 是掃描影像型，整個技術路線要怎麼改？
```

**驗收**：`pdftotext` 能印出像 `CG33998265` 的 10 碼字軌。

---

## 第 2 段：期別換算（最容易錯的地方）

```
先實作期別模型，不碰網路。

規則：
- 台灣統一發票每兩個月一期：01-02、03-04、05-06、07-08、09-10、11-12 月。
- 期別代碼是民國年 3 碼 + 期別 2 碼，例如 115 年 07-08 月 = 11504。
- 開獎日 = 該期「結束月」的次月 25 日。（注意：07-08 月期是 9/25 開獎，不是 7/25）
- 領獎期間 = 開獎日次月 6 日起算三個月。

請實作 struct Period { roc_year: u16, slot: u8 }，提供：
code()、label()、draw_date()、redeem_window()、containing_date()、from_code()、next()、prev()。

不要引入 chrono 之類的時間 crate，自己用 Howard Hinnant 的 days_from_civil /
civil_from_days 演算法處理日期。

然後寫測試，用官方公告的實際日期當斷言：
- 115 年 07-08 月期：開獎 2026-09-25，領獎 2026-10-06 ~ 2027-01-05
- 115 年 05-06 月期：領獎 2026-08-06 ~ 2026-11-05
- 115 年 11-12 月期：開獎 2027-01-25，領獎 2027-02-06 ~ 2027-05-05

先只給我 period.rs 與它的測試。
```

---

## 第 3 段：字軌編碼與獎別規則

```
接著實作兩件事。

A. 字軌號碼：格式固定 10 碼 = 2 碼大寫英文 + 8 碼數字，例如 AB12345678。
   請壓縮成 37 bit 的 u64：高 10 bit 存字母索引（A=0..Z=25，AA=0、ZZ=675），
   低 27 bit 存 8 位數字。
   提供 parse()、from_digits()（紙本發票只有 8 碼）、to_raw()、from_raw()、
   digits()、letters()、suffix(n)。要能容忍小寫與連字號、空白。

B. 獎別規則 enum：
   特別獎 8 碼全中 1000 萬 / 特獎 8 碼全中 200 萬 / 頭獎 8 碼全中 20 萬 /
   二獎末 7 碼 4 萬 / 三獎末 6 碼 1 萬 / 四獎末 5 碼 4000 /
   五獎末 4 碼 1000 / 六獎末 3 碼 200 / 增開六獎末 3 碼 200 /
   雲端專屬獎 100 萬、2000、800、500（字軌全同）。
   附 amount()、label()、is_cloud()、subject_to_income_tax()、net_amount()。

然後實作對獎引擎 check_draw(draw, code) -> Vec<Hit>：
- 二~六獎是對「頭獎號碼」取末 n 碼，要取最長的匹配（末 7 碼中了就不再算末 6 碼）
- 同一張發票中多獎時全部回傳，但依金額由高到低排序

用這組真實號碼寫測試（115 年 07-08 月期）：
特別獎 89996565、特獎 91098182、頭獎 54348835 / 44991397 / 06595111。
驗證：AB54348835→頭獎、AB04348835→二獎、AB00048835→四獎、
AB00000835→六獎、AB00000035→未中獎。
```

---

## 第 4 段：抓官方網頁

```
實作網頁解析與抓取。

A. 手寫一個容錯的 HTML 抽取器（不要引入 scraper/html5ever）：
   - table_rows(html) -> Vec<Vec<String>>：抓出所有 <tr> 的 <td> 文字
   - strip_tags(s)：去標籤、<br> 轉換行、解 &amp; &nbsp; &#NN; 等實體
   - digit_runs(s, len)：抓出指定長度的數字串
   - parse_prize_page(html) -> Draw

B. 期別定位（重要陷阱）：
   官方 lastNumber.html 的導覽列同時列出「當期」與「上一期」兩個標題。
   如果直接取第一個符合「NNN年MM-MM月」的字串，會抓錯期別。
   請改成用「href 等於本頁檔名」來定位，函式簽名：
   parse_period_title_for(html, page_rel) -> Result<Period>

C. 交叉驗證：
   頁面上有「領獎期間自115年10月06日起至116年01月05日止」這段文字。
   解析它，跟 Period::redeem_window() 的推算結果比對，不一致就回傳錯誤。

D. 用 ureq 抓取，設 User-Agent、3 次重試、遞增退避。

寫完後用 cargo test -- --ignored 實際打官方站台驗證。
```

---

## 第 5 段：PDF 解析（雲端發票專屬獎）

```
實作 PDF 文字抽取，處理官方「雲端發票專屬獎中獎號碼清單」。

這些 PDF 是文字型的，不需要 OCR。流程：
1. 掃描位元組找 "stream" ... "endstream"（要排除 "endstream" 裡的 stream）
2. 用 flate2 的 ZlibDecoder 解 /FlateDecode 內容流
3. 從解壓後的內容流取出所有字串運算元：
   (..)Tj、[(..)-2(..)]TJ、<hex>，處理 \( \) \\ \101 等轉義
4. 串接後掃描 [A-Z]{2}[0-9]{8} 樣式，去重

要有記憶體意識：500 元獎的 PDF 是 129 MB、435 萬筆，
不要做多餘複製，解析完直接產生索引。

先給我一個 example 可以直接跑：
cargo run --example probe_pdf -- <pdf-url>
輸出下載大小、解析耗時、抽到幾筆字軌、前 5 筆。
```

**驗收**：百萬元獎 30 筆、八百元獎 100000 筆、五百元獎 4350000 筆。

---

## 第 6 段：雲端獎索引（效能的關鍵）

```
435 萬筆字軌要塞進 2-3 MB 的執行檔是不可能的，所以要做壓縮索引。

格式（自己設計 binary layout，不要用 serde）：
- magic "TWCX" + version + tier_id + count
- anchors[]：每 128 筆存一個絕對值 u64
- offsets[]：每塊在 deltas 裡的起始位移 u32
- deltas：塊內相鄰差值的 LEB128 varint

查詢 contains(raw) 要做到 O(log 塊數 + 128)：
二分找 anchors → 用 offsets 跳到該塊 → 塊內依序累加比對。

⚠️ 沒有 offsets 的話，每次查詢都要從頭線性跳過前面所有差值，會退化成 O(n)。
這點請特別注意。

寫測試驗證：
1. 435 萬筆全部都能查到
2. 非成員不會誤判
3. encode/decode round-trip
4. 索引大小 < 12 MB（實測應該 10 MB 左右，每筆 2.42 bytes）
5. 損壞的輸入要回錯誤不可以 panic
```

---

## 第 7 段：發票 QR 解析

```
實作「電子發票證明聯」左側 QR Code 解析。欄位固定 77 字元：

[0..10]   字軌號碼（2 英 + 8 數）
[10..17]  開立日期（民國 yyymmdd）
[17..21]  隨機碼
[21..29]  銷售額（16 進位）
[29..37]  總計額（16 進位）
[37..45]  買受人統編（全 0 表示未打統編）
[45..53]  賣方統編
[53..77]  防偽隨機碼

用 rqrr + image 兩個 crate 從圖片解出 QR：
image::load_from_memory → to_luma8 → rqrr::PreparedImage::prepare → detect_grids → decode

一張證明聯通常會掃到兩組 QR（左碼與右碼），
請實作 from_payloads() 自動挑出「長度 77 且前 10 碼是合法字軌」的那一組。

另外實作 has_buyer_tax_id()：買受人統編不是 00000000 就是打了統編，
依規定這種發票不得領獎，要主動提示。

驗證方式：用 qrencode 造一張 77 字元的假 QR，
然後 cargo run --example probe_qr -- <圖檔> 看能不能解出欄位。
```

---

## 第 8 段：單一執行檔（三種入口）

```
把上面組合成一個執行檔 twiv，三種模式：

1. twiv（無參數）→ 啟動本機 HTTP 服務（只綁 127.0.0.1）+ 自動開瀏覽器
2. twiv check / scan / qr / status / fetch / cloud → CLI
3. twiv mcp → MCP server（stdio，換行分隔 JSON-RPC 2.0）

HTTP 服務不要引入 axum/actix，用 std::net::TcpListener + 執行緒自己寫。
理由：只有 6 條路由、單一使用者，框架會讓單檔多出數 MB。

路由：
GET  /                    內嵌的 index.html
GET  /app.js /style.css
GET  /api/status
GET  /api/check?code=&period=
POST /api/check           JSON body
POST /api/scan            圖片二進位 → QR → 對獎（只接受 image/*，上限 32 MB）
POST /api/update          抓官方最新號碼（驗 SHA-256）
POST /api/cloud           下載當期雲端索引

MCP 方法：initialize、tools/list、tools/call、ping
MCP 工具：list_periods、check_invoice、check_receipt_image、data_status

前端用 include_str! 內嵌，深色維運台風格，無 CDN、無 npm。
上傳支援拖放 / 點選 / Ctrl+V 貼上，手機可用，所有輸出要 escapeHtml。

Cargo.toml 的 release profile：
opt-level="z"、lto=true、codegen-units=1、panic="abort"、strip=true
目標：編出來 2-3 MB。
```

---

## 第 9 段：封裝、發佈、接 ChatGPT

```
最後三件事。

A. 資料更新機制（離線優先）：
   執行檔內嵌一份 draws.json 當保底，完全離線也能對獎。
   使用者按「檢查更新」才發請求：GET manifest.json → 比對 sha256 →
   不同才下載 → 驗雜湊 → 寫入使用者資料目錄。
   Windows 用 %APPDATA%\twiv，macOS/Linux 用 ~/.local/share/twiv。

B. GitHub：
   .github/workflows/build.yml 三平台矩陣編譯（ubuntu / windows / macos），
   上傳 artifact 並發 Release。
   LICENSE 用專有授權（All Rights Reserved，只允許原樣執行）。

C. 接 ChatGPT 的三條路線，各給我一份設定檔：
   1. Skill 封裝：skill/SKILL.md，YAML frontmatter 要有 name 與 description，
      description 要把使用者可能說的觸發詞都寫進去。
   2. MCP：mcpServers 的 JSON 設定範例（Windows 與 macOS 各一份）。
   3. Custom GPT Action：openapi/openapi.yaml（OpenAPI 3.1），
      要含 checkInvoice 與 getDataStatus 兩個 operationId。

完成後給我一份 checklist，確認：
□ 75 項以上單元測試通過
□ 編出來的執行檔 < 3 MB
□ 斷網啟動仍能對獎
□ 官方頁面交叉驗證通過
```

---

## 附錄：一句話需求（要更短的話用這個）

```
幫我做一個台灣統一發票對獎工具，Rust 寫的單一執行檔（<3MB），
同時能當網頁 App、CLI 和 MCP server。

要求：
- 自動抓財政部官方每期的中獎號碼（網頁 + PDF，不用 OCR）
- 使用者上傳電子發票證明聯照片，解析左下 QR Code 直接對獎
- 支援雲端發票專屬獎（一期 435 萬筆，要壓成約 10MB 的索引）
- 完全閉源、單檔、開程式才連網檢查更新
- 領獎期間必須與官方頁面文字交叉驗證

先給我最終架構圖與專案結構，確認後再開始寫程式。
```

---

## 提醒：不要讓 AI 一次寫完

這類專案失敗的典型模式是「一次要 AI 產出整個專案」——
模型會漏掉期別換算的細節、PDF 的邊界條件、索引的效能問題，
而且你很難看出哪裡錯了。

**一段一驗收**才是能真正跑起來的做法。
特別是第 2 段（期別換算）與第 6 段（索引 offsets），
這兩處錯了系統還是能跑，但答案會是錯的——這比編譯不過危險得多。
