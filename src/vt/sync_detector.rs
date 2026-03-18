use memchr::memmem;
use tracing::{debug, trace};

/// DEC Mode 2026 synchronized output markers
const SYNC_START: &[u8] = b"\x1b[?2026h"; // Begin Synchronized Update (BSU)
const SYNC_END: &[u8] = b"\x1b[?2026l"; // End Synchronized Update (ESU)
const CLEAR_SCREEN: &[u8] = b"\x1b[2J";
const CURSOR_HOME: &[u8] = b"\x1b[H";

/// Events emitted by the sync block detector
#[derive(Debug, PartialEq)]
pub enum SyncEvent<'a> {
    /// Data outside any sync block — pass through normally
    PassThrough(&'a [u8]),
    /// A complete sync block has been received
    SyncBlock {
        data: Vec<u8>,
        is_full_redraw: bool,
    },
}

/// Detects DEC Mode 2026 synchronized output blocks in a VT byte stream.
///
/// Accumulates data between BSU and ESU markers. Data outside sync blocks
/// is emitted as PassThrough events immediately.
pub struct SyncBlockDetector {
    /// Buffer for accumulating sync block content
    sync_buffer: Vec<u8>,
    /// Whether we're currently inside a sync block
    in_sync_block: bool,
    /// Pre-compiled finders for SIMD-accelerated byte search
    sync_start_finder: memmem::Finder<'static>,
    sync_end_finder: memmem::Finder<'static>,
    clear_screen_finder: memmem::Finder<'static>,
    cursor_home_finder: memmem::Finder<'static>,

    // Metrics
    sync_blocks_detected: u64,
    full_redraws_detected: u64,
    bytes_in_sync_blocks: u64,
}

impl SyncBlockDetector {
    /// Maximum sync block buffer size (1 MiB) — prevents unbounded memory growth
    const MAX_SYNC_BUFFER: usize = 1024 * 1024;

    pub fn new() -> Self {
        Self {
            sync_buffer: Vec::with_capacity(64 * 1024), // 64 KiB initial
            in_sync_block: false,
            sync_start_finder: memmem::Finder::new(SYNC_START),
            sync_end_finder: memmem::Finder::new(SYNC_END),
            clear_screen_finder: memmem::Finder::new(CLEAR_SCREEN),
            cursor_home_finder: memmem::Finder::new(CURSOR_HOME),
            sync_blocks_detected: 0,
            full_redraws_detected: 0,
            bytes_in_sync_blocks: 0,
        }
    }

    /// Process incoming bytes and emit sync events.
    ///
    /// Returns a Vec of events. Data outside sync blocks is returned as
    /// PassThrough slices (zero-copy references into `data`). Complete
    /// sync blocks are returned as owned SyncBlock events.
    pub fn process<'a>(&mut self, data: &'a [u8]) -> Vec<SyncEvent<'a>> {
        let mut events = Vec::new();
        let mut remaining = data;

        while !remaining.is_empty() {
            if self.in_sync_block {
                // Look for sync end marker
                if let Some(end_pos) = self.sync_end_finder.find(remaining) {
                    // Accumulate up to (but not including) the end marker
                    self.sync_buffer
                        .extend_from_slice(&remaining[..end_pos]);

                    let block_data = std::mem::take(&mut self.sync_buffer);
                    let is_full_redraw = self.is_full_redraw(&block_data);

                    self.sync_blocks_detected += 1;
                    self.bytes_in_sync_blocks += block_data.len() as u64;
                    if is_full_redraw {
                        self.full_redraws_detected += 1;
                    }

                    debug!(
                        block_size = block_data.len(),
                        is_full_redraw,
                        total_blocks = self.sync_blocks_detected,
                        "sync block complete"
                    );

                    events.push(SyncEvent::SyncBlock {
                        data: block_data,
                        is_full_redraw,
                    });

                    self.in_sync_block = false;
                    remaining = &remaining[end_pos + SYNC_END.len()..];
                } else {
                    // No end marker found — accumulate everything
                    if self.sync_buffer.len() + remaining.len() <= Self::MAX_SYNC_BUFFER {
                        self.sync_buffer.extend_from_slice(remaining);
                    } else {
                        // Buffer overflow — flush as pass-through to prevent memory issues
                        tracing::warn!(
                            buffer_size = self.sync_buffer.len(),
                            incoming = remaining.len(),
                            "sync buffer overflow, flushing as pass-through"
                        );
                        let flushed = std::mem::take(&mut self.sync_buffer);
                        events.push(SyncEvent::PassThrough(remaining));
                        // We can't return the owned flushed data as a PassThrough reference,
                        // so emit it as a sync block without the full-redraw flag
                        events.push(SyncEvent::SyncBlock {
                            data: flushed,
                            is_full_redraw: false,
                        });
                        self.in_sync_block = false;
                    }
                    break;
                }
            } else {
                // Look for sync start marker
                if let Some(start_pos) = self.sync_start_finder.find(remaining) {
                    // Emit everything before the sync start as pass-through
                    if start_pos > 0 {
                        trace!(bytes = start_pos, "pass-through data before sync block");
                        events.push(SyncEvent::PassThrough(&remaining[..start_pos]));
                    }

                    self.in_sync_block = true;
                    self.sync_buffer.clear();
                    remaining = &remaining[start_pos + SYNC_START.len()..];
                } else {
                    // No sync markers — everything is pass-through
                    if !remaining.is_empty() {
                        events.push(SyncEvent::PassThrough(remaining));
                    }
                    break;
                }
            }
        }

        events
    }

    /// Check if a sync block contains a full-screen redraw.
    ///
    /// Detects ESC[2J (clear screen) paired with any cursor-home variant:
    /// ESC[H, ESC[;H, ESC[1;1H, ESC[1H — ConPTY uses different forms
    /// depending on context and these are all semantically equivalent.
    fn is_full_redraw(&self, data: &[u8]) -> bool {
        if self.clear_screen_finder.find(data).is_none() {
            return false;
        }
        // Check for any cursor-home variant
        self.cursor_home_finder.find(data).is_some()
            || contains_cursor_home_variant(data)
    }

    /// Returns whether the detector is currently inside a sync block
    pub fn in_sync_block(&self) -> bool {
        self.in_sync_block
    }

    /// Returns accumulated metrics
    pub fn metrics(&self) -> SyncMetrics {
        SyncMetrics {
            sync_blocks_detected: self.sync_blocks_detected,
            full_redraws_detected: self.full_redraws_detected,
            bytes_in_sync_blocks: self.bytes_in_sync_blocks,
        }
    }
}

/// Check for cursor-home variants beyond the exact `ESC[H`:
/// `ESC[1;1H`, `ESC[;H`, `ESC[1H`
fn contains_cursor_home_variant(data: &[u8]) -> bool {
    // Search for ESC[ prefix then check what follows
    let mut i = 0;
    while i + 2 < data.len() {
        if data[i] == 0x1b && data[i + 1] == b'[' {
            // Parse CSI parameters to check for cursor-home variants
            let start = i + 2;
            let mut j = start;
            // Consume parameter bytes (digits, semicolons)
            while j < data.len() && (data[j].is_ascii_digit() || data[j] == b';') {
                j += 1;
            }
            // Check if it ends with 'H'
            if j < data.len() && data[j] == b'H' {
                let params = &data[start..j];
                // Match: ESC[H (empty), ESC[;H, ESC[1;1H, ESC[1H
                if params.is_empty()
                    || params == b";"
                    || params == b"1;1"
                    || params == b"1"
                {
                    return true;
                }
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    false
}

impl Default for SyncBlockDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics from sync block detection
#[derive(Debug, Clone)]
pub struct SyncMetrics {
    pub sync_blocks_detected: u64,
    pub full_redraws_detected: u64,
    pub bytes_in_sync_blocks: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_sync_markers_passes_through() {
        let mut detector = SyncBlockDetector::new();
        let data = b"hello world";
        let events = detector.process(data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            SyncEvent::PassThrough(d) => assert_eq!(*d, b"hello world"),
            _ => panic!("expected PassThrough"),
        }
    }

    #[test]
    fn test_complete_sync_block() {
        let mut detector = SyncBlockDetector::new();
        let mut data = Vec::new();
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(b"content inside block");
        data.extend_from_slice(SYNC_END);

        let events = detector.process(&data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            SyncEvent::SyncBlock {
                data,
                is_full_redraw,
            } => {
                assert_eq!(data, b"content inside block");
                assert!(!is_full_redraw);
            }
            _ => panic!("expected SyncBlock"),
        }
    }

    #[test]
    fn test_sync_block_with_full_redraw() {
        let mut detector = SyncBlockDetector::new();
        let mut data = Vec::new();
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(CLEAR_SCREEN);
        data.extend_from_slice(CURSOR_HOME);
        data.extend_from_slice(b"screen content");
        data.extend_from_slice(SYNC_END);

        let events = detector.process(&data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            SyncEvent::SyncBlock {
                is_full_redraw, ..
            } => {
                assert!(is_full_redraw);
            }
            _ => panic!("expected SyncBlock"),
        }
    }

    #[test]
    fn test_data_before_and_after_sync_block() {
        let mut detector = SyncBlockDetector::new();
        let mut data = Vec::new();
        data.extend_from_slice(b"before");
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(b"inside");
        data.extend_from_slice(SYNC_END);
        data.extend_from_slice(b"after");

        let events = detector.process(&data);
        assert_eq!(events.len(), 3);

        match &events[0] {
            SyncEvent::PassThrough(d) => assert_eq!(*d, b"before"),
            _ => panic!("expected PassThrough"),
        }
        match &events[1] {
            SyncEvent::SyncBlock { data, .. } => assert_eq!(data, b"inside"),
            _ => panic!("expected SyncBlock"),
        }
        match &events[2] {
            SyncEvent::PassThrough(d) => assert_eq!(*d, b"after"),
            _ => panic!("expected PassThrough"),
        }
    }

    #[test]
    fn test_split_sync_block_across_calls() {
        let mut detector = SyncBlockDetector::new();

        // First chunk: sync start + partial content
        let mut data1 = Vec::new();
        data1.extend_from_slice(SYNC_START);
        data1.extend_from_slice(b"partial");
        let events1 = detector.process(&data1);
        assert!(events1.is_empty()); // All accumulated, no events yet
        assert!(detector.in_sync_block());

        // Second chunk: rest of content + sync end
        let mut data2 = Vec::new();
        data2.extend_from_slice(b" content");
        data2.extend_from_slice(SYNC_END);
        let events2 = detector.process(&data2);
        assert_eq!(events2.len(), 1);
        match &events2[0] {
            SyncEvent::SyncBlock { data, .. } => {
                assert_eq!(data, b"partial content");
            }
            _ => panic!("expected SyncBlock"),
        }
        assert!(!detector.in_sync_block());
    }

    #[test]
    fn test_multiple_sync_blocks() {
        let mut detector = SyncBlockDetector::new();
        let mut data = Vec::new();
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(b"block1");
        data.extend_from_slice(SYNC_END);
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(b"block2");
        data.extend_from_slice(SYNC_END);

        let events = detector.process(&data);
        assert_eq!(events.len(), 2);

        let metrics = detector.metrics();
        assert_eq!(metrics.sync_blocks_detected, 2);
    }

    #[test]
    fn test_empty_input() {
        let mut detector = SyncBlockDetector::new();
        let events = detector.process(b"");
        assert!(events.is_empty());
    }

    #[test]
    fn test_metrics_accumulate() {
        let mut detector = SyncBlockDetector::new();

        let mut data = Vec::new();
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(CLEAR_SCREEN);
        data.extend_from_slice(CURSOR_HOME);
        data.extend_from_slice(b"redraw content");
        data.extend_from_slice(SYNC_END);

        detector.process(&data);
        let metrics = detector.metrics();
        assert_eq!(metrics.sync_blocks_detected, 1);
        assert_eq!(metrics.full_redraws_detected, 1);
        assert!(metrics.bytes_in_sync_blocks > 0);
    }

    #[test]
    fn test_full_redraw_with_cursor_home_1_1() {
        let mut detector = SyncBlockDetector::new();
        let mut data = Vec::new();
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(CLEAR_SCREEN);
        data.extend_from_slice(b"\x1b[1;1H"); // ESC[1;1H variant
        data.extend_from_slice(b"screen content");
        data.extend_from_slice(SYNC_END);

        let events = detector.process(&data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            SyncEvent::SyncBlock { is_full_redraw, .. } => {
                assert!(is_full_redraw, "ESC[1;1H should be detected as full redraw");
            }
            _ => panic!("expected SyncBlock"),
        }
    }

    #[test]
    fn test_full_redraw_with_cursor_home_semicolon() {
        let mut detector = SyncBlockDetector::new();
        let mut data = Vec::new();
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(CLEAR_SCREEN);
        data.extend_from_slice(b"\x1b[;H"); // ESC[;H variant
        data.extend_from_slice(b"screen content");
        data.extend_from_slice(SYNC_END);

        let events = detector.process(&data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            SyncEvent::SyncBlock { is_full_redraw, .. } => {
                assert!(is_full_redraw, "ESC[;H should be detected as full redraw");
            }
            _ => panic!("expected SyncBlock"),
        }
    }

    #[test]
    fn test_full_redraw_with_cursor_home_1() {
        let mut detector = SyncBlockDetector::new();
        let mut data = Vec::new();
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(CLEAR_SCREEN);
        data.extend_from_slice(b"\x1b[1H"); // ESC[1H variant
        data.extend_from_slice(b"screen content");
        data.extend_from_slice(SYNC_END);

        let events = detector.process(&data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            SyncEvent::SyncBlock { is_full_redraw, .. } => {
                assert!(is_full_redraw, "ESC[1H should be detected as full redraw");
            }
            _ => panic!("expected SyncBlock"),
        }
    }

    #[test]
    fn test_cursor_position_not_home_not_full_redraw() {
        let mut detector = SyncBlockDetector::new();
        let mut data = Vec::new();
        data.extend_from_slice(SYNC_START);
        data.extend_from_slice(CLEAR_SCREEN);
        data.extend_from_slice(b"\x1b[5;10H"); // Not cursor home
        data.extend_from_slice(b"screen content");
        data.extend_from_slice(SYNC_END);

        let events = detector.process(&data);
        assert_eq!(events.len(), 1);
        match &events[0] {
            SyncEvent::SyncBlock { is_full_redraw, .. } => {
                assert!(!is_full_redraw, "ESC[5;10H is not cursor home");
            }
            _ => panic!("expected SyncBlock"),
        }
    }
}
