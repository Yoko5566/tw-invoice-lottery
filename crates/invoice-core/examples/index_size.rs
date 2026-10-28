//! 量測雲端獎索引在真實規模下的體積（不連網，用模擬分布）。
//! 用法：cargo run --release -p invoice-core --example index_size

use invoice_core::cloud_index::CloudTier;
use invoice_core::rules::Prize;

fn main() {
    // 實測：115年07-08期 500 元獎共 4,350,000 筆，分布於 676 個英文字軌
    let total: u64 = 4_350_000;
    let prefixes: u64 = 676;
    let per_prefix = total / prefixes;

    let mut raw = Vec::with_capacity(total as usize);
    let mut seed: u64 = 0x2F6E2B1;
    for p in 0..prefixes {
        for _ in 0..per_prefix {
            // 線性同餘產生 0..1e8 的偽隨機數字，模擬真實隨機中獎分布
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let digits = (seed >> 33) % 100_000_000;
            raw.push((p << 27) | digits);
        }
    }

    let t = CloudTier::build_from_unsorted(Prize::Cloud500, raw);
    let bytes = t.size_bytes();
    println!("筆數      : {}", t.count);
    println!("索引大小  : {:.2} MB", bytes as f64 / 1_048_576.0);
    println!("每筆平均  : {:.2} bytes", bytes as f64 / t.count as f64);

    let mut buf = Vec::new();
    t.encode(&mut buf);
    println!("序列化後  : {:.2} MB", buf.len() as f64 / 1_048_576.0);

    let back = CloudTier::decode(&buf).unwrap().0;
    println!("解碼回讀  : {} 筆", back.count);
}
