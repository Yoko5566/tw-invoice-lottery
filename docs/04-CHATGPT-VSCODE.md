# 怎麼讓 ChatGPT / VSCode / Codex 用這個專案

三條路線，按「做到什麼程度」排列。**建議從路線 A 開始**，五分鐘能跑起來。

---

## 路線 A：當成 Skill 用（最簡單）

`skill/SKILL.md` 就是技能描述檔。把它跟編譯好的執行檔放一起，
交給支援技能的用戶端讀取即可。

```
skill/
├── SKILL.md        ← 技能定義（name / description / 使用說明）
├── scripts/        ← 放 twiv 執行檔或呼叫腳本
└── references/     ← 名詞對照、獎別規則
```

### 做法

1. 把 `twiv`（或 `twiv.exe`）放進 `skill/scripts/`
2. 整個 `skill/` 資料夾加入 ChatGPT 的 Skills，或放進 Codex / Claude 的技能目錄
3. 之後使用者說「幫我看這張發票有沒有中獎」，模型就會依 SKILL.md 呼叫 `twiv scan`

### SKILL.md 的關鍵寫法

```yaml
---
name: tw-invoice-lottery
description: >
  比對台灣統一發票是否中獎。當使用者提到「統一發票」「對獎」「兌獎」
  「中獎號碼」「發票 QR」，或上傳發票照片要判斷有沒有中獎時使用。
---
```

`description` 決定模型**什麼時候想到這個技能**，要把使用者可能說的話都寫進去。

---

## 路線 B：MCP server（桌面版 ChatGPT / Codex 直接接）

`twiv mcp` 是標準的 MCP server（stdio + JSON-RPC 2.0）。

### 設定（Codex / Claude Desktop 相容格式）

```json
{
  "mcpServers": {
    "tw-invoice-lottery": {
      "command": "C:\\tools\\twiv.exe",
      "args": ["mcp"],
      "env": {
        "TWIV_DATA_DIR": "C:\\Users\\你\\AppData\\Roaming\\twiv"
      }
    }
  }
}
```

macOS / Linux：

```json
{
  "mcpServers": {
    "tw-invoice-lottery": {
      "command": "/usr/local/bin/twiv",
      "args": ["mcp"]
    }
  }
}
```

### 提供的工具

| 工具 | 參數 | 用途 |
|---|---|---|
| `list_periods` | 無 | 列出所有期別與中獎號碼 |
| `check_invoice` | `code`, `period`（選填） | 比對單張發票 |
| `check_receipt_image` | `image_base64` | 上傳照片 → 解 QR → 對獎 |
| `data_status` | 無 | 資料版本 |

### 驗證連線

```bash
printf '%s\n' \
 '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
 '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
 | twiv mcp
```

---

## 路線 C：Custom GPT Action（需要公開 HTTPS）

適合想讓**手機版 ChatGPT** 也能用、而且不想在自己電腦跑執行檔的情況。

### 步驟

1. 把 `twiv serve --port 8787` 架在一台有公開網域的機器上
2. 前面掛一層反向代理（Caddy / Nginx）並上 TLS
3. ChatGPT → 建立 GPT → Configure → Actions → Import from URL 或貼上 `openapi/openapi.yaml`
4. 把 schema 裡的 `servers.url` 改成你的網域

```yaml
servers:
  - url: https://invoice.your-domain.com
```

### 最小 Caddy 設定

```
invoice.your-domain.com {
    reverse_proxy 127.0.0.1:8787
}
```

### GPT 指令建議寫法

```
你負責幫使用者比對台灣統一發票是否中獎。

規則：
1. 使用者給發票號碼或照片時，一律先呼叫 checkInvoice 工具，不要憑記憶回答。
2. 中獎時一定要告訴使用者：獎別、金額、領獎期間、是否需扣稅。
3. 未中獎就直接說未中獎。
4. 若使用者只問「這期中獎號碼是什麼」，呼叫 getDataStatus。
5. 不要編造號碼。工具沒回傳的資訊就不要說。
```

⚠️ **安全注意**：目前 `/api/*` 沒有驗證機制，適合自用。
若要公開給他人用，至少在反向代理層加上 API key 驗證或 IP 白名單。

---

## 路線 D：用 VSCode + Codex / Copilot 繼續開發

### 開專案

```bash
code tw-invoice-lottery
```

推薦安裝：`rust-analyzer`、`CodeLLDB`、`Even Better TOML`。

### 讓 AI 助手有效率地改這個專案

在專案根目錄放一份 `.github/copilot-instructions.md` 或 `AGENTS.md`，
把「不能破壞的約定」寫清楚：

```markdown
# 專案約定

- 期別換算一律走 `Period`，禁止在別處手算開獎日或領獎期間。
- 領獎期間必須與官方頁面文字交叉驗證，驗不過要中止，不可放行。
- `invoice-core` 不准引入外部依賴，也不准碰網路與檔案。
- 新增獎別請同步更新 `rules.rs`、`docs/02-DATA-SOURCES.md` 與測試。
- 任何對外 JSON 一律用 `invoice_core::json`，不要自己拼字串。
- 前端所有輸出必須經過 `escapeHtml`。
```

### 常用指令

```bash
cargo check --workspace          # 快速型別檢查
cargo clippy --workspace -- -D warnings
cargo fmt --all
cargo test --workspace
cargo run --release -- status
```

### 除錯技巧

```bash
# 看官方頁面實際抓下來長什麼樣
cargo run -p invoice-scrape --example fetch_draws -- /tmp/test.json
cat /tmp/test.json

# 看某張圖片的 QR 原始內容
twiv qr receipt.jpg

# 看雲端索引建得對不對
cargo run --release -p invoice-core --example index_size
```

---

## 選哪一條？

| 你的情況 | 選 |
|---|---|
| 自己用，想快點跑起來 | A |
| 用 Codex / Claude Desktop 寫程式，要 AI 直接查 | B |
| 想用手機 ChatGPT 查，有網域 | C |
| 要繼續改功能 | D + A |

A 和 B 可以同時用，不衝突。
