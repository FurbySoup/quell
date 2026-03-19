// Live output filter — sits between ConPTY output and stdout.
//
// Layer 1 (byte-level): strips known-dangerous sequences inline.
// Stateful to handle sequences split across chunk boundaries.
//
// Layer 2 (termwiz parse-level) is in escape_filter.rs for history replay only.

use tracing::{debug, info, warn};

/// Byte-level output filter for the live output path.
///
/// Strips dangerous escape sequences before they reach the user's terminal.
/// Handles sequences that span chunk boundaries via internal state machine.
pub struct OutputFilter {
    state: FilterState,
    /// Output buffer for the current filter() call
    output: Vec<u8>,
    /// Total bytes processed — used to skip ESC[2J stripping during
    /// initial display setup (first ~64KB of output)
    total_bytes_processed: u64,
    /// Metrics
    osc52_stripped: u64,
    osc50_stripped: u64,
    c1_bytes_stripped: u64,
    clear_screen_stripped: u64,
    queries_stripped: u64,
    titles_sanitized: u64,
    links_stripped: u64,
}

#[derive(Debug)]
enum FilterState {
    Normal,
    /// Saw ESC (0x1B), waiting for next byte
    EscapeSeen,
    /// Inside CSI sequence (ESC [), accumulating parameter bytes
    InCsi { buf: Vec<u8> },
    /// Inside OSC sequence (ESC ]), accumulating until ST
    InOsc { buf: Vec<u8> },
    /// Inside DCS sequence (ESC P), accumulating until ST
    InDcs,
    /// Saw 0xC2 at chunk boundary — waiting for next byte to determine
    /// if this is a C1 control (0xC2 0x80-0x9F) or a valid character
    PendingC2,
}

impl Default for OutputFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFilter {
    pub fn new() -> Self {
        info!("output filter initialized");
        Self {
            state: FilterState::Normal,
            output: Vec::with_capacity(8192),
            total_bytes_processed: 0,
            osc52_stripped: 0,
            osc50_stripped: 0,
            c1_bytes_stripped: 0,
            clear_screen_stripped: 0,
            queries_stripped: 0,
            titles_sanitized: 0,
            links_stripped: 0,
        }
    }

    /// Filter a chunk of output. Returns the filtered bytes.
    /// Handles sequences split across chunks via internal state.
    pub fn filter(&mut self, data: &[u8]) -> &[u8] {
        self.total_bytes_processed += data.len() as u64;
        self.output.clear();
        self.output.reserve(data.len());

        let mut i = 0;
        while i < data.len() {
            match &mut self.state {
                FilterState::Normal => {
                    let b = data[i];
                    if b == 0x1B {
                        self.state = FilterState::EscapeSeen;
                        i += 1;
                    } else if b == 0xC2 && i + 1 < data.len() && (0x80..=0x9F).contains(&data[i + 1]) {
                        // C1 control character in UTF-8 encoding (U+0080..U+009F)
                        // Two bytes: 0xC2 followed by 0x80-0x9F
                        self.c1_bytes_stripped += 1;
                        i += 2;
                    } else if b == 0xC2 && i + 1 >= data.len() {
                        // 0xC2 at chunk boundary — could be start of C1 or a valid
                        // two-byte character. Buffer it and decide on next chunk.
                        self.state = FilterState::PendingC2;
                        i += 1;
                    } else {
                        self.output.push(b);
                        i += 1;
                    }
                }
                FilterState::EscapeSeen => {
                    let b = data[i];
                    match b {
                        b'[' => {
                            // CSI sequence
                            self.state = FilterState::InCsi { buf: Vec::new() };
                            i += 1;
                        }
                        b']' => {
                            // OSC sequence
                            self.state = FilterState::InOsc { buf: Vec::new() };
                            i += 1;
                        }
                        b'P' => {
                            // DCS sequence — strip entirely
                            self.state = FilterState::InDcs;
                            i += 1;
                        }
                        _ => {
                            // Other ESC sequences (e.g., ESC =, ESC >, ESC M, etc.)
                            // These are generally safe — pass through
                            self.output.push(0x1B);
                            self.output.push(b);
                            self.state = FilterState::Normal;
                            i += 1;
                        }
                    }
                }
                FilterState::InCsi { buf } => {
                    let b = data[i];
                    buf.push(b);
                    i += 1;

                    // CSI parameters are 0x30-0x3F, intermediates are 0x20-0x2F,
                    // final byte is 0x40-0x7E
                    if (0x40..=0x7E).contains(&b) {
                        // Complete CSI sequence — check if it's a query to strip
                        let csi_buf = std::mem::take(buf);
                        self.state = FilterState::Normal;
                        if self.is_blocked_csi(&csi_buf) {
                            self.queries_stripped += 1;
                        } else {
                            self.output.push(0x1B);
                            self.output.push(b'[');
                            self.output.extend_from_slice(&csi_buf);
                        }
                    }
                    // Still accumulating parameter/intermediate bytes
                }
                FilterState::InOsc { buf } => {
                    let b = data[i];
                    i += 1;

                    // Check if previous byte (in buf) was ESC and this is backslash
                    if b == b'\\' && buf.last() == Some(&0x1B) {
                        // ST terminator (ESC was buffered from previous chunk)
                        buf.pop(); // Remove the ESC from content
                        let osc_buf = std::mem::take(buf);
                        self.state = FilterState::Normal;
                        self.handle_osc(&osc_buf);
                    } else if b == 0x07 {
                        // BEL terminates OSC
                        let osc_buf = std::mem::take(buf);
                        self.state = FilterState::Normal;
                        self.handle_osc(&osc_buf);
                    } else if b == 0x1B {
                        // Could be ESC \ (ST) — peek ahead
                        if i < data.len() && data[i] == b'\\' {
                            // ST terminator
                            i += 1;
                            let osc_buf = std::mem::take(buf);
                            self.state = FilterState::Normal;
                            self.handle_osc(&osc_buf);
                        } else if i >= data.len() {
                            // ESC at end of chunk — could be start of ST
                            buf.push(b);
                        } else {
                            // ESC followed by something else — malformed
                            buf.push(b);
                        }
                    } else {
                        buf.push(b);
                    }
                }
                FilterState::InDcs => {
                    let b = data[i];
                    i += 1;

                    // DCS sequences are stripped entirely — just scan for ST
                    if b == 0x07 {
                        // BEL terminates (some terminals accept this for DCS too)
                        self.queries_stripped += 1;
                        self.state = FilterState::Normal;
                    } else if b == 0x1B
                        && i < data.len() && data[i] == b'\\' {
                        // ST terminator
                        i += 1;
                        self.queries_stripped += 1;
                        self.state = FilterState::Normal;
                        // else: ESC at chunk boundary or inside DCS — keep scanning
                    }
                }
                FilterState::PendingC2 => {
                    let b = data[i];
                    if (0x80..=0x9F).contains(&b) {
                        // C1 control character (U+0080..U+009F) — strip both bytes
                        self.c1_bytes_stripped += 1;
                        i += 1;
                    } else {
                        // Valid UTF-8 two-byte character starting with 0xC2 — emit both
                        self.output.push(0xC2);
                        self.output.push(b);
                        i += 1;
                    }
                    self.state = FilterState::Normal;
                }
            }
        }

        &self.output
    }

    /// Check if a CSI sequence should be blocked.
    fn is_blocked_csi(&mut self, buf: &[u8]) -> bool {
        if buf.is_empty() {
            return false;
        }

        let final_byte = buf[buf.len() - 1];
        let params = &buf[..buf.len() - 1];

        match final_byte {
            // DA primary: CSI c, CSI 0 c
            b'c' => {
                params.is_empty() || params == b"0" || params == b">0" || params == b">"
            }
            // DSR: CSI 6 n (cursor position report request)
            // Also CSI 5 n (device status report)
            b'n' => params == b"6" || params == b"5",
            // DECRQM: CSI ? Ps $ p
            b'p' => params.ends_with(b"$") && params.starts_with(b"?"),
            // Kitty keyboard query: CSI ? u
            b'u' => params == b"?",
            // CSI 2 J — erase entire display. Stripped after initial
            // display setup (~64KB) to prevent scroll jumping. The
            // first few clear-screens are needed for the child process
            // to set up its UI; after that they're redundant repaints
            // that reset the viewport position.
            // CSI 3 J — erase scrollback buffer. Always stripped — this
            // destroys the user's scroll history and snaps the viewport.
            b'J' if params == b"3" || (params == b"2" && self.total_bytes_processed > 65_536) => {
                self.clear_screen_stripped += 1;
                if self.clear_screen_stripped <= 5 {
                    debug!(
                        total_bytes = self.total_bytes_processed,
                        count = self.clear_screen_stripped,
                        variant = ?params,
                        "stripping clear-screen/scrollback"
                    );
                }
                true
            }
            // Note: cursor-home (ESC[H etc.) cannot be stripped — it's
            // needed for correct content positioning during redraws.
            // Stripping it causes content duplication (appended instead
            // of overwriting the active screen area).
            _ => false,
        }
    }

    /// Handle a complete OSC sequence. Either emit (possibly sanitized) or strip.
    fn handle_osc(&mut self, buf: &[u8]) {
        // Determine OSC type from the numeric prefix
        let osc_type = self.parse_osc_type(buf);

        match osc_type {
            Some(52) => {
                // OSC 52 — clipboard access — STRIP
                self.osc52_stripped += 1;
                debug!("stripped OSC 52 (clipboard access)");
            }
            Some(50) => {
                // OSC 50 — font query — STRIP
                self.osc50_stripped += 1;
                debug!("stripped OSC 50 (font query)");
            }
            Some(2) => {
                // OSC 2 — window title — sanitize control characters
                self.titles_sanitized += 1;
                self.emit_sanitized_osc2(buf);
            }
            Some(8) => {
                // OSC 8 — hyperlink — check URL scheme whitelist
                self.handle_osc8(buf);
            }
            _ => {
                // Other OSC sequences — pass through
                self.output.push(0x1B);
                self.output.push(b']');
                self.output.extend_from_slice(buf);
                self.output.push(0x07); // Re-terminate with BEL
            }
        }
    }

    /// Parse the numeric OSC type (e.g., "2" from "2;title text").
    fn parse_osc_type(&self, buf: &[u8]) -> Option<u32> {
        let semi = buf.iter().position(|&b| b == b';').unwrap_or(buf.len());
        let num_str = std::str::from_utf8(&buf[..semi]).ok()?;
        num_str.parse().ok()
    }

    /// Emit an OSC 2 (window title) with control characters stripped from the title.
    fn emit_sanitized_osc2(&mut self, buf: &[u8]) {
        self.output.push(0x1B);
        self.output.push(b']');

        // Find the semicolon separating "2" from the title
        if let Some(semi_pos) = buf.iter().position(|&b| b == b';') {
            // Emit the "2;" prefix
            self.output.extend_from_slice(&buf[..=semi_pos]);
            // Emit title with control characters stripped
            for &b in &buf[semi_pos + 1..] {
                if b >= 0x20 || b == b'\t' {
                    // Printable or tab — allow
                    self.output.push(b);
                }
                // else: control character — strip
            }
        } else {
            // No semicolon — malformed, emit as-is
            self.output.extend_from_slice(buf);
        }

        self.output.push(0x07); // BEL terminator
    }

    /// Handle OSC 8 hyperlinks with URL scheme whitelist.
    /// Format: "8;params;URI" (opening) or "8;;" (closing)
    /// Allowed schemes: http, https, file. Others are stripped (link wrapper
    /// removed, visible text preserved).
    fn handle_osc8(&mut self, buf: &[u8]) {
        // Find the two semicolons: "8;params;URI"
        // First semicolon separates "8" from params
        let after_type = match buf.iter().position(|&b| b == b';') {
            Some(pos) => pos + 1,
            None => {
                // Malformed — pass through
                self.emit_osc_passthrough(buf);
                return;
            }
        };

        // Second semicolon separates params from URI
        let uri_start = match buf[after_type..].iter().position(|&b| b == b';') {
            Some(pos) => after_type + pos + 1,
            None => {
                // Malformed — pass through
                self.emit_osc_passthrough(buf);
                return;
            }
        };

        let uri = &buf[uri_start..];

        // Empty URI = closing tag — always pass through
        if uri.is_empty() {
            self.emit_osc_passthrough(buf);
            return;
        }

        // Check the URI scheme against the whitelist
        if self.is_allowed_scheme(uri) {
            self.emit_osc_passthrough(buf);
        } else {
            // Strip the OSC 8 wrapper — the visible text between opening and
            // closing tags will still appear, just without the hyperlink
            self.links_stripped += 1;
            let scheme_end = uri.iter().position(|&b| b == b':').unwrap_or(0);
            let scheme = std::str::from_utf8(&uri[..scheme_end]).unwrap_or("unknown");
            warn!(scheme, "stripped OSC 8 hyperlink with disallowed scheme");
        }
    }

    /// Check if a URI has an allowed scheme.
    fn is_allowed_scheme(&self, uri: &[u8]) -> bool {
        let uri_lower: Vec<u8> = uri.iter().map(|b| b.to_ascii_lowercase()).collect();
        uri_lower.starts_with(b"http://")
            || uri_lower.starts_with(b"https://")
            || uri_lower.starts_with(b"file://")
    }

    fn emit_osc_passthrough(&mut self, buf: &[u8]) {
        self.output.push(0x1B);
        self.output.push(b']');
        self.output.extend_from_slice(buf);
        self.output.push(0x07);
    }

    pub fn metrics(&self) -> OutputFilterMetrics {
        OutputFilterMetrics {
            osc52_stripped: self.osc52_stripped,
            osc50_stripped: self.osc50_stripped,
            c1_bytes_stripped: self.c1_bytes_stripped,
            queries_stripped: self.queries_stripped,
            titles_sanitized: self.titles_sanitized,
            links_stripped: self.links_stripped,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputFilterMetrics {
    pub osc52_stripped: u64,
    pub osc50_stripped: u64,
    pub c1_bytes_stripped: u64,
    pub queries_stripped: u64,
    pub titles_sanitized: u64,
    pub links_stripped: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text_passes_through() {
        let mut f = OutputFilter::new();
        assert_eq!(f.filter(b"hello world"), b"hello world");
    }

    #[test]
    fn test_sgr_passes_through() {
        let mut f = OutputFilter::new();
        let input = b"\x1b[31mred\x1b[0m";
        assert_eq!(f.filter(input), input.to_vec());
    }

    #[test]
    fn test_cursor_movement_passes_through() {
        let mut f = OutputFilter::new();
        // CUP, CUU, CUD, CUF, CUB
        assert_eq!(f.filter(b"\x1b[10;20H"), b"\x1b[10;20H");
        assert_eq!(f.filter(b"\x1b[5A"), b"\x1b[5A");
        assert_eq!(f.filter(b"\x1b[3B"), b"\x1b[3B");
    }

    #[test]
    fn test_c1_bytes_stripped() {
        let mut f = OutputFilter::new();
        // C1 controls in UTF-8 encoding: 0xC2 followed by 0x80-0x9F
        // U+0090 (DCS) = C2 90, U+009B (CSI) = C2 9B, U+009C (ST) = C2 9C
        let result = f.filter(b"hello\xC2\x90world\xC2\x9Bfoo\xC2\x9C");
        assert_eq!(result, b"helloworldfoo");
        assert_eq!(f.metrics().c1_bytes_stripped, 3);
    }

    #[test]
    fn test_osc52_stripped_bel() {
        let mut f = OutputFilter::new();
        let input = b"before\x1b]52;c;SGVsbG8=\x07after";
        assert_eq!(f.filter(input), b"beforeafter");
        assert_eq!(f.metrics().osc52_stripped, 1);
    }

    #[test]
    fn test_osc52_stripped_st() {
        let mut f = OutputFilter::new();
        let input = b"before\x1b]52;c;SGVsbG8=\x1b\\after";
        assert_eq!(f.filter(input), b"beforeafter");
        assert_eq!(f.metrics().osc52_stripped, 1);
    }

    #[test]
    fn test_osc50_stripped() {
        let mut f = OutputFilter::new();
        let input = b"\x1b]50;font query\x07";
        assert_eq!(f.filter(input), b"");
        assert_eq!(f.metrics().osc50_stripped, 1);
    }

    #[test]
    fn test_osc2_title_passes_clean() {
        let mut f = OutputFilter::new();
        let input = b"\x1b]2;My Terminal\x07";
        let result = f.filter(input);
        assert_eq!(result, b"\x1b]2;My Terminal\x07");
    }

    #[test]
    fn test_osc2_title_sanitized() {
        let mut f = OutputFilter::new();
        // Title with embedded control characters
        let input = b"\x1b]2;Evil\x07";
        // The \x07 here terminates the OSC, title is "Evil" (no control chars to strip)
        let result = f.filter(input);
        assert_eq!(result, b"\x1b]2;Evil\x07");

        // Title with embedded newline and other controls
        let input2 = b"\x1b]2;Title\x0d\x0awith\x01controls\x07";
        let result2 = f.filter(input2);
        // \x0d, \x0a, \x01 should be stripped from title
        assert_eq!(result2, b"\x1b]2;Titlewithcontrols\x07");
        assert_eq!(f.metrics().titles_sanitized, 2);
    }

    #[test]
    fn test_da_primary_stripped() {
        let mut f = OutputFilter::new();
        assert_eq!(f.filter(b"\x1b[c"), b"");
        assert_eq!(f.filter(b"\x1b[0c"), b"");
        assert_eq!(f.metrics().queries_stripped, 2);
    }

    #[test]
    fn test_da_secondary_stripped() {
        let mut f = OutputFilter::new();
        assert_eq!(f.filter(b"\x1b[>c"), b"");
        assert_eq!(f.filter(b"\x1b[>0c"), b"");
        assert_eq!(f.metrics().queries_stripped, 2);
    }

    #[test]
    fn test_dsr_cursor_position_stripped() {
        let mut f = OutputFilter::new();
        assert_eq!(f.filter(b"\x1b[6n"), b"");
        assert_eq!(f.metrics().queries_stripped, 1);
    }

    #[test]
    fn test_dsr_device_status_stripped() {
        let mut f = OutputFilter::new();
        assert_eq!(f.filter(b"\x1b[5n"), b"");
        assert_eq!(f.metrics().queries_stripped, 1);
    }

    #[test]
    fn test_decrqm_stripped() {
        let mut f = OutputFilter::new();
        assert_eq!(f.filter(b"\x1b[?1$p"), b"");
        assert_eq!(f.metrics().queries_stripped, 1);
    }

    #[test]
    fn test_kitty_keyboard_query_stripped() {
        let mut f = OutputFilter::new();
        assert_eq!(f.filter(b"\x1b[?u"), b"");
        assert_eq!(f.metrics().queries_stripped, 1);
    }

    #[test]
    fn test_dcs_stripped() {
        let mut f = OutputFilter::new();
        let input = b"\x1bP$q some data\x1b\\";
        assert_eq!(f.filter(input), b"");
        assert_eq!(f.metrics().queries_stripped, 1);
    }

    #[test]
    fn test_osc8_https_passes_through() {
        let mut f = OutputFilter::new();
        let input = b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07";
        let result = f.filter(input);
        assert_eq!(result, input.to_vec());
        assert_eq!(f.metrics().links_stripped, 0);
    }

    #[test]
    fn test_osc8_http_passes_through() {
        let mut f = OutputFilter::new();
        let input = b"\x1b]8;;http://example.com\x07link\x1b]8;;\x07";
        let result = f.filter(input);
        assert_eq!(result, input.to_vec());
    }

    #[test]
    fn test_osc8_file_passes_through() {
        let mut f = OutputFilter::new();
        let input = b"\x1b]8;;file:///tmp/foo.rs\x07foo.rs\x1b]8;;\x07";
        let result = f.filter(input);
        assert_eq!(result, input.to_vec());
    }

    #[test]
    fn test_osc8_ssh_stripped() {
        let mut f = OutputFilter::new();
        // SSH scheme — blocked (CVE-2023-46322)
        let input = b"\x1b]8;;ssh://evil.com\x07click here\x1b]8;;\x07";
        let result = f.filter(input);
        // Opening link stripped, visible text preserved, closing link passed through
        assert_eq!(result, b"click here\x1b]8;;\x07");
        assert_eq!(f.metrics().links_stripped, 1);
    }

    #[test]
    fn test_osc8_javascript_stripped() {
        let mut f = OutputFilter::new();
        let input = b"\x1b]8;;javascript:alert(1)\x07click\x1b]8;;\x07";
        let result = f.filter(input);
        assert_eq!(result, b"click\x1b]8;;\x07");
        assert_eq!(f.metrics().links_stripped, 1);
    }

    #[test]
    fn test_osc8_x_man_page_stripped() {
        let mut f = OutputFilter::new();
        // x-man-page scheme — blocked (CVE-2023-46321)
        let input = b"\x1b]8;;x-man-page://1/ls\x07ls(1)\x1b]8;;\x07";
        let result = f.filter(input);
        assert_eq!(result, b"ls(1)\x1b]8;;\x07");
        assert_eq!(f.metrics().links_stripped, 1);
    }

    #[test]
    fn test_osc8_closing_tag_always_passes() {
        let mut f = OutputFilter::new();
        // Closing tag (empty URI) must always pass through
        let input = b"\x1b]8;;\x07";
        let result = f.filter(input);
        assert_eq!(result, input.to_vec());
        assert_eq!(f.metrics().links_stripped, 0);
    }

    #[test]
    fn test_osc8_with_params_passes() {
        let mut f = OutputFilter::new();
        // OSC 8 with id parameter
        let input = b"\x1b]8;id=link1;https://example.com\x07text\x1b]8;;\x07";
        let result = f.filter(input);
        assert_eq!(result, input.to_vec());
    }

    #[test]
    fn test_osc8_case_insensitive_scheme() {
        let mut f = OutputFilter::new();
        let input = b"\x1b]8;;HTTPS://example.com\x07link\x1b]8;;\x07";
        let result = f.filter(input);
        assert_eq!(result, input.to_vec());
    }

    #[test]
    fn test_mixed_safe_and_unsafe() {
        let mut f = OutputFilter::new();
        let input = b"\x1b[31mred\x1b]52;c;data\x07\x1b[0m normal\x1b[c";
        let result = f.filter(input);
        // SGR red passes, OSC 52 stripped, SGR reset passes, text passes, DA stripped
        assert_eq!(result, b"\x1b[31mred\x1b[0m normal");
    }

    #[test]
    fn test_empty_input() {
        let mut f = OutputFilter::new();
        assert_eq!(f.filter(b""), b"");
    }

    #[test]
    fn test_chunk_boundary_osc52() {
        let mut f = OutputFilter::new();
        // OSC 52 split across two chunks
        let result1 = f.filter(b"before\x1b]52;c;da").to_vec();
        let result2 = f.filter(b"ta\x07after");
        assert_eq!(result1, b"before");
        assert_eq!(result2, b"after");
    }

    #[test]
    fn test_chunk_boundary_csi() {
        let mut f = OutputFilter::new();
        // CSI sequence split: ESC [ in chunk 1, 31 m in chunk 2
        let result1 = f.filter(b"text\x1b[").to_vec();
        let result2 = f.filter(b"31m more");
        assert_eq!(result1, b"text");
        assert_eq!(result2, b"\x1b[31m more");
    }

    #[test]
    fn test_chunk_boundary_esc_at_end() {
        let mut f = OutputFilter::new();
        // ESC at end of chunk
        let result1 = f.filter(b"text\x1b").to_vec();
        let result2 = f.filter(b"[32mgreen");
        assert_eq!(result1, b"text");
        assert_eq!(result2, b"\x1b[32mgreen");
    }

    #[test]
    fn test_chunk_boundary_st_split() {
        let mut f = OutputFilter::new();
        // OSC 52 with ST (ESC \) split: ESC at chunk end, \ at next chunk start
        // OSC 52 is stripped, so only "next" should appear
        let result1 = f.filter(b"\x1b]52;c;data\x1b").to_vec();
        let result2 = f.filter(b"\\next");
        assert_eq!(result1, b"");
        assert_eq!(result2, b"next");
    }

    #[test]
    fn test_other_esc_sequences_pass_through() {
        let mut f = OutputFilter::new();
        // ESC =, ESC >, ESC M (reverse index) etc.
        assert_eq!(f.filter(b"\x1b="), b"\x1b=");
        assert_eq!(f.filter(b"\x1b>"), b"\x1b>");
        assert_eq!(f.filter(b"\x1bM"), b"\x1bM");
    }

    #[test]
    fn test_all_c1_range_stripped() {
        let mut f = OutputFilter::new();
        // All C1 controls in UTF-8 encoding: 0xC2 0x80 through 0xC2 0x9F
        let mut input = Vec::new();
        for b in 0x80u8..=0x9F {
            input.push(0xC2);
            input.push(b);
        }
        let result = f.filter(&input);
        assert!(result.is_empty());
        assert_eq!(f.metrics().c1_bytes_stripped, 32);
    }

    #[test]
    fn test_utf8_continuation_bytes_preserved() {
        let mut f = OutputFilter::new();
        // U+276F (❯) = E2 9D AF — 0x9D is a continuation byte, not a C1 control
        let input = "before❯after".as_bytes();
        let result = f.filter(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_utf8_box_drawing_preserved() {
        let mut f = OutputFilter::new();
        // Box-drawing characters with continuation bytes in 0x80-0x9F range
        // U+2500 (─) = E2 94 80, U+2502 (│) = E2 94 82
        let input = "┌──┐│hi│└──┘".as_bytes();
        let result = f.filter(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_c1_at_chunk_boundary() {
        let mut f = OutputFilter::new();
        // C1 control split across chunks: 0xC2 at end of chunk 1, 0x90 at start of chunk 2
        let result1 = f.filter(b"text\xC2").to_vec();
        let result2 = f.filter(b"\x90more");
        assert_eq!(result1, b"text");
        assert_eq!(result2, b"more");
        assert_eq!(f.metrics().c1_bytes_stripped, 1);
    }

    #[test]
    fn test_c2_non_c1_at_chunk_boundary() {
        let mut f = OutputFilter::new();
        // Valid UTF-8 char starting with 0xC2 split across chunks
        // U+00A9 (©) = C2 A9 — NOT a C1 control
        let result1 = f.filter(b"text\xC2").to_vec();
        let result2 = f.filter(b"\xA9more");
        assert_eq!(result1, b"text");
        assert_eq!(result2, b"\xC2\xA9more");
    }
}
