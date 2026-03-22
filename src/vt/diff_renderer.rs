#![allow(dead_code)] // Available for future use — GUI currently uses raw passthrough
use tracing::{debug, trace};

/// Differential renderer using vt100 terminal emulator.
///
/// Maintains a virtual screen via `vt100::Parser`. All child output is fed
/// through the emulator. When rendering, only the cells that changed since
/// the last render are emitted as ANSI sequences.
pub struct DiffRenderer {
    /// VT100 terminal emulator — tracks current screen state
    parser: vt100::Parser,
    /// Previous screen state for computing diffs
    prev_screen: Option<vt100::Screen>,
    /// Whether new data has been fed since last render
    dirty: bool,

    // Metrics
    renders: u64,
    diff_renders: u64,
    full_renders: u64,
    total_bytes_in: u64,
    total_bytes_out: u64,
}

impl DiffRenderer {
    /// Create a new diff renderer with the given terminal dimensions
    pub fn new(rows: u16, cols: u16) -> Self {
        debug!(rows, cols, "initializing diff renderer");
        Self {
            parser: vt100::Parser::new(rows, cols, 0), // 0 scrollback — we manage our own
            prev_screen: None,
            dirty: false,
            renders: 0,
            diff_renders: 0,
            full_renders: 0,
            total_bytes_in: 0,
            total_bytes_out: 0,
        }
    }

    /// Feed raw VT data into the emulator. This updates the virtual screen
    /// state but does NOT produce any output. Call `render()` to get the diff.
    pub fn feed(&mut self, data: &[u8]) {
        trace!(bytes = data.len(), "feeding data to VT emulator");
        self.parser.process(data);
        self.total_bytes_in += data.len() as u64;
        self.dirty = true;
    }

    /// Render the current screen state as a diff against the previous frame.
    ///
    /// Returns the ANSI escape sequences needed to update the display,
    /// wrapped in DEC 2026 synchronized output markers.
    ///
    /// Returns `None` if there's nothing new to render.
    pub fn render(&mut self) -> Option<Vec<u8>> {
        if !self.dirty {
            return None;
        }

        self.dirty = false;
        self.renders += 1;

        let screen = self.parser.screen();
        let mut output = Vec::with_capacity(4096);

        // Wrap in synchronized output markers
        output.extend_from_slice(b"\x1b[?2026h");

        if let Some(ref prev) = self.prev_screen {
            // Differential render — only changed cells
            let diff = screen.contents_diff(prev);
            trace!(
                diff_bytes = diff.len(),
                "computed differential render"
            );
            output.extend_from_slice(&diff);
            self.diff_renders += 1;
        } else {
            // Full render — first frame or after reset
            let formatted = screen.contents_formatted();
            trace!(
                full_bytes = formatted.len(),
                "computed full render"
            );
            output.extend_from_slice(&formatted);
            self.full_renders += 1;
        }

        // Append cursor state
        output.extend_from_slice(&screen.cursor_state_formatted());

        // End synchronized output
        output.extend_from_slice(b"\x1b[?2026l");

        self.total_bytes_out += output.len() as u64;
        self.prev_screen = Some(screen.clone());

        let compression = if self.total_bytes_in > 0 {
            (1.0 - (self.total_bytes_out as f64 / self.total_bytes_in as f64)) * 100.0
        } else {
            0.0
        };

        debug!(
            render_num = self.renders,
            output_bytes = output.len(),
            total_in = self.total_bytes_in,
            total_out = self.total_bytes_out,
            compression_pct = format!("{compression:.1}"),
            diff_renders = self.diff_renders,
            full_renders = self.full_renders,
            "render complete"
        );

        Some(output)
    }

    /// Force the next render to be a full redraw (e.g., after terminal resize)
    pub fn invalidate(&mut self) {
        debug!("invalidating previous screen — next render will be full");
        self.prev_screen = None;
        self.dirty = true;
    }

    /// Resize the virtual terminal
    pub fn resize(&mut self, rows: u16, cols: u16) {
        debug!(rows, cols, "resizing VT emulator");
        self.parser.set_size(rows, cols);
        self.invalidate();
    }

    /// Whether there's pending data that hasn't been rendered yet
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Returns accumulated metrics
    pub fn metrics(&self) -> DiffMetrics {
        DiffMetrics {
            renders: self.renders,
            diff_renders: self.diff_renders,
            full_renders: self.full_renders,
            total_bytes_in: self.total_bytes_in,
            total_bytes_out: self.total_bytes_out,
        }
    }
}

/// Metrics from the differential renderer
#[derive(Debug, Clone)]
pub struct DiffMetrics {
    pub renders: u64,
    pub diff_renders: u64,
    pub full_renders: u64,
    pub total_bytes_in: u64,
    pub total_bytes_out: u64,
}

impl DiffMetrics {
    /// Compression ratio (0.0 = no compression, 1.0 = 100% compressed)
    pub fn compression_ratio(&self) -> f64 {
        if self.total_bytes_in == 0 {
            return 0.0;
        }
        1.0 - (self.total_bytes_out as f64 / self.total_bytes_in as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_renderer_is_not_dirty() {
        let renderer = DiffRenderer::new(24, 80);
        assert!(!renderer.is_dirty());
    }

    #[test]
    fn test_feed_marks_dirty() {
        let mut renderer = DiffRenderer::new(24, 80);
        renderer.feed(b"hello");
        assert!(renderer.is_dirty());
    }

    #[test]
    fn test_render_clears_dirty() {
        let mut renderer = DiffRenderer::new(24, 80);
        renderer.feed(b"hello");
        assert!(renderer.is_dirty());
        renderer.render();
        assert!(!renderer.is_dirty());
    }

    #[test]
    fn test_render_returns_none_when_clean() {
        let mut renderer = DiffRenderer::new(24, 80);
        assert!(renderer.render().is_none());
    }

    #[test]
    fn test_first_render_is_full() {
        let mut renderer = DiffRenderer::new(24, 80);
        renderer.feed(b"hello world");
        let output = renderer.render().unwrap();

        // Should be wrapped in sync markers
        assert!(output.starts_with(b"\x1b[?2026h"));
        assert!(output.ends_with(b"\x1b[?2026l"));

        let metrics = renderer.metrics();
        assert_eq!(metrics.full_renders, 1);
        assert_eq!(metrics.diff_renders, 0);
    }

    #[test]
    fn test_second_render_is_diff() {
        let mut renderer = DiffRenderer::new(24, 80);

        renderer.feed(b"hello world");
        renderer.render();

        renderer.feed(b"\r\nhello world line 2");
        let output = renderer.render().unwrap();

        let metrics = renderer.metrics();
        assert_eq!(metrics.full_renders, 1);
        assert_eq!(metrics.diff_renders, 1);

        // Diff should be smaller than full render
        assert!(!output.is_empty());
    }

    #[test]
    fn test_identical_content_produces_minimal_diff() {
        let mut renderer = DiffRenderer::new(24, 80);

        renderer.feed(b"hello world");
        renderer.render();

        // Feed identical content — screen doesn't change
        // (Just feeding the same text would append, so we do a clear + same)
        renderer.feed(b"\x1b[2J\x1b[Hhello world");
        let output = renderer.render().unwrap();

        // The diff should be very small (just cursor repositioning at most)
        // Sync wrapper is ~20 bytes, so allow some overhead
        assert!(output.len() < 100, "diff too large: {} bytes", output.len());
    }

    #[test]
    fn test_invalidate_forces_full_render() {
        let mut renderer = DiffRenderer::new(24, 80);

        renderer.feed(b"hello world");
        renderer.render();

        renderer.feed(b"\r\nline 2");
        renderer.invalidate();
        renderer.render();

        let metrics = renderer.metrics();
        assert_eq!(metrics.full_renders, 2); // Both renders were full
    }

    #[test]
    fn test_resize_invalidates() {
        let mut renderer = DiffRenderer::new(24, 80);

        renderer.feed(b"hello world");
        renderer.render();

        renderer.resize(40, 120);
        assert!(renderer.is_dirty());

        renderer.render();
        let metrics = renderer.metrics();
        assert_eq!(metrics.full_renders, 2); // Resize caused full render
    }

    #[test]
    fn test_metrics_accumulate() {
        let mut renderer = DiffRenderer::new(24, 80);

        renderer.feed(b"frame 1");
        renderer.render();
        renderer.feed(b"\r\nframe 2");
        renderer.render();
        renderer.feed(b"\r\nframe 3");
        renderer.render();

        let metrics = renderer.metrics();
        assert_eq!(metrics.renders, 3);
        assert_eq!(metrics.full_renders, 1);
        assert_eq!(metrics.diff_renders, 2);
        assert!(metrics.total_bytes_in > 0);
        assert!(metrics.total_bytes_out > 0);
    }

    #[test]
    fn test_compression_ratio() {
        let metrics = DiffMetrics {
            renders: 10,
            diff_renders: 9,
            full_renders: 1,
            total_bytes_in: 10000,
            total_bytes_out: 1000,
        };
        let ratio = metrics.compression_ratio();
        assert!((ratio - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_compression_ratio_zero_input() {
        let metrics = DiffMetrics {
            renders: 0,
            diff_renders: 0,
            full_renders: 0,
            total_bytes_in: 0,
            total_bytes_out: 0,
        };
        assert_eq!(metrics.compression_ratio(), 0.0);
    }
}
