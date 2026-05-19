//! Versioned wrapper for `--json` list outputs.
//!
//! Agent consumers parse `version` first and branch on it; raw `Vec<_>`
//! outputs are reserved for follow-up tooling that opts out.

/// Versioned wrapper for `--json` list outputs. Agent consumers parse
/// `version` first and branch on it; raw `Vec<_>` outputs are reserved
/// for follow-up tooling that opts out.
#[derive(Debug, serde::Serialize)]
pub struct JsonList<'a, T: serde::Serialize> {
    /// Envelope schema version. Currently always `"1"`; bumped on a
    /// breaking shape change.
    pub version: &'a str,
    /// The actual payload — a borrowed slice of items to serialize.
    pub items: &'a [T],
}

impl<'a, T: serde::Serialize> JsonList<'a, T> {
    /// Current envelope schema version emitted by [`JsonList::new`].
    pub const VERSION: &'static str = "1";

    /// Wrap a slice of items in the current versioned envelope.
    pub fn new(items: &'a [T]) -> Self {
        Self {
            version: Self::VERSION,
            items,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_list_envelope_shape() {
        let items = vec!["a", "b"];
        let env = JsonList::new(&items);
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"version\":\"1\""), "got: {s}");
        assert!(s.contains("\"items\":[\"a\",\"b\"]"), "got: {s}");
    }

    #[test]
    fn json_list_envelope_empty_items() {
        let items: Vec<&str> = vec![];
        let env = JsonList::new(&items);
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"version\":\"1\""), "got: {s}");
        assert!(s.contains("\"items\":[]"), "got: {s}");
    }
}
