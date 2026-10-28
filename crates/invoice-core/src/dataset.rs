//! 資料集：多期一般獎別號碼的集合，可序列化為 JSON 放進版本庫。

use crate::check::{check_draw, Draw, Hit};
use crate::cloud_index::CloudIndex;
use crate::code::InvoiceCode;
use crate::json::{self, Json};
use crate::period::Period;
use crate::rules::Prize;

/// 多期資料集。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dataset {
    /// 產生時間（UTC ISO-8601）。
    pub generated_at: String,
    /// 資料來源說明（官方頁面或 PDF 網址）。
    pub source: String,
    /// 各期一般獎別，依期別新到舊排序。
    pub periods: Vec<Draw>,
}

/// 雲端發票專屬獎的查詢結果（含「不適用」狀態）。
#[derive(Debug, Clone, PartialEq)]
pub enum CloudOutcome {
    /// 未載入該期雲端索引，無法判斷。
    NotLoaded,
    /// 已查詢，未中獎。
    Miss,
    /// 中獎。
    Hit(Prize),
}

impl Dataset {
    pub fn latest(&self) -> Option<&Draw> {
        self.periods.first()
    }

    pub fn by_period(&self, p: Period) -> Option<&Draw> {
        self.periods.iter().find(|d| d.period == p)
    }

    /// 對單一發票號碼對獎（一般獎別 + 可選雲端專屬獎）。
    ///
    /// 回傳所有命中，已依獎金排序；同時對中多獎時取第一筆（法規：擇一領取）。
    pub fn check(
        &self,
        code: InvoiceCode,
        period: Period,
        cloud: Option<&CloudIndex>,
    ) -> Result<(Vec<Hit>, CloudOutcome), String> {
        let draw = self
            .by_period(period)
            .ok_or_else(|| format!("資料集中沒有 {} 期的中獎號碼", period.label()))?;

        let mut hits = check_draw(draw, code);

        let cloud_outcome = match cloud {
            None => CloudOutcome::NotLoaded,
            Some(idx) => match idx.lookup(code.to_raw()) {
                Some((prize, _)) => {
                    hits.push(Hit {
                        prize,
                        matched: code.to_string(),
                        detail: format!("字軌 {} 與雲端發票專屬獎名單相符", code),
                    });
                    CloudOutcome::Hit(prize)
                }
                None => CloudOutcome::Miss,
            },
        };

        hits.sort_by_key(|h| std::cmp::Reverse(h.amount()));
        Ok((hits, cloud_outcome))
    }

    /// 序列化為 JSON。
    pub fn to_json(&self) -> Json {
        let periods = self
            .periods
            .iter()
            .map(|d| {
                json::obj(vec![
                    ("period", json::s(d.period.code())),
                    ("label", json::s(d.period.label())),
                    ("draw_date", json::s(fmt_date(d.period.draw_date()))),
                    ("special", json::s(&d.special)),
                    ("grand", json::s(&d.grand)),
                    ("first", Json::Arr(d.first.iter().map(json::s).collect())),
                    (
                        "extra_sixth",
                        Json::Arr(d.extra_sixth.iter().map(json::s).collect()),
                    ),
                ])
            })
            .collect();

        json::obj(vec![
            ("schema", json::n(1.0)),
            ("generated_at", json::s(&self.generated_at)),
            ("source", json::s(&self.source)),
            ("periods", Json::Arr(periods)),
        ])
    }

    /// 由 JSON 還原。
    pub fn from_json(v: &Json) -> Result<Self, String> {
        let arr = v
            .get("periods")
            .and_then(|x| x.as_arr())
            .ok_or("缺少 periods 欄位")?;

        let mut periods = Vec::new();
        for p in arr {
            let code = p.str_at("period").ok_or("期別缺少 period 欄位")?;
            let period = Period::from_code(code).ok_or_else(|| format!("無效期別：{code}"))?;
            let d = Draw::new(
                period,
                p.str_at("special").unwrap_or_default().to_string(),
                p.str_at("grand").unwrap_or_default().to_string(),
                p.str_arr_at("first"),
                p.str_arr_at("extra_sixth"),
            );
            d.validate()?;
            periods.push(d);
        }
        periods.sort_by_key(|d| std::cmp::Reverse(d.period));

        Ok(Self {
            generated_at: v.str_at("generated_at").unwrap_or_default().to_string(),
            source: v.str_at("source").unwrap_or_default().to_string(),
            periods,
        })
    }

    /// 把某期加進資料集（同期覆蓋）。
    pub fn upsert(&mut self, draw: Draw) {
        self.periods.retain(|d| d.period != draw.period);
        self.periods.push(draw);
        self.periods.sort_by_key(|d| std::cmp::Reverse(d.period));
    }
}

fn fmt_date(d: crate::civil::Ymd) -> String {
    format!("{:04}-{:02}-{:02}", d.y, d.m, d.d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ds() -> Dataset {
        let mut d = Dataset::default();
        d.upsert(Draw::new(
            Period::new(115, 4).unwrap(),
            "89996565".into(),
            "91098182".into(),
            vec!["54348835".into(), "44991397".into(), "06595111".into()],
            vec![],
        ));
        d.upsert(Draw::new(
            Period::new(115, 3).unwrap(),
            "38548029".into(),
            "12913459".into(),
            vec!["24121937".into(), "28589937".into(), "83663337".into()],
            vec![],
        ));
        d
    }

    #[test]
    fn sorted_newest_first() {
        let d = ds();
        assert_eq!(d.latest().unwrap().period, Period::new(115, 4).unwrap());
        assert_eq!(d.periods.len(), 2);
    }

    #[test]
    fn json_roundtrip() {
        let d = ds();
        let text = d.to_json().to_pretty();
        let back = Dataset::from_json(&json::parse(&text).unwrap()).unwrap();
        assert_eq!(back.periods.len(), 2);
        assert_eq!(back.latest().unwrap().special, "89996565");
    }

    #[test]
    fn upsert_replaces_same_period() {
        let mut d = ds();
        d.upsert(Draw::new(
            Period::new(115, 4).unwrap(),
            "11111111".into(),
            "22222222".into(),
            vec!["33333333".into(), "44444444".into(), "55555555".into()],
            vec![],
        ));
        assert_eq!(d.periods.len(), 2);
        assert_eq!(
            d.by_period(Period::new(115, 4).unwrap()).unwrap().special,
            "11111111"
        );
    }

    #[test]
    fn check_with_and_without_cloud() {
        let d = ds();
        let p = Period::new(115, 4).unwrap();
        let c = InvoiceCode::parse("AA54348835").unwrap();
        let (hits, cloud) = d.check(c, p, None).unwrap();
        assert_eq!(hits[0].prize, Prize::First);
        assert_eq!(cloud, CloudOutcome::NotLoaded);

        let mut idx = CloudIndex::default();
        idx.add(crate::cloud_index::CloudTier::build(
            Prize::CloudMillion,
            &[c.to_raw()],
        ));
        let (hits, cloud) = d.check(c, p, Some(&idx)).unwrap();
        assert_eq!(hits[0].prize, Prize::CloudMillion);
        assert_eq!(cloud, CloudOutcome::Hit(Prize::CloudMillion));
    }

    #[test]
    fn missing_period_errors() {
        let d = ds();
        let c = InvoiceCode::parse("AA00000000").unwrap();
        assert!(d.check(c, Period::new(113, 1).unwrap(), None).is_err());
    }
}
