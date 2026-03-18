// VT capture recorder — records filtered VT output for replay testing.
//
// Format: VTCAP/1 with human-readable text header followed by binary chunks.
// Each chunk is [8-byte timestamp_us LE][4-byte length LE][N bytes data].
//
// Gated behind `--features recording` — not compiled into release builds.

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::time::{Instant, SystemTime};

use anyhow::{bail, Context, Result};
use tracing::{debug, info};

#[allow(dead_code)] // Used by read_vtcap (consumed by external test crate)
const MAGIC: &[u8] = b"VTCAP/1\n";

/// Format a SystemTime as a simplified ISO 8601 / RFC 3339 string.
/// Avoids pulling in the `chrono` crate for a single timestamp.
fn format_timestamp(t: SystemTime) -> String {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(dur) => {
            let secs = dur.as_secs();
            // Calculate date/time components from unix timestamp
            let days = secs / 86400;
            let time_of_day = secs % 86400;
            let hours = time_of_day / 3600;
            let minutes = (time_of_day % 3600) / 60;
            let seconds = time_of_day % 60;

            // Days since epoch to y/m/d (civil_from_days algorithm)
            let z = days as i64 + 719468;
            let era = z.div_euclid(146097);
            let doe = z.rem_euclid(146097) as u64;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let y = yoe as i64 + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = if m <= 2 { y + 1 } else { y };

            format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
        }
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}

/// Records filtered VT output to a .vtcap file.
pub struct VtcapRecorder {
    writer: BufWriter<File>,
    start: Instant,
    chunks_written: u64,
    bytes_written: u64,
}

impl VtcapRecorder {
    /// Create a new recorder, writing the header immediately.
    pub fn create(path: &Path, cols: u16, rows: u16, tool: &impl std::fmt::Display) -> Result<Self> {
        let file = File::create(path)
            .with_context(|| format!("failed to create vtcap file: {}", path.display()))?;
        let mut writer = BufWriter::new(file);

        // Write human-readable header
        let now = format_timestamp(SystemTime::now());
        write!(
            writer,
            "VTCAP/1\ncols={cols}\nrows={rows}\ntool={tool}\nrecorded={now}\n\n"
        )
        .context("failed to write vtcap header")?;
        writer.flush().context("failed to flush vtcap header")?;

        info!(
            path = %path.display(),
            cols,
            rows,
            "vtcap recording started"
        );

        Ok(Self {
            writer,
            start: Instant::now(),
            chunks_written: 0,
            bytes_written: 0,
        })
    }

    /// Write a chunk of VT data with timestamp.
    pub fn write_chunk(&mut self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        let timestamp_us = self.start.elapsed().as_micros() as u64;
        let length = data.len() as u32;

        self.writer
            .write_all(&timestamp_us.to_le_bytes())
            .context("failed to write chunk timestamp")?;
        self.writer
            .write_all(&length.to_le_bytes())
            .context("failed to write chunk length")?;
        self.writer
            .write_all(data)
            .context("failed to write chunk data")?;

        self.chunks_written += 1;
        self.bytes_written += data.len() as u64;

        Ok(())
    }

    /// Flush and finalize the recording.
    pub fn finish(mut self) -> Result<()> {
        self.writer.flush().context("failed to flush vtcap file")?;
        info!(
            chunks = self.chunks_written,
            bytes = self.bytes_written,
            elapsed_ms = self.start.elapsed().as_millis() as u64,
            "vtcap recording finished"
        );
        Ok(())
    }
}

/// Parsed header from a .vtcap file.
#[allow(dead_code)] // Consumed by replay tests in external test crate
#[derive(Debug, Clone)]
pub struct VtcapHeader {
    pub cols: u16,
    pub rows: u16,
    pub tool: String,
    pub recorded: String,
}

/// A single chunk of recorded VT data.
#[allow(dead_code)] // Consumed by replay tests in external test crate
#[derive(Debug, Clone)]
pub struct VtcapChunk {
    pub timestamp_us: u64,
    pub data: Vec<u8>,
}

/// Read a .vtcap file, returning the header and all chunks.
#[allow(dead_code)] // Consumed by replay tests in external test crate
pub fn read_vtcap(path: &Path) -> Result<(VtcapHeader, Vec<VtcapChunk>)> {
    let file = File::open(path)
        .with_context(|| format!("failed to open vtcap file: {}", path.display()))?;
    let mut reader = BufReader::new(file);

    // Read and validate magic
    let mut magic_buf = vec![0u8; MAGIC.len()];
    reader
        .read_exact(&mut magic_buf)
        .context("failed to read vtcap magic")?;
    if magic_buf != MAGIC {
        bail!("not a valid vtcap file (bad magic)");
    }

    // Read header lines until empty line
    let mut cols: u16 = 0;
    let mut rows: u16 = 0;
    let mut tool = String::new();
    let mut recorded = String::new();

    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .context("failed to read header line")?;
        let line = line.trim_end_matches('\n').trim_end_matches('\r');

        if line.is_empty() {
            break;
        }

        if let Some(val) = line.strip_prefix("cols=") {
            cols = val.parse().context("invalid cols value")?;
        } else if let Some(val) = line.strip_prefix("rows=") {
            rows = val.parse().context("invalid rows value")?;
        } else if let Some(val) = line.strip_prefix("tool=") {
            tool = val.to_string();
        } else if let Some(val) = line.strip_prefix("recorded=") {
            recorded = val.to_string();
        }
    }

    let header = VtcapHeader {
        cols,
        rows,
        tool,
        recorded,
    };

    // Read binary chunks
    let mut chunks = Vec::new();
    let mut ts_buf = [0u8; 8];
    let mut len_buf = [0u8; 4];

    loop {
        match reader.read_exact(&mut ts_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e).context("failed to read chunk timestamp"),
        }

        reader
            .read_exact(&mut len_buf)
            .context("failed to read chunk length")?;

        let timestamp_us = u64::from_le_bytes(ts_buf);
        let length = u32::from_le_bytes(len_buf) as usize;

        let mut data = vec![0u8; length];
        reader
            .read_exact(&mut data)
            .context("failed to read chunk data")?;

        chunks.push(VtcapChunk { timestamp_us, data });
    }

    debug!(
        chunks = chunks.len(),
        cols = header.cols,
        rows = header.rows,
        "vtcap file loaded"
    );

    Ok((header, chunks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    #[test]
    fn test_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.vtcap");

        // Write
        {
            let mut rec = VtcapRecorder::create(&path, 120, 30, &"claude").unwrap();
            rec.write_chunk(b"hello world").unwrap();
            rec.write_chunk(b"\x1b[31mred\x1b[0m").unwrap();
            rec.finish().unwrap();
        }

        // Read back
        let (header, chunks) = read_vtcap(&path).unwrap();
        assert_eq!(header.cols, 120);
        assert_eq!(header.rows, 30);
        assert_eq!(header.tool, "claude");
        assert!(!header.recorded.is_empty());
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data, b"hello world");
        assert_eq!(chunks[1].data, b"\x1b[31mred\x1b[0m");
        assert!(chunks[1].timestamp_us >= chunks[0].timestamp_us);
    }

    #[test]
    fn test_empty_recording() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.vtcap");

        {
            let rec = VtcapRecorder::create(&path, 80, 24, &"test").unwrap();
            rec.finish().unwrap();
        }

        let (header, chunks) = read_vtcap(&path).unwrap();
        assert_eq!(header.cols, 80);
        assert_eq!(header.rows, 24);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_empty_chunk_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("skip.vtcap");

        {
            let mut rec = VtcapRecorder::create(&path, 80, 24, &"test").unwrap();
            rec.write_chunk(b"").unwrap();
            rec.write_chunk(b"data").unwrap();
            rec.finish().unwrap();
        }

        let (_, chunks) = read_vtcap(&path).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, b"data");
    }

    #[test]
    fn test_large_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.vtcap");

        let big_data = vec![0x42u8; 256 * 1024]; // 256 KiB

        {
            let mut rec = VtcapRecorder::create(&path, 200, 50, &"claude").unwrap();
            rec.write_chunk(&big_data).unwrap();
            rec.finish().unwrap();
        }

        let (_, chunks) = read_vtcap(&path).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data.len(), 256 * 1024);
        assert!(chunks[0].data.iter().all(|&b| b == 0x42));
    }

    #[test]
    fn test_header_readable_with_head() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("head.vtcap");

        {
            let rec = VtcapRecorder::create(&path, 120, 30, &"claude").unwrap();
            rec.finish().unwrap();
        }

        // Read first 200 bytes — should be human-readable
        let mut file = File::open(&path).unwrap();
        let mut buf = vec![0u8; 200];
        let n = file.read(&mut buf).unwrap();
        let text = String::from_utf8_lossy(&buf[..n]);
        assert!(text.starts_with("VTCAP/1\n"));
        assert!(text.contains("cols=120"));
        assert!(text.contains("rows=30"));
        assert!(text.contains("tool=claude"));
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.vtcap");

        {
            let mut file = File::create(&path).unwrap();
            file.write_all(b"NOT_VTCAP\n").unwrap();
        }

        assert!(read_vtcap(&path).is_err());
    }

    #[test]
    fn test_timestamp_format() {
        // Unix epoch
        let t = SystemTime::UNIX_EPOCH;
        assert_eq!(format_timestamp(t), "1970-01-01T00:00:00Z");

        // Known date: 2025-01-01T00:00:00Z = 1735689600 seconds since epoch
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1735689600);
        assert_eq!(format_timestamp(t), "2025-01-01T00:00:00Z");
    }
}
