//! 雲端發票專屬獎索引。
//!
//! 問題：雲端發票專屬獎每期有數百萬組中獎字軌（500 元獎一期約 200 萬組），
//! 官方以 PDF 公布，單檔可達 130 MB 以上，不可能塞進「輕量單檔應用」。
//!
//! 解法：把每期名單預先壓成「分塊 + 差分 + LEB128」的緊湊二進位索引：
//!
//! ```text
//!   每 128 筆為一塊
//!   anchors[i] : 第 i 塊第一筆的絕對值（u64）
//!   offsets[i] : 第 i 塊在 deltas 裡的起始位移（u32）
//!   deltas     : 塊內每筆與前一筆的差值（LEB128 varint）
//! ```
//!
//! 一張 10 碼字軌壓成 37 bit，排序後差值多半落在 1~3 bytes。
//! 200 萬組約 5~8 MB，查詢為 O(log 塊數 + 128)，
//! 不必把整份名單讀進記憶體也能定位。
//!
//! 索引由建置流程（`invoice-scrape`）產生；App 只在啟動時下載／更新，
//! 執行期完全離線比對。

use crate::rules::Prize;

/// 每個區塊容納的筆數。
pub const BLOCK: usize = 128;

const MAGIC: &[u8; 4] = b"TWCX";
const VERSION: u8 = 1;

/// 單一獎級的緊湊索引。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudTier {
    pub prize: Prize,
    pub count: u32,
    anchors: Vec<u64>,
    offsets: Vec<u32>,
    deltas: Vec<u8>,
}

impl CloudTier {
    /// 由一個**已排序去重**的原始值序列建立索引。
    pub fn build(prize: Prize, sorted: &[u64]) -> Self {
        let count = sorted.len() as u32;
        let nblocks = sorted.len().div_ceil(BLOCK);
        let mut anchors = Vec::with_capacity(nblocks);
        let mut offsets = Vec::with_capacity(nblocks);
        let mut deltas = Vec::with_capacity(sorted.len() * 3);

        for chunk in sorted.chunks(BLOCK) {
            anchors.push(chunk[0]);
            offsets.push(deltas.len() as u32);
            let mut prev = chunk[0];
            for &v in &chunk[1..] {
                write_varint(&mut deltas, v.wrapping_sub(prev));
                prev = v;
            }
        }
        Self {
            prize,
            count,
            anchors,
            offsets,
            deltas,
        }
    }

    /// 由未排序序列建立（內部會排序去重）。
    pub fn build_from_unsorted(prize: Prize, mut raw: Vec<u64>) -> Self {
        raw.sort_unstable();
        raw.dedup();
        Self::build(prize, &raw)
    }

    /// 是否命中。O(log 塊數 + BLOCK)。
    pub fn contains(&self, raw: u64) -> bool {
        if self.anchors.is_empty() {
            return false;
        }
        let idx = match self.anchors.binary_search(&raw) {
            Ok(_) => return true,
            Err(0) => return false,
            Err(i) => i - 1,
        };
        let mut cur = self.anchors[idx];
        let start = idx * BLOCK;
        let remaining = (self.count as usize - start).min(BLOCK) - 1;
        let mut pos = self.offsets[idx] as usize;
        for _ in 0..remaining {
            let (d, n) = read_varint(&self.deltas[pos..]);
            pos += n;
            cur = cur.wrapping_add(d);
            if cur == raw {
                return true;
            }
            if cur > raw {
                return false;
            }
        }
        false
    }

    /// 序列化為位元組。
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.push(tier_id(self.prize));
        out.extend_from_slice(&self.count.to_le_bytes());
        out.extend_from_slice(&(self.anchors.len() as u32).to_le_bytes());
        for a in &self.anchors {
            out.extend_from_slice(&a.to_le_bytes());
        }
        for o in &self.offsets {
            out.extend_from_slice(&o.to_le_bytes());
        }
        out.extend_from_slice(&(self.deltas.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.deltas);
    }

    /// 從位元組游標解碼，回傳 (索引, 已消耗長度)。
    pub fn decode(buf: &[u8]) -> Result<(Self, usize), String> {
        if buf.len() < 18 || &buf[0..4] != MAGIC {
            return Err("雲端索引 magic 不符".into());
        }
        if buf[4] != VERSION {
            return Err(format!("雲端索引版本不支援：{}", buf[4]));
        }
        let prize = tier_from_id(buf[5]).ok_or("未知獎級代碼")?;
        let count = u32::from_le_bytes(buf[6..10].try_into().unwrap());
        let n = u32::from_le_bytes(buf[10..14].try_into().unwrap()) as usize;
        if n != (count as usize).div_ceil(BLOCK) || n > (buf.len() - 18) / 12 {
            return Err("雲端索引區塊長度無效".into());
        }
        let mut off = 14;
        let mut anchors = Vec::with_capacity(n);
        for _ in 0..n {
            anchors.push(u64::from_le_bytes(buf[off..off + 8].try_into().unwrap()));
            off += 8;
        }
        let mut offsets = Vec::with_capacity(n);
        for _ in 0..n {
            offsets.push(u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()));
            off += 4;
        }
        let dlen = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        if buf.len() < off + dlen {
            return Err("雲端索引被截斷".into());
        }
        let deltas = buf[off..off + dlen].to_vec();
        off += dlen;
        let mut position = 0;
        let mut previous = None;
        for (block, (&anchor, &offset)) in anchors.iter().zip(&offsets).enumerate() {
            if offset as usize != position || previous.is_some_and(|p| p >= anchor) {
                return Err("雲端索引排序或偏移無效".into());
            }
            let mut current = anchor;
            let size = (count as usize - block * BLOCK).min(BLOCK);
            for _ in 1..size {
                let (delta, used) = read_varint(&deltas[position..]);
                if used == 0 || delta == 0 {
                    return Err("雲端索引差分無效".into());
                }
                current = current.checked_add(delta).ok_or("雲端索引數值溢位")?;
                position += used;
            }
            previous = Some(current);
        }
        if position != deltas.len() {
            return Err("雲端索引差分長度不符".into());
        }
        Ok((
            Self {
                prize,
                count,
                anchors,
                offsets,
                deltas,
            },
            off,
        ))
    }

    /// 索引位元組大小（供壓縮率報告）。
    pub fn size_bytes(&self) -> usize {
        14 + self.anchors.len() * 12 + 4 + self.deltas.len()
    }
}

fn tier_id(p: Prize) -> u8 {
    match p {
        Prize::CloudMillion => 1,
        Prize::Cloud2000 => 2,
        Prize::Cloud800 => 3,
        Prize::Cloud500 => 4,
        _ => 0,
    }
}

fn tier_from_id(b: u8) -> Option<Prize> {
    Some(match b {
        1 => Prize::CloudMillion,
        2 => Prize::Cloud2000,
        3 => Prize::Cloud800,
        4 => Prize::Cloud500,
        _ => return None,
    })
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut b = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

fn read_varint(buf: &[u8]) -> (u64, usize) {
    let mut v = 0u64;
    let mut shift = 0;
    for (i, &b) in buf.iter().enumerate() {
        if i >= 10 || (i == 9 && b > 1) {
            return (0, 0);
        }
        v |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return (v, i + 1);
        }
        shift += 7;
    }
    (0, 0)
}

/// 一期完整的雲端發票專屬獎索引（含四個獎級）。
#[derive(Debug, Clone, Default)]
pub struct CloudIndex {
    pub tiers: Vec<CloudTier>,
}

impl CloudIndex {
    pub fn add(&mut self, tier: CloudTier) {
        self.tiers.push(tier);
    }

    pub fn total(&self) -> u32 {
        self.tiers.iter().map(|t| t.count).sum()
    }

    /// 回傳命中的最高獎級。同一張發票只取一個獎項。
    pub fn lookup(&self, raw: u64) -> Option<(Prize, u32)> {
        let mut best: Option<(Prize, u32)> = None;
        for t in &self.tiers {
            if t.contains(raw) && best.map_or(true, |(p, _)| t.prize.amount() > p.amount()) {
                best = Some((t.prize, t.count));
            }
        }
        best
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for t in &self.tiers {
            t.encode(&mut out);
        }
        out
    }

    pub fn decode(mut buf: &[u8]) -> Result<Self, String> {
        let mut tiers = Vec::new();
        while !buf.is_empty() {
            let (t, used) = CloudTier::decode(buf)?;
            if used == 0 {
                return Err("雲端索引解析無進展".into());
            }
            tiers.push(t);
            buf = &buf[used..];
        }
        Ok(Self { tiers })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_truncated_index_prefixes_return_error() {
        let tier = CloudTier::build(Prize::Cloud500, &sample());
        let mut bytes = Vec::new();
        tier.encode(&mut bytes);
        for len in 0..bytes.len() {
            assert!(CloudTier::decode(&bytes[..len]).is_err());
        }
        let mut forged = bytes.clone();
        forged[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(CloudTier::decode(&forged).is_err());
    }

    fn sample() -> Vec<u64> {
        (0..5_000u64).map(|i| i * 7 + 11).collect()
    }

    #[test]
    fn build_and_hit() {
        let raw = sample();
        let t = CloudTier::build(Prize::Cloud500, &raw);
        assert_eq!(t.count, 5_000);
        assert!(t.contains(11));
        assert!(t.contains(raw[4999]));
        assert!(!t.contains(12));
        assert!(!t.contains(0));
    }

    #[test]
    fn every_element_found_and_gaps_not() {
        let raw = sample();
        let t = CloudTier::build(Prize::Cloud500, &raw);
        for &v in &raw {
            assert!(t.contains(v), "遺漏 {v}");
        }
        // 每個元素前一個值（非序列成員）都不該命中
        for &v in raw.iter().take(500) {
            if v > 0 && !raw.contains(&(v - 1)) {
                assert!(!t.contains(v - 1));
            }
        }
    }

    #[test]
    fn encode_decode_roundtrip() {
        let raw = sample();
        let t = CloudTier::build(Prize::Cloud500, &raw);
        let mut buf = Vec::new();
        t.encode(&mut buf);
        let (back, used) = CloudTier::decode(&buf).unwrap();
        assert_eq!(used, buf.len());
        assert_eq!(back, t);
    }

    #[test]
    fn compression_is_reasonable() {
        // 模擬真實分布：兩百萬組號碼，平均間距 ~50
        let raw: Vec<u64> = (0..2_000_000u64).map(|i| i * 50 + 1).collect();
        let t = CloudTier::build(Prize::Cloud500, &raw);
        assert!(t.contains(1));
        assert!(t.contains(2_000_000 * 50 - 49));
        assert!(!t.contains(2));
        let mb = t.size_bytes() as f64 / 1_048_576.0;
        assert!(mb < 12.0, "索引過大：{mb:.1} MB");
    }

    #[test]
    fn multi_tier_best_prize() {
        let mut idx = CloudIndex::default();
        idx.add(CloudTier::build(Prize::Cloud500, &[100, 200]));
        idx.add(CloudTier::build(Prize::CloudMillion, &[200, 300]));
        assert_eq!(idx.lookup(200).unwrap().0, Prize::CloudMillion);
        assert_eq!(idx.lookup(100).unwrap().0, Prize::Cloud500);
        assert!(idx.lookup(999).is_none());
        assert_eq!(idx.total(), 4);

        let back = CloudIndex::decode(&idx.encode()).unwrap();
        assert_eq!(back.total(), 4);
        assert_eq!(back.lookup(200).unwrap().0, Prize::CloudMillion);
    }

    #[test]
    fn rejects_corrupt() {
        assert!(CloudTier::decode(b"XXXX\x01\x01").is_err());
        let t = CloudTier::build(Prize::Cloud500, &[1, 2, 3]);
        let mut buf = Vec::new();
        t.encode(&mut buf);
        buf.truncate(buf.len() - 1);
        assert!(CloudTier::decode(&buf).is_err());
    }
}
