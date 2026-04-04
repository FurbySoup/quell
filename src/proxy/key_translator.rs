// Key translation layer for the input path.
//
// Scans input bytes for Kitty keyboard protocol sequences and translates
// them before forwarding to ConPTY. Handles partial sequences at buffer
// boundaries via internal buffering.
//
// Currently translates:
// - Shift+Enter (CSI 13;2 u) → tool-specific newline sequence

use tracing::{debug, trace};

use crate::config::ToolKind;

/// Kitty protocol enable sequence: CSI > 1 u
/// Requests progressive enhancement level 1 (disambiguate escape codes).
pub const KITTY_ENABLE: &[u8] = b"\x1b[>1u";

/// Kitty protocol disable sequence: CSI < u
/// Restores original keyboard mode.
pub const KITTY_DISABLE: &[u8] = b"\x1b[<u";

/// Comprehensive input mode reset sent at proxy shutdown.
///
/// Covers every input mode the child process may have enabled via ConPTY.
/// ConPTY is known to swallow some disable sequences (e.g. `?9001l`) when the
/// child exits, leaving the parent terminal in a mode where keypresses are
/// encoded as structured sequences (e.g. `;28;13;1;0;1_` for Enter in Win32
/// input mode) that the parent shell cannot interpret.
///
/// Safe to emit unconditionally — terminals silently ignore disables for modes
/// that are not currently active.
pub const TERMINAL_INPUT_RESET: &[u8] = b"\x1b[<u\x1b[?9001l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?2004l";

/// Translates Kitty keyboard protocol sequences to tool-specific byte sequences.
pub struct KeyTranslator {
    tool: ToolKind,
    /// Partial escape sequence buffer for chunk boundary handling
    pending: Vec<u8>,
    /// Metrics
    translations: u64,
}

impl KeyTranslator {
    pub fn new(tool: ToolKind) -> Self {
        debug!(tool = %tool, "key translator initialized");
        Self {
            tool,
            pending: Vec::with_capacity(16),
            translations: 0,
        }
    }

    /// Translate input bytes. Returns the translated byte sequence.
    /// Handles Kitty CSI u sequences split across chunk boundaries.
    pub fn translate(&mut self, data: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(data.len() + self.pending.len());
        let mut i = 0;

        // Prepend any pending bytes from previous chunk
        let combined;
        let input = if !self.pending.is_empty() {
            combined = [self.pending.as_slice(), data].concat();
            self.pending.clear();
            &combined
        } else {
            data
        };

        while i < input.len() {
            if input[i] == 0x1B {
                // Start of escape sequence — try to match Kitty CSI u
                match self.try_match_kitty_csi(&input[i..]) {
                    MatchResult::Translated(replacement, consumed) => {
                        output.extend_from_slice(replacement);
                        i += consumed;
                        self.translations += 1;
                        debug!(
                            tool = %self.tool,
                            total = self.translations,
                            "translated Shift+Enter"
                        );
                    }
                    MatchResult::NotKitty(consumed) => {
                        // Pass through the escape sequence as-is
                        output.extend_from_slice(&input[i..i + consumed]);
                        i += consumed;
                    }
                    MatchResult::Incomplete => {
                        // Buffer remaining bytes for next chunk
                        self.pending.extend_from_slice(&input[i..]);
                        trace!(pending_bytes = self.pending.len(), "buffering partial escape");
                        return output;
                    }
                }
            } else {
                output.push(input[i]);
                i += 1;
            }
        }

        output
    }

    /// Try to match a Kitty CSI u sequence starting at the given position.
    /// Input must start with ESC (0x1B).
    fn try_match_kitty_csi<'a>(&self, input: &'a [u8]) -> MatchResult<'a> {
        // Minimum Kitty CSI u: ESC [ <digit> u = 4 bytes
        // Shift+Enter:          ESC [ 1 3 ; 2 u = 7 bytes
        if input.len() < 2 {
            return MatchResult::Incomplete;
        }

        if input[1] != b'[' {
            // Not CSI — pass through ESC + next byte
            return MatchResult::NotKitty(2);
        }

        // We have ESC [ — scan for the final byte
        let mut pos = 2;
        loop {
            if pos >= input.len() {
                return MatchResult::Incomplete;
            }

            let b = input[pos];
            if b == b'u' {
                // Found the final byte — parse the CSI parameters
                let params = &input[2..pos];
                pos += 1; // consume the 'u'

                if let Some(replacement) = self.translate_csi_u(params) {
                    return MatchResult::Translated(replacement, pos);
                } else {
                    // Valid CSI u but not one we translate — pass through
                    return MatchResult::NotKitty(pos);
                }
            } else if (0x40..=0x7E).contains(&b) {
                // Some other CSI final byte — not a Kitty 'u' sequence
                return MatchResult::NotKitty(pos + 1);
            } else if b.is_ascii_digit() || b == b';' || b == b':' || b == b'?' || b == b'>' || b == b'<' {
                // Parameter or intermediate byte — keep scanning
                pos += 1;
            } else {
                // Unexpected byte — not a valid CSI
                return MatchResult::NotKitty(pos + 1);
            }
        }
    }

    /// Translate a CSI u parameter sequence.
    /// `params` is the bytes between `[` and `u` (e.g., "13;2" for Shift+Enter).
    fn translate_csi_u(&self, params: &[u8]) -> Option<&'static [u8]> {
        // Parse "codepoint;modifiers" format
        let params_str = std::str::from_utf8(params).ok()?;
        let mut parts = params_str.split(';');

        let codepoint: u32 = parts.next()?.parse().ok()?;
        let modifiers: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);

        // Kitty modifier encoding: value = 1 + modifier_bits
        // Shift = bit 0, so Shift+key has modifiers = 2 (1 + 1)
        match (codepoint, modifiers) {
            // Shift+Enter: codepoint 13 (CR), modifier 2 (shift)
            (13, 2) => Some(self.tool.shift_enter_bytes()),
            _ => None,
        }
    }

    #[allow(dead_code)] // Phase 2 — used for status bar metrics
    pub fn translations(&self) -> u64 {
        self.translations
    }
}

enum MatchResult<'a> {
    /// Successfully translated — replacement bytes and number of input bytes consumed
    Translated(&'a [u8], usize),
    /// Not a Kitty sequence we translate — number of bytes to pass through
    NotKitty(usize),
    /// Incomplete sequence — need more data
    Incomplete,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        assert_eq!(t.translate(b"hello world"), b"hello world");
    }

    #[test]
    fn test_shift_enter_claude() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // CSI 13;2 u = Shift+Enter in Kitty protocol
        let result = t.translate(b"\x1b[13;2u");
        assert_eq!(result, b"\x1b\x0d"); // ESC + CR
        assert_eq!(t.translations(), 1);
    }

    #[test]
    fn test_shift_enter_gemini() {
        let mut t = KeyTranslator::new(ToolKind::Gemini);
        let result = t.translate(b"\x1b[13;2u");
        assert_eq!(result, b"\x0a"); // Ctrl+J
        assert_eq!(t.translations(), 1);
    }

    #[test]
    fn test_shift_enter_copilot() {
        let mut t = KeyTranslator::new(ToolKind::Copilot);
        let result = t.translate(b"\x1b[13;2u");
        assert_eq!(result, b"\x0a"); // literal newline
    }

    #[test]
    fn test_shift_enter_with_surrounding_text() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        let result = t.translate(b"before\x1b[13;2uafter");
        assert_eq!(result, b"before\x1b\x0dafter");
        assert_eq!(t.translations(), 1);
    }

    #[test]
    fn test_plain_enter_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // Plain Enter in Kitty: CSI 13 u (no modifier)
        let result = t.translate(b"\x1b[13u");
        // Should pass through unchanged — no translation for unmodified Enter
        assert_eq!(result, b"\x1b[13u");
        assert_eq!(t.translations(), 0);
    }

    #[test]
    fn test_other_csi_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // CSI 31 m (SGR red) — not a Kitty u sequence
        assert_eq!(t.translate(b"\x1b[31m"), b"\x1b[31m");
        // CSI 5 A (cursor up 5)
        assert_eq!(t.translate(b"\x1b[5A"), b"\x1b[5A");
        assert_eq!(t.translations(), 0);
    }

    #[test]
    fn test_other_esc_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // ESC O P (F1 in some terminals)
        assert_eq!(t.translate(b"\x1bOP"), b"\x1bOP");
    }

    #[test]
    fn test_ctrl_c_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        assert_eq!(t.translate(b"\x03"), b"\x03");
    }

    #[test]
    fn test_chunk_boundary_complete() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // Shift+Enter split: ESC [ 13 at end of chunk 1
        let result1 = t.translate(b"\x1b[13");
        assert_eq!(result1, b""); // buffered
        // ;2u at start of chunk 2
        let result2 = t.translate(b";2u");
        assert_eq!(result2, b"\x1b\x0d"); // translated
        assert_eq!(t.translations(), 1);
    }

    #[test]
    fn test_chunk_boundary_esc_only() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // Just ESC at chunk boundary
        let result1 = t.translate(b"text\x1b");
        assert_eq!(result1, b"text"); // ESC buffered
        let result2 = t.translate(b"[13;2u");
        assert_eq!(result2, b"\x1b\x0d"); // translated
    }

    #[test]
    fn test_chunk_boundary_non_kitty() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // ESC at chunk boundary followed by non-CSI
        let result1 = t.translate(b"\x1b");
        assert_eq!(result1, b""); // buffered
        let result2 = t.translate(b"Omore");
        // ESC O passed through, then "more"
        assert_eq!(result2, b"\x1bOmore");
    }

    #[test]
    fn test_multiple_translations() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        let result = t.translate(b"\x1b[13;2u\x1b[13;2u");
        assert_eq!(result, b"\x1b\x0d\x1b\x0d");
        assert_eq!(t.translations(), 2);
    }

    #[test]
    fn test_other_kitty_modifier_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // Ctrl+Enter: CSI 13;5 u (ctrl = modifier 5)
        let result = t.translate(b"\x1b[13;5u");
        // Not translated — pass through
        assert_eq!(result, b"\x1b[13;5u");
        assert_eq!(t.translations(), 0);
    }

    // Standard shortcut passthrough tests — these must arrive at the child
    // process byte-identical to what the user typed.

    #[test]
    fn test_all_ctrl_shortcuts_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // All standard Ctrl shortcuts used by AI CLI tools
        assert_eq!(t.translate(b"\x03"), b"\x03"); // Ctrl+C
        assert_eq!(t.translate(b"\x04"), b"\x04"); // Ctrl+D
        assert_eq!(t.translate(b"\x0C"), b"\x0C"); // Ctrl+L
        assert_eq!(t.translate(b"\x12"), b"\x12"); // Ctrl+R
        assert_eq!(t.translate(b"\x01"), b"\x01"); // Ctrl+A
        assert_eq!(t.translate(b"\x05"), b"\x05"); // Ctrl+E
        assert_eq!(t.translate(b"\x17"), b"\x17"); // Ctrl+W
        assert_eq!(t.translate(b"\x15"), b"\x15"); // Ctrl+U
        assert_eq!(t.translate(b"\x1A"), b"\x1A"); // Ctrl+Z
        assert_eq!(t.translations(), 0);
    }

    #[test]
    fn test_tab_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        assert_eq!(t.translate(b"\x09"), b"\x09"); // Tab
    }

    #[test]
    fn test_escape_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // Lone ESC followed by non-sequence data in next chunk
        let result1 = t.translate(b"\x1b");
        assert!(result1.is_empty()); // buffered
        // Next chunk starts with a non-[ byte — ESC is passed through
        let result2 = t.translate(b" ");
        assert_eq!(result2, b"\x1b ");
    }

    #[test]
    fn test_enter_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        assert_eq!(t.translate(b"\x0d"), b"\x0d"); // CR (Enter)
    }

    #[test]
    fn test_arrow_keys_passthrough() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        assert_eq!(t.translate(b"\x1b[A"), b"\x1b[A"); // Up
        assert_eq!(t.translate(b"\x1b[B"), b"\x1b[B"); // Down
        assert_eq!(t.translate(b"\x1b[C"), b"\x1b[C"); // Right
        assert_eq!(t.translate(b"\x1b[D"), b"\x1b[D"); // Left
    }

    #[test]
    fn test_mixed_shortcuts_and_shift_enter() {
        let mut t = KeyTranslator::new(ToolKind::Claude);
        // Ctrl+A, then Shift+Enter, then Ctrl+E
        let result = t.translate(b"\x01\x1b[13;2u\x05");
        assert_eq!(result, b"\x01\x1b\x0d\x05");
        assert_eq!(t.translations(), 1);
    }
}
