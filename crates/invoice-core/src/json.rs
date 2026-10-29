//! 極簡 JSON 解析／序列化（零依賴）。
//!
//! 只用來處理本專案自己的資料檔（中獎號碼清單、更新清單、API 回應），
//! 因此只實作 RFC 8259 的完整子集，足以處理字串轉義、巢狀物件／陣列、
//! 數字、布林與 null。刻意不引入 serde，讓最終執行檔保持輕量。

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// 取字串欄位。
    pub fn str_at(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    /// 取字串陣列欄位。
    pub fn str_arr_at(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(|v| v.as_arr())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 序列化（緊湊輸出）。等同 `to_string()`。
    pub fn to_compact(&self) -> String {
        let mut s = String::new();
        self.write(&mut s, None, 0);
        s
    }

    /// 序列化（縮排輸出，適合放進 git 做 diff）。
    pub fn to_pretty(&self) -> String {
        let mut s = String::new();
        self.write(&mut s, Some(2), 0);
        s
    }

    fn write(&self, out: &mut String, indent: Option<usize>, depth: usize) {
        let pad = |depth: usize| -> String {
            match indent {
                Some(n) => " ".repeat(n * depth),
                None => String::new(),
            }
        };
        let nl = |indent: Option<usize>| if indent.is_some() { "\n" } else { "" };
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 9e15 {
                    let _ = write!(out, "{}", *n as i64);
                } else {
                    let _ = write!(out, "{n}");
                }
            }
            Json::Str(s) => write_str(out, s),
            Json::Arr(a) => {
                if a.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                out.push_str(nl(indent));
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                        out.push_str(nl(indent));
                    }
                    out.push_str(&pad(depth + 1));
                    v.write(out, indent, depth + 1);
                }
                out.push_str(nl(indent));
                out.push_str(&pad(depth));
                out.push(']');
            }
            Json::Obj(m) => {
                if m.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                out.push_str(nl(indent));
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                        out.push_str(nl(indent));
                    }
                    out.push_str(&pad(depth + 1));
                    write_str(out, k);
                    out.push(':');
                    if indent.is_some() {
                        out.push(' ');
                    }
                    v.write(out, indent, depth + 1);
                }
                out.push_str(nl(indent));
                out.push_str(&pad(depth));
                out.push('}');
            }
        }
    }
}

impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        self.write(&mut s, None, 0);
        f.write_str(&s)
    }
}

fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// 解析錯誤。
#[derive(Debug, Clone, PartialEq)]
pub struct JsonError {
    pub msg: String,
    pub pos: usize,
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON 解析失敗（位移 {}）：{}", self.pos, self.msg)
    }
}

impl std::error::Error for JsonError {}

struct P<'a> {
    b: &'a [u8],
    i: usize,
    depth: usize,
}

impl<'a> P<'a> {
    fn err<T>(&self, msg: &str) -> Result<T, JsonError> {
        Err(JsonError {
            msg: msg.to_string(),
            pos: self.i,
        })
    }

    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn value(&mut self) -> Result<Json, JsonError> {
        self.ws();
        if self.depth >= 64 {
            return self.err("JSON 巢狀深度超過上限");
        }
        self.depth += 1;
        let result = match self.peek() {
            None => self.err("檔案意外結束"),
            Some(b'{') => self.obj(),
            Some(b'[') => self.arr(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'n') => self.lit("null", Json::Null),
            Some(_) => self.num(),
        };
        self.depth -= 1;
        result
    }

    fn lit(&mut self, word: &str, v: Json) -> Result<Json, JsonError> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            self.err("無效的字面值")
        }
    }

    fn num(&mut self) -> Result<Json, JsonError> {
        let start = self.i;
        while self.i < self.b.len()
            && matches!(
                self.b[self.i],
                b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
            )
        {
            self.i += 1;
        }
        std::str::from_utf8(&self.b[start..self.i])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .map(Json::Num)
            .ok_or(JsonError {
                msg: "無效的數字".into(),
                pos: start,
            })
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.i += 1; // 略過開頭引號
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                return self.err("字串未閉合");
            };
            match c {
                b'"' => {
                    self.i += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.i += 1;
                    let Some(e) = self.peek() else {
                        return self.err("轉義未完成");
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            if self.i + 4 > self.b.len() {
                                return self.err("\\u 轉義長度不足");
                            }
                            let hex =
                                std::str::from_utf8(&self.b[self.i..self.i + 4]).unwrap_or("");
                            let cp = u32::from_str_radix(hex, 16).map_err(|_| JsonError {
                                msg: "無效的 \\u 轉義".into(),
                                pos: self.i,
                            })?;
                            self.i += 4;
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        }
                        _ => return self.err("未知的轉義字元"),
                    }
                }
                _ => {
                    // 直接複製一個 UTF-8 序列
                    let start = self.i;
                    let len = utf8_len(c);
                    self.i = (self.i + len).min(self.b.len());
                    out.push_str(std::str::from_utf8(&self.b[start..self.i]).unwrap_or(""));
                }
            }
        }
    }

    fn arr(&mut self) -> Result<Json, JsonError> {
        self.i += 1;
        let mut v = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Arr(v));
        }
        loop {
            v.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(v));
                }
                _ => return self.err("陣列缺少逗號或結尾"),
            }
        }
    }

    fn obj(&mut self) -> Result<Json, JsonError> {
        self.i += 1;
        let mut m = BTreeMap::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Obj(m));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return self.err("物件鍵必須是字串");
            }
            let k = self.string()?;
            self.ws();
            if self.peek() != Some(b':') {
                return self.err("物件缺少冒號");
            }
            self.i += 1;
            let v = self.value()?;
            m.insert(k, v);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(m));
                }
                _ => return self.err("物件缺少逗號或結尾"),
            }
        }
    }
}

fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// 解析 JSON 文字。
pub fn parse(s: &str) -> Result<Json, JsonError> {
    let mut p = P {
        b: s.as_bytes(),
        i: 0,
        depth: 0,
    };
    let v = p.value()?;
    p.ws();
    if p.i != p.b.len() {
        return Err(JsonError {
            msg: "尾端有多餘內容".into(),
            pos: p.i,
        });
    }
    Ok(v)
}

/// 便利建構子。
pub fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

pub fn s(v: impl Into<String>) -> Json {
    Json::Str(v.into())
}

pub fn n(v: impl Into<f64>) -> Json {
    Json::Num(v.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_excessive_nesting() {
        let input = format!("{}0{}", "[".repeat(65), "]".repeat(65));
        assert!(parse(&input).is_err());
        assert!(parse("[[0]]").is_ok());
    }

    #[test]
    fn roundtrip_nested() {
        let src = r#"{"a":[1,2,{"b":"中獎"}],"c":true,"d":null,"e":-3.5}"#;
        let v = parse(src).unwrap();
        assert_eq!(v.str_arr_at("a"), Vec::<String>::new());
        assert_eq!(v.get("c").unwrap().as_bool(), Some(true));
        assert_eq!(v.get("e").unwrap().as_f64(), Some(-3.5));
        let re = parse(&v.to_string()).unwrap();
        assert_eq!(v, re);
    }

    #[test]
    fn escapes() {
        let v = parse(r#""a\"b\\c\n中\u6587""#).unwrap();
        assert_eq!(v.as_str().unwrap(), "a\"b\\c\n中文");
        let out = Json::Str("x\"y\nz".into()).to_string();
        assert_eq!(parse(&out).unwrap().as_str().unwrap(), "x\"y\nz");
    }

    #[test]
    fn rejects_trailing() {
        assert!(parse("{} x").is_err());
        assert!(parse("{").is_err());
    }

    #[test]
    fn pretty_is_stable() {
        let v = parse(r#"{"z":1,"a":[1,2]}"#).unwrap();
        let p = v.to_pretty();
        assert!(p.contains('\n'));
        assert_eq!(parse(&p).unwrap(), v);
    }
}
