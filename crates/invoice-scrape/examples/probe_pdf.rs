//! 手動驗證：給一個官方 PDF 網址，印出抽到的字軌數量與樣本。
//! 用法：cargo run -p invoice-scrape --example probe_pdf -- <pdf-url>

use invoice_scrape::{net, pdf};

fn main() {
    let url = std::env::args().nth(1).expect("請提供 PDF 網址");
    let t0 = std::time::Instant::now();
    let bytes = net::get_bytes(&url, 512 * 1024 * 1024).expect("下載失敗");
    let dl = t0.elapsed();
    let t1 = std::time::Instant::now();
    let text = pdf::extract_text(&bytes).expect("PDF 解析失敗");
    let codes = pdf::extract_codes(&text);
    println!(
        "下載 {:.1} MB 於 {:?}；解析 {:?}；抽到 {} 筆字軌",
        bytes.len() as f64 / 1_048_576.0,
        dl,
        t1.elapsed(),
        codes.len()
    );
    for c in codes.iter().take(5) {
        println!("  {c}");
    }
}
