# 完整建置步驟（從零到可推上 GitHub 的封裝包）

> 這份文件是可以照著敲的路線圖。每一階段都標了「驗收條件」，
> 沒通過就不要進下一階段——這個專案最容易出錯的地方是**期別換算**與**期別定位**，
> 錯了會把錯的領獎期限告訴使用者，比對不到獎還糟。

---

## 階段 0：環境準備

| 工具 | 版本 | 用途 |
|---|---|---|
| Rust | stable ≥ 1.80 | 全部程式碼 |
| Git | 任意 | 版控 |
| `qrencode`（選用） | 4.x | 造測試 QR 圖 |
| `curl` | 任意 | 手動驗證資料源 |

```bash
# Windows（PowerShell）
winget install Rustlang.Rustup
rustup default stable

# macOS / Linux
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable

rustc --version    # 應該 ≥ 1.80
cargo --version
```

**驗收**：`cargo --version` 有輸出。

---

## 階段 1：先驗證資料源，不要先寫程式

> 順序很重要。很多這類專案死在「寫完才發現官方頁面抓不到」。
> 先用手動命令確認資料拿得到、格式長什麼樣。

```bash
# 1) 當期號碼單（確認有 特別獎 / 特獎 / 頭獎 三段）
curl -s https://invoice.etax.nat.gov.tw/index.html | grep -o '特別獎' | head -1

# 2) 確認頁面是靜態 HTML（沒有 JS 也能拿到號碼）
curl -s https://invoice.etax.nat.gov.tw/index.html | grep -oE '90[0-9]{6}|89[0-9]{6}' | head

# 3) 雲端發票專屬獎：確認是 PDF 清單
curl -s https://invoice.etax.nat.gov.tw/cloudNowNumber.html | grep -oE 'pdf/[0-9_Sorteda-zA-Z.]+\.pdf' | head

# 4) 確認 PDF 是文字型（不是掃描圖）
curl -s -o /tmp/t.pdf https://invoice.etax.nat.gov.tw/pdf/<上面抓到的檔名>
pdftotext /tmp/t.pdf - | head -20
```

**驗收**：第 4 步 `pdftotext` 能印出 `CG33998265` 這種 10 碼字軌。
若印不出來（PDF 是圖片），整個 QR/PDF 路線要改成 OCR，成本會高一個量級。

> 實測結果（115 年 07-08 月期）：
> - 特別獎 `89996565`、特獎 `91098182`、頭獎 3 組
> - 500 元獎 PDF **128.9 MB / 435 萬筆**，但 `pdftotext` 可讀 → 確定走文字解析

---

## 階段 2：把規則寫成純函式（核心層）

先寫**不碰網路、不碰檔案**的 `invoice-core`。這一層要 100% 可單元測試。

### 2.1 期別換算 `period.rs`

三個關鍵公式：

```rust
// 開獎日 = 該期結束月的次月 25 日
fn draw_date(&self) -> Ymd {
    Ymd::new(self.year(), self.end_month(), 25).add_months(1)
}

// 領獎起 = 開獎日次月 6 日
// 領獎迄 = 起 + 3 個月 - 1 天
fn redeem_window(&self) -> (Ymd, Ymd) {
    let d = self.draw_date().add_months(1);
    let start = Ymd::new(d.y, d.m, 6);
    (start, start.add_months(3).add_days(-1))
}
```

**這裡最常見的錯**：把開獎日寫成「期別起始月 25 日」。
07-08 月期是 **9/25** 開獎，不是 7/25。

**驗收**：用官方公告寫死測試

```rust
// 官方：115年07-08月期，領獎 115/10/06 ~ 116/01/05
assert_eq!(p.draw_date(), Ymd::new(2026, 9, 25));
assert_eq!(p.redeem_window(), (Ymd::new(2026,10,6), Ymd::new(2027,1,5)));

// 官方：115年05-06月期，領獎 115/08/06 ~ 115/11/05
assert_eq!(p.redeem_window(), (Ymd::new(2026,8,6), Ymd::new(2026,11,5)));
```

### 2.2 字軌壓縮 `code.rs`

10 碼 → 37 bit `u64`：`(字母索引 << 27) | 8位數字`。
字母 A=0..Z=25，`AA`=0、`ZZ`=675。

為什麼要壓：雲端獎 435 萬筆，存字串是 43 MB，存 `u64` 是 34 MB，
再差分壓縮變 10 MB。

### 2.3 獎別規則 `rules.rs`

注意三件事：
- 特別獎與特獎都必須**全 8 碼相同**
- 二～六獎是對**頭獎號碼**取末 n 碼，取到最長的為止
- 同一張發票中多獎 → **只取最高**

### 2.4 對獎引擎 `check.rs`

```rust
pub fn check_draw(draw: &Draw, code: InvoiceCode) -> Vec<Hit>
```

**驗收**（用真實號碼）：

| 輸入 | 期望 |
|---|---|
| `ZZ89996565` | 特別獎 10,000,000 |
| `AB54348835` | 頭獎 200,000 |
| `AB04348835` | 二獎 40,000（末 7 碼） |
| `AB00048835` | 四獎 4,000（末 5 碼） |
| `AB00000835` | 六獎 200（末 3 碼） |
| `AB00000035` | 未中獎（只有末 2 碼） |

```bash
cargo test -p invoice-core
```

---

## 階段 3：抓取與解析（取用層）

### 3.1 HTML 解析 `html.rs`

不用 HTML 解析器，用容錯小函式：
- `table_rows()`：抓出所有 `<tr>` 的 `<td>` 文字
- `strip_tags()`：去標籤、`<br>` 轉換行、解 `&amp;` 等實體
- `parse_period_title_for(html, page_rel)`：**用頁面檔名定位期別**

> ⚠️ 這一支是整專案最容易踩的坑：官方每頁的導覽列都列出當期與上一期，
> 直接取第一個符合的標題會抓錯。用 `href == 本頁檔名` 定位。

### 3.2 交叉驗證（很重要）

解析完不要直接信，拿官方頁面自己寫的「領獎期間自 X 起至 Y 止」
跟 `Period::redeem_window()` 比對：

```rust
if let Some((s, e)) = redeem_window {
    if draw.period.redeem_window() != (s, e) {
        return Err("領獎期間交叉驗證失敗");   // 寧可中止，不要給錯期限
    }
}
```

### 3.3 PDF 解析 `pdf.rs`

流程：
1. 掃 `stream` … `endstream`
2. `flate2::ZlibDecoder` 解 `/FlateDecode`
3. 取出所有字串運算元（`(..)Tj`、`[(..)(..)]TJ`、`<hex>`）串接
4. 掃 `[A-Z]{2}\d{8}` 樣式，去重

**驗收**：
```bash
cargo run -p invoice-scrape --example probe_pdf -- <官方 PDF 網址>
# 百萬元獎 → 30 筆
# 八百元獎 → 100000 筆
# 五百元獎 → 4350000 筆
```

---

## 階段 4：QR 解析（發票照片 → 號碼）

### 4.1 為什麼是 QR 而不是 OCR

台灣「電子發票證明聯」左側 QR 內含完整對獎資訊，欄位固定 77 字元：

| 位移 | 長度 | 內容 |
|---|---|---|
| 0–9 | 10 | 字軌號碼（2 英 + 8 數） |
| 10–16 | 7 | 開立日期（民國 yyymmdd） |
| 17–20 | 4 | 隨機碼 |
| 21–28 | 8 | 銷售額（16 進位） |
| 29–36 | 8 | 總計額（16 進位） |
| 37–44 | 8 | 買受人統編（全 0 = 未打統編） |
| 45–52 | 8 | 賣方統編 |
| 53–76 | 24 | 防偽隨機碼 |

QR 解碼準確率 100%，OCR 在反光/歪斜下可能把 `0` 讀成 `O`。

### 4.2 實作

```rust
image::load_from_memory(bytes)?.to_luma8()
  → rqrr::PreparedImage::prepare(luma)
  → detect_grids() → decode()
```

**驗收**（造一張假發票 QR）：

```bash
P="AA1234567811507011234000003E8000003E80000000012345678$(printf 'X%.0s' $(seq 1 24))"
qrencode -o /tmp/inv_qr.png -s 8 -m 4 "$P"
cargo run -p invoice-qr --example probe_qr -- /tmp/inv_qr.png
```

應輸出：字軌 `AA12345678`、開立日期 `2026-07-01`、期別 `115年07-08月`、總計額 `1000`。

> ⚠️ 上線前務必用**一張真實發票**驗證欄位邊界。
> `twiv qr <照片>` 會原樣印出 payload，方便對照紙本核對。

---

## 階段 5：雲端獎索引（讓 435 萬筆塞進 App）

### 5.1 格式

```
magic "TWCX" | version | tier_id | count
anchors[]  u64 × 塊數        （每 128 筆一個絕對值）
offsets[]  u32 × 塊數        （該塊在 deltas 的起始位移）
deltas     LEB128 varint     （塊內相鄰差值）
```

### 5.2 為什麼要 offsets

只存 anchors + deltas 的話，查詢時得從頭線性跳過前面所有差值 → O(n)，
435 萬筆會慢到不能用。加了 `offsets` 才能 O(log 塊數 + 128)。

**驗收**：
```bash
cargo run --release -p invoice-core --example index_size
# 筆數 4349255 / 索引 10.02 MB / 每筆 2.42 bytes
```

---

## 階段 6：執行檔（三種入口）

### 6.1 本機服務（手寫 HTTP）

不引入 axum/actix 的理由：只有 6 條路由、單一使用者、只綁 `127.0.0.1`。
框架會讓單檔多出數 MB。用 `TcpListener` + 執行緒即可。

路由：
```
GET  /                 內嵌 index.html
GET  /app.js /style.css
GET  /api/status       資料版本與期別
GET  /api/check?code=&period=
POST /api/check        JSON body
POST /api/scan         圖片二進位 → QR → 對獎
POST /api/update       抓官方最新號碼（驗 SHA-256）
POST /api/cloud        下載當期雲端索引
```

### 6.2 CLI

```
twiv / twiv serve / twiv check / twiv scan / twiv qr / twiv status
twiv fetch / twiv cloud / twiv mcp / twiv version
```

### 6.3 MCP server

stdio + 換行分隔 JSON-RPC 2.0。方法：`initialize`、`tools/list`、`tools/call`、`ping`。
工具：`list_periods`、`check_invoice`、`check_receipt_image`、`data_status`。

**驗收**：
```bash
printf '%s\n' \
 '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
 '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"check_invoice","arguments":{"code":"ZZ89996565","period":"11504"}}}' \
 | ./target/release/twiv mcp
```

應回傳 `"is_winner": true`、`"best_amount": 10000000`。

---

## 階段 7：前端

`web/` 三個檔案，全部內嵌進執行檔（`include_str!`）。
沒有 CDN、沒有 npm、沒有建置步驟——改完直接 `cargo build`。

要求：
- 深色維運台風格、等寬字顯示號碼
- 拖放／點選／Ctrl+V 貼上都能上傳
- 手機可用（`viewport`、單欄、大按鈕）
- 所有輸出 `escapeHtml`

---

## 階段 8：資料更新機制

```
執行檔內嵌 data/draws.json（保底，離線也能對獎）
        │
啟動時 / 按「檢查更新」
        ▼
GET manifest.json  → 比對 dataset_sha256
        ▼
不同才下載 → 驗 SHA-256 → 寫入使用者資料目錄
        ▼
下次啟動優先讀使用者目錄版本
```

資料目錄：
- Windows `%APPDATA%\twiv`
- macOS/Linux `$XDG_DATA_HOME/twiv` 或 `~/.local/share/twiv`

**驗收**：斷網啟動，`twiv status` 仍能顯示內建期別。

---

## 階段 9：測試

```bash
cargo test --workspace          # 75 項
cargo test -- --ignored         # 需要網路：抓官方頁面 + 交叉驗證
```

必測清單：

- [x] 期別 ↔ 開獎日 ↔ 領獎期間（對照官方公告）
- [x] 各獎別比對（含邊界：末 2 碼不算中獎）
- [x] 多獎取最高
- [x] 字軌壓縮 round-trip
- [x] 雲端索引 435 萬筆全數可查、間隙不誤判
- [x] JSON 轉義 round-trip
- [x] QR 圖片 → 欄位
- [x] 領獎期間與官方網頁文字一致（交叉驗證）
- [x] MCP initialize / tools/list / tools/call
- [x] 損壞輸入不 panic（錯誤的 PDF、截斷的索引、亂碼 base64）

---

## 階段 10：封裝與發佈

見 [05-CLOSED-SOURCE-PACKAGING.md](05-CLOSED-SOURCE-PACKAGING.md)。

```bash
cargo build --release -p invoice-bin
ls -lh target/release/twiv     # 實測 2.5 MB，只依賴 libc
```

---

## 階段 11：給 ChatGPT 用

三種路線，見 [04-CHATGPT-VSCODE.md](04-CHATGPT-VSCODE.md)：

1. **Skill 封裝**（`skill/SKILL.md`）— 最簡單，ChatGPT/Codex 讀檔執行
2. **MCP server**（`twiv mcp`）— 桌面版 ChatGPT / Codex 直接接
3. **Custom GPT Action**（`openapi/openapi.yaml`）— 需要公開 HTTPS 端點

---

## 階段 12：推上 GitHub

```bash
git init
git add .
git commit -m "feat: 台灣統一發票對獎工具"
git branch -M main
git remote add origin git@github.com:<你的帳號>/tw-invoice-lottery.git
git push -u origin main
```

- **要閉源** → repo 設 **Private**，只發佈 `twiv` 執行檔到 Releases
- **要公開但保留權利** → repo 設 Public，LICENSE 用專有授權（本專案預設）

CI 會自動在三個平台編譯並附上 Release（見 `.github/workflows/build.yml`）。

---

## 常見錯誤排查

| 症狀 | 原因 | 解法 |
|---|---|---|
| `parse_period_title` 抓到期別不對 | 導覽列有多期標題 | 用 `parse_period_title_for(html, 檔名)` |
| 領獎期間差一個月 | 開獎日算成起始月 | 開獎日 = 結束月的次月 25 日 |
| PDF 抽不到字軌 | 該 PDF 是掃描圖 | `pdftotext` 先驗；圖片就得改 OCR |
| 索引查詢很慢 | 沒有 `offsets`，每查一次線性掃 | 加每塊位移表 |
| 打包後 5 MB+ | 沒開 LTO / strip | 用 `Cargo.toml` 的 `[profile.release]` |
| Windows 上執行檔要求 VCRuntime | MSVC 目標 | 改用 `x86_64-pc-windows-gnu` 或靜態 CRT |
