//! Port type system (spec §4.9).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortType {
    String,
    Number,
    Json,
    Document,
    DocumentList,
    NoteRef,
    NoteRefList,
    Card,
    Any,
}

impl PortType {
    pub const ALL: [PortType; 9] = [
        PortType::String,
        PortType::Number,
        PortType::Json,
        PortType::Document,
        PortType::DocumentList,
        PortType::NoteRef,
        PortType::NoteRefList,
        PortType::Card,
        PortType::Any,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PortType::String => "string",
            PortType::Number => "number",
            PortType::Json => "json",
            PortType::Document => "document",
            PortType::DocumentList => "document[]",
            PortType::NoteRef => "note_ref",
            PortType::NoteRefList => "note_ref[]",
            PortType::Card => "card",
            PortType::Any => "any",
        }
    }

    pub fn parse(s: &str) -> Option<PortType> {
        let norm = s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        Some(match norm.as_str() {
            "string" | "str" | "text" => PortType::String,
            "number" | "num" | "float" | "int" => PortType::Number,
            "json" | "object" => PortType::Json,
            "document" | "doc" => PortType::Document,
            "document[]" | "documents" | "doc[]" | "documentlist" => PortType::DocumentList,
            "note_ref" | "noteref" | "note" => PortType::NoteRef,
            "note_ref[]" | "noteref[]" | "note_refs" | "noterefs" => PortType::NoteRefList,
            "card" => PortType::Card,
            "any" | "*" => PortType::Any,
            _ => return None,
        })
    }

    /// Can an output of type `self` feed an input of type `to`?
    /// - `any` connects to everything
    /// - `document` → `document[]` is allowed (auto-wrapped at runtime)
    /// - everything else requires an exact match
    pub fn connects_to(self, to: PortType) -> bool {
        self == PortType::Any
            || to == PortType::Any
            || self == to
            || (self == PortType::Document && to == PortType::DocumentList)
    }

    /// Runtime coercion matching `connects_to` (Document → DocumentList wraps).
    pub fn coerce(self, to: PortType, v: serde_json::Value) -> serde_json::Value {
        match (self, to, v) {
            (PortType::Document, PortType::DocumentList, v @ serde_json::Value::Object(_)) => serde_json::Value::Array(vec![v]),
            (_, _, v) => v,
        }
    }
}

impl fmt::Display for PortType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for PortType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PortType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        PortType::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown port type `{s}`")))
    }
}

#[cfg(test)]
mod tests {
    use super::PortType::*;
    use super::*;

    #[test]
    fn compatibility_rules() {
        for t in PortType::ALL {
            assert!(Any.connects_to(t) && t.connects_to(Any), "any ↔ {t}");
            assert!(t.connects_to(t));
        }
        assert!(Document.connects_to(DocumentList));
        assert!(!DocumentList.connects_to(Document));
        assert!(!String.connects_to(Number));
        assert!(!NoteRef.connects_to(NoteRefList), "only document auto-wraps");
    }

    #[test]
    fn parse_roundtrip() {
        for t in PortType::ALL {
            assert_eq!(PortType::parse(t.as_str()), Some(t));
        }
        assert_eq!(PortType::parse("note-ref[]"), Some(NoteRefList));
        assert_eq!(PortType::parse("banana"), None);
    }

    #[test]
    fn coerce_wraps_document() {
        let d = serde_json::json!({"path": "a.md"});
        assert_eq!(Document.coerce(DocumentList, d.clone()), serde_json::json!([d]));
    }
}
