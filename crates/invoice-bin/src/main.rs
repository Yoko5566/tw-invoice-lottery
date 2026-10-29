//! `twiv` — 台灣統一發票對獎工具（單一執行檔）。
//!
//! 一個執行檔同時扮演四種角色：
//!
//! - **桌面／網頁 App**：`twiv`（啟動本機服務並開啟瀏覽器）
//! - **命令列工具**：`twiv check AB12345678`
//! - **MCP server**：`twiv mcp`（給 ChatGPT / Codex / Claude 呼叫）
//! - **資料維護工具**：`twiv fetch`（開發者重新抓取官方號碼）

mod engine;
mod mcp;
mod server;
mod store;
#[cfg(test)]
mod testutil;

use invoice_core::json;
use std::process::ExitCode;

const USAGE: &str = "\
twiv — 台灣統一發票對獎工具

用法：
  twiv                          啟動本機 App（自動開瀏覽器）
  twiv serve [--port 8787]      啟動本機 App（不自動開瀏覽器）
  twiv check <號碼> [--period 11504] [--json]
                                對獎；號碼可為 10 碼字軌或 8 碼紙本
  twiv scan <圖片路徑> [--json]  讀取發票照片的 QR 並對獎
  twiv qr <圖片路徑>            只印出 QR 原始內容（除錯用）
  twiv status [--json]          顯示資料版本與可用期別
  twiv fetch [--out 路徑] [--check]
                                重新抓取官方中獎號碼；--check 會拒絕過期資料
  twiv cloud [--period 11504]   下載雲端發票專屬獎名單並建立索引
  twiv publish-data [--out-dir data] [--base-url 網址]
                                維護者：抓取、驗證並產生資料集／雲端索引／清單
  twiv mcp                     以 MCP server 模式執行（stdio）
  twiv version                  顯示版本

環境變數：
  TWIV_DATA_DIR   覆寫資料目錄
  TWIV_MANIFEST   覆寫更新清單網址
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("app");
    let rest = &args[args.len().min(1)..];

    let result = match cmd {
        "app" => run_app(rest, true),
        "serve" => run_app(rest, false),
        "check" => run_check(rest),
        "scan" => run_scan(rest),
        "qr" => run_qr(rest),
        "status" => run_status(rest),
        "fetch" => run_fetch(rest),
        "cloud" => run_cloud(rest),
        "publish-data" => run_publish_data(rest),
        "mcp" => {
            let s = store::Store::load();
            mcp::run(s).map_err(|e| e.to_string())
        }
        "version" | "--version" | "-V" => {
            println!("twiv {}", invoice_core::VERSION);
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => {
            eprintln!("未知指令：{other}\n");
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("錯誤：{e}");
            ExitCode::FAILURE
        }
    }
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn positional(args: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a.starts_with("--") {
            if matches!(a.as_str(), "--period" | "--port" | "--out") {
                skip = true;
            }
            continue;
        }
        out.push(a);
    }
    out
}

fn run_app(args: &[String], open: bool) -> Result<(), String> {
    let port: u16 = flag_value(args, "--port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(8787);
    let s = store::Store::load();
    let latest = s
        .dataset
        .latest()
        .map(|d| d.period.label())
        .unwrap_or_else(|| "（無資料）".into());
    eprintln!("統一發票對獎台 — 資料期別：{latest}");
    eprintln!("服務位址寫在下一行，按 Ctrl+C 結束。");

    // 先把網址印出來，再開瀏覽器
    let handle = std::thread::spawn(move || server::serve(s, port));
    std::thread::sleep(std::time::Duration::from_millis(120));
    let url = format!("http://127.0.0.1:{}", if port == 0 { 8787 } else { port });
    if open {
        server::open_browser(&url);
    }
    match handle.join() {
        Ok(r) => r.map(|_| ()),
        Err(_) => Err("服務執行緒異常結束".into()),
    }
}

fn run_check(args: &[String]) -> Result<(), String> {
    let pos = positional(args);
    let code = pos.first().ok_or("請提供發票號碼")?;
    let period = flag_value(args, "--period");
    let mut s = store::Store::load();
    let r = engine::check_code(&mut s, code, period.as_deref())?;

    if has_flag(args, "--json") {
        println!("{}", r.to_json(None).to_pretty());
    } else {
        println!("發票號碼：{}", r.code);
        println!("期別　　：{}（{}）", r.period.label(), r.period.code());
        println!(
            "領獎期間：{:04}-{:02}-{:02} ~ {:04}-{:02}-{:02}",
            r.redeem_start.y,
            r.redeem_start.m,
            r.redeem_start.d,
            r.redeem_end.y,
            r.redeem_end.m,
            r.redeem_end.d
        );
        if let Some(h) = r.best() {
            println!(
                "結果　　：中獎！{} NT${}",
                h.prize.label(),
                h.prize.amount()
            );
            for x in &r.hits {
                println!("          - {}（{}）", x.detail, x.prize.label());
            }
        } else {
            println!("結果　　：未中獎");
        }
        let cloud = match r.cloud {
            invoice_core::dataset::CloudOutcome::Hit(_) => "中獎",
            invoice_core::dataset::CloudOutcome::Miss => "未中獎",
            invoice_core::dataset::CloudOutcome::NotLoaded => "未載入名單",
        };
        println!("雲端專屬獎：{cloud}");
        for n in &r.notes {
            println!("注意　　：{n}");
        }
    }
    Ok(())
}

fn run_scan(args: &[String]) -> Result<(), String> {
    let pos = positional(args);
    let path = pos.first().ok_or("請提供圖片路徑")?;
    let bytes = std::fs::read(path.as_str()).map_err(|e| format!("讀取 {path} 失敗：{e}"))?;
    let payloads = invoice_qr::decode_image_bytes(&bytes).map_err(|e| e.to_string())?;
    let q = invoice_qr::from_payloads(&payloads)
        .map_err(|e| format!("{e}\n解到的 QR 內容：{}", payloads.join(" | ")))?;

    let mut s = store::Store::load();
    let period = q.period().code();
    let r = engine::check_code(&mut s, &q.code.to_string(), Some(&period))?;

    if has_flag(args, "--json") {
        let mut j = r.to_json(Some(q.has_buyer_tax_id()));
        if let json::Json::Obj(m) = &mut j {
            m.insert(
                "issue_date".into(),
                json::s(format!(
                    "{:04}-{:02}-{:02}",
                    q.issue_date.y, q.issue_date.m, q.issue_date.d
                )),
            );
            m.insert("random_code".into(), json::s(&q.random_code));
            m.insert("total_amount".into(), json::n(q.total_amount as f64));
        }
        println!("{}", j.to_pretty());
    } else {
        println!("字軌　　：{}", q.code);
        println!(
            "開立日期：{:04}-{:02}-{:02}（{}）",
            q.issue_date.y,
            q.issue_date.m,
            q.issue_date.d,
            q.period().label()
        );
        println!("隨機碼　：{}", q.random_code);
        println!("總計額　：NT${}", q.total_amount);
        if q.has_buyer_tax_id() {
            println!(
                "警告　　：已打買受人統編 {}，依規定不得領獎",
                q.buyer_tax_id
            );
        }
        match r.best() {
            Some(h) => println!(
                "結果　　：中獎！{} NT${}",
                h.prize.label(),
                h.prize.amount()
            ),
            None => println!("結果　　：未中獎"),
        }
        for n in &r.notes {
            println!("注意　　：{n}");
        }
    }
    Ok(())
}

fn run_qr(args: &[String]) -> Result<(), String> {
    let pos = positional(args);
    let path = pos.first().ok_or("請提供圖片路徑")?;
    let bytes = std::fs::read(path.as_str()).map_err(|e| format!("讀取失敗：{e}"))?;
    let payloads = invoice_qr::decode_image_bytes(&bytes).map_err(|e| e.to_string())?;
    for p in payloads {
        println!("[{}] {}", p.len(), p);
    }
    Ok(())
}

fn run_status(args: &[String]) -> Result<(), String> {
    let s = store::Store::load();
    if has_flag(args, "--json") {
        println!("{}", s.status().to_pretty());
        return Ok(());
    }
    let st = s.status();
    println!("版本　　：twiv {}", invoice_core::VERSION);
    println!("資料來源：{}", st.str_at("dataset_origin").unwrap_or("-"));
    println!("更新時間：{}", st.str_at("generated_at").unwrap_or("-"));
    println!(
        "雲端名單：{}",
        if st
            .get("cloud_loaded")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            format!(
                "已載入（{} 筆）",
                st.get("cloud_total")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as u64
            )
        } else {
            "未載入".to_string()
        }
    );
    println!();
    for d in &s.dataset.periods {
        let (a, b) = d.period.redeem_window();
        println!(
            "{}  特別獎 {}  特獎 {}  頭獎 {}  領獎 {:04}-{:02}-{:02}~{:04}-{:02}-{:02}",
            d.period.label(),
            d.special,
            d.grand,
            d.first.join(" / "),
            a.y,
            a.m,
            a.d,
            b.y,
            b.m,
            b.d
        );
    }
    Ok(())
}

fn run_fetch(args: &[String]) -> Result<(), String> {
    let out = flag_value(args, "--out").unwrap_or_else(|| "data/draws.json".into());
    let pages = invoice_scrape::fetch_current_and_last().map_err(|e| e.to_string())?;
    let ds = invoice_scrape::build_dataset(&pages);
    if has_flag(args, "--check") {
        invoice_scrape::validate_dataset_freshness(&ds, invoice_scrape::today_utc())
            .map_err(|e| e.to_string())?;
    }
    let text = ds.to_json().to_pretty();
    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(&out, &text).map_err(|e| e.to_string())?;
    println!(
        "已更新 {}（{} 期）{}",
        out,
        ds.periods.len(),
        if has_flag(args, "--check") {
            "，新鮮度檢查通過"
        } else {
            ""
        }
    );
    Ok(())
}

fn run_cloud(args: &[String]) -> Result<(), String> {
    let mut s = store::Store::load();
    let code = flag_value(args, "--period")
        .or_else(|| s.dataset.latest().map(|d| d.period.code()))
        .ok_or("資料集中沒有期別")?;
    println!("正在下載並建立 {code} 期的雲端發票專屬獎索引…");
    let r = s.update_cloud(&code)?;
    println!(
        "完成：{}（{:.2} MB）\n存放於 {}",
        r.latest,
        r.bytes as f64 / 1_048_576.0,
        s.dir.display()
    );
    Ok(())
}

fn run_publish_data(args: &[String]) -> Result<(), String> {
    let out_dir = flag_value(args, "--out-dir").unwrap_or_else(|| "data".into());
    let base = flag_value(args, "--base-url").unwrap_or_else(|| {
        "https://raw.githubusercontent.com/Yoko5566/tw-invoice-lottery-data/main".into()
    });
    let base = base.trim_end_matches('/');
    let out_dir = std::path::PathBuf::from(out_dir);
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    println!("[1/3] 抓取並驗證一般獎號碼…");
    let pages = invoice_scrape::fetch_current_and_last().map_err(|e| e.to_string())?;
    let ds = invoice_scrape::build_dataset(&pages);
    invoice_scrape::validate_dataset_freshness(&ds, invoice_scrape::today_utc())
        .map_err(|e| e.to_string())?;
    let draws = ds.to_json().to_pretty().into_bytes();

    println!("[2/3] 下載官方 PDF 並建立雲端專屬獎索引（可能需要數分鐘）…");
    let cloud =
        invoice_scrape::fetch_cloud_index("cloudNowNumber.html").map_err(|e| e.to_string())?;
    let latest = ds.latest().ok_or("一般獎資料集是空的")?.period;
    if cloud.period != latest {
        return Err(format!(
            "一般獎期別 {} 與雲端獎期別 {} 不一致，拒絕發布",
            latest.code(),
            cloud.period.code()
        ));
    }
    let cloud_bytes = cloud.index.encode();
    let cloud_name = format!("cloud_{}.bin", cloud.period.code());
    let dataset_url = format!("{base}/data/draws.json");
    let cloud_url = format!("{base}/data/{cloud_name}");
    let manifest = invoice_scrape::build_manifest(
        &ds,
        &dataset_url,
        &draws,
        Some((&cloud, &cloud_url, &cloud_bytes)),
        invoice_core::VERSION,
    );

    println!("[3/3] 寫入已驗證產物…");
    std::fs::write(out_dir.join("draws.json"), &draws).map_err(|e| e.to_string())?;
    std::fs::write(out_dir.join(&cloud_name), &cloud_bytes).map_err(|e| e.to_string())?;
    std::fs::write(out_dir.join("manifest.json"), manifest.to_pretty())
        .map_err(|e| e.to_string())?;
    println!(
        "完成：{}，雲端索引 {} 筆 / {:.2} MB",
        latest.label(),
        cloud.index.total(),
        cloud_bytes.len() as f64 / 1_048_576.0
    );
    Ok(())
}
