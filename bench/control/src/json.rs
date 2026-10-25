//! Minimal JSON writer (std-only). Values are built in order and rendered
//! compactly; `Raw` embeds an already-serialized JSON value verbatim.

use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub enum J {
    Null,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
    Raw(String),
}

impl J {
    pub fn obj() -> Self {
        J::Obj(Vec::new())
    }

    /// Builder: append a field to an object.
    pub fn with(mut self, key: &str, value: impl Into<J>) -> Self {
        self.set(key, value);
        self
    }

    pub fn set(&mut self, key: &str, value: impl Into<J>) {
        if let J::Obj(fields) = self {
            let value = value.into();
            if let Some(slot) = fields.iter_mut().find(|(k, _)| k == key) {
                slot.1 = value;
            } else {
                fields.push((key.to_string(), value));
            }
        }
    }

    pub fn push(&mut self, value: impl Into<J>) {
        if let J::Arr(items) = self {
            items.push(value.into());
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            J::Null => out.push_str("null"),
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Int(i) => {
                let _ = write!(out, "{i}");
            }
            J::Num(n) if n.is_finite() => {
                let _ = write!(out, "{n}");
            }
            J::Num(_) => out.push_str("null"),
            J::Str(s) => escape(s, out),
            J::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            J::Obj(fields) => {
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    escape(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
            J::Raw(r) => out.push_str(r.trim()),
        }
    }
}

fn escape(s: &str, out: &mut String) {
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

impl From<bool> for J {
    fn from(v: bool) -> Self {
        J::Bool(v)
    }
}
impl From<i64> for J {
    fn from(v: i64) -> Self {
        J::Int(v)
    }
}
impl From<u64> for J {
    fn from(v: u64) -> Self {
        J::Int(v as i64)
    }
}
impl From<u32> for J {
    fn from(v: u32) -> Self {
        J::Int(v as i64)
    }
}
impl From<usize> for J {
    fn from(v: usize) -> Self {
        J::Int(v as i64)
    }
}
impl From<i32> for J {
    fn from(v: i32) -> Self {
        J::Int(v as i64)
    }
}
impl From<f64> for J {
    fn from(v: f64) -> Self {
        J::Num(v)
    }
}
impl From<&str> for J {
    fn from(v: &str) -> Self {
        J::Str(v.to_string())
    }
}
impl From<String> for J {
    fn from(v: String) -> Self {
        J::Str(v)
    }
}
impl From<&String> for J {
    fn from(v: &String) -> Self {
        J::Str(v.clone())
    }
}
impl<T: Into<J>> From<Option<T>> for J {
    fn from(v: Option<T>) -> Self {
        v.map_or(J::Null, Into::into)
    }
}
impl From<Vec<J>> for J {
    fn from(v: Vec<J>) -> Self {
        J::Arr(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_nested_values_and_escapes() {
        let j = J::obj()
            .with("a", 1i64)
            .with("s", "q\"\n\u{1}")
            .with("n", J::Null)
            .with("l", vec![J::Bool(true), J::Num(1.5)])
            .with("r", J::Raw(" {\"x\":1}\n".into()));
        assert_eq!(
            j.render(),
            r#"{"a":1,"s":"q\"\n\u0001","n":null,"l":[true,1.5],"r":{"x":1}}"#
        );
    }

    #[test]
    fn set_replaces_existing_key() {
        let mut j = J::obj().with("a", 1i64);
        j.set("a", 2i64);
        assert_eq!(j.render(), r#"{"a":2}"#);
    }
}
