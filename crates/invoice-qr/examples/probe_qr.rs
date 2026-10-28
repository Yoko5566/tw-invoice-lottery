//! 手動驗證：給一張發票照片（或 QR 圖檔），印出解到的 payload 與解析結果。
//! 用法：cargo run -p invoice-qr --example probe_qr -- <圖片路徑>

fn main() {
    let path = std::env::args().nth(1).expect("請提供圖片路徑");
    let bytes = std::fs::read(&path).expect("讀檔失敗");
    let payloads = invoice_qr::decode_image_bytes(&bytes).expect("QR 解碼失敗");
    println!("解到 {} 組 QR：", payloads.len());
    for p in &payloads {
        println!("  [{}] {}", p.len(), p);
    }
    match invoice_qr::from_payloads(&payloads) {
        Ok(q) => {
            println!("--- 解析成功 ---");
            println!("字軌      : {}", q.code);
            println!(
                "開立日期  : {:04}-{:02}-{:02}",
                q.issue_date.y, q.issue_date.m, q.issue_date.d
            );
            println!("期別      : {}", q.period().label());
            println!("隨機碼    : {}", q.random_code);
            println!("總計額    : {}", q.total_amount);
            println!(
                "買受人統編: {} (打統編={})",
                q.buyer_tax_id,
                q.has_buyer_tax_id()
            );
        }
        Err(e) => println!("解析失敗：{e}"),
    }
}
