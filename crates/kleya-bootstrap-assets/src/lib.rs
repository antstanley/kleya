//! Embedded bootstrap script + ghostty terminfo source.

#![allow(missing_docs)]

pub const SETUP_TEMPLATE: &str = include_str!("../assets/setup_devbox.sh.j2");

pub const GHOSTTY_TERMINFO: &str = include_str!("../assets/ghostty.terminfo");

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assets_are_non_empty() {
        assert!(!SETUP_TEMPLATE.is_empty());
        assert!(!GHOSTTY_TERMINFO.is_empty());
    }
}
