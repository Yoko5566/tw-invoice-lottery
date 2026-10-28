//! 抓取當期與上一期一般獎別號碼，輸出 `data/draws.json`。
//! 用法：cargo run -p invoice-scrape --example fetch_draws -- <輸出路徑>

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "data/draws.json".into());
    let pages = invoice_scrape::fetch_current_and_last().expect("抓取失敗");
    for p in &pages {
        println!(
            "取得 {} 特別獎={} 領獎={:?}",
            p.draw.period.label(),
            p.draw.special,
            p.redeem_window
        );
    }
    let ds = invoice_scrape::build_dataset(&pages);
    let text = ds.to_json().to_pretty();
    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(&out, &text).expect("寫檔失敗");
    println!(
        "已寫入 {out}（{} 期，{} bytes）",
        ds.periods.len(),
        text.len()
    );
}
