// Sync block filter for reducing scrollback duplication.
//
// Claude Code's Ink renderer wraps full-screen redraws in DEC 2026
// sync markers at ~30fps. Most frames are byte-identical (redundant
// redraws with no new content). This filter suppresses those redundant
// sync blocks before they reach xterm.js, preventing the scrollback
// duplication that occurs when Ink's erase-and-rewrite cycle pushes
// duplicate content past the viewport.

// DEC Mode 2026 markers (8 bytes each)
const BSU = new Uint8Array([0x1b, 0x5b, 0x3f, 0x32, 0x30, 0x32, 0x36, 0x68]); // ESC[?2026h
const ESU = new Uint8Array([0x1b, 0x5b, 0x3f, 0x32, 0x30, 0x32, 0x36, 0x6c]); // ESC[?2026l
const MAX_SYNC_BUFFER = 1024 * 1024; // 1 MiB safety valve

function fnv1aHash(data: Uint8Array, start: number, end: number): number {
  let hash = 0x811c9dc5; // FNV offset basis (32-bit)
  for (let i = start; i < end; i++) {
    hash ^= data[i];
    hash = Math.imul(hash, 0x01000193); // FNV prime
  }
  return hash >>> 0; // ensure unsigned
}

function findSequence(
  haystack: Uint8Array,
  needle: Uint8Array,
  offset: number,
): number {
  const needleLen = needle.length;
  const limit = haystack.length - needleLen;
  for (let i = offset; i <= limit; i++) {
    if (haystack[i] === needle[0]) {
      let match = true;
      for (let j = 1; j < needleLen; j++) {
        if (haystack[i + j] !== needle[j]) {
          match = false;
          break;
        }
      }
      if (match) return i;
    }
  }
  return -1;
}

type FilterState = "passthrough" | "buffering";

export class SyncBlockFilter {
  private state: FilterState = "passthrough";
  private syncBuffer: Uint8Array = new Uint8Array(0);
  private pendingBytes: Uint8Array = new Uint8Array(0);
  private prevHash: number = 0;
  private prevLength: number = 0;
  private hasBaseline: boolean = false;

  // Metrics
  private _suppressed: number = 0;
  private _passed: number = 0;

  get suppressed(): number {
    return this._suppressed;
  }
  get passed(): number {
    return this._passed;
  }

  process(data: Uint8Array): Uint8Array {
    // Prepend any pending bytes from previous call
    let input: Uint8Array;
    if (this.pendingBytes.length > 0) {
      input = new Uint8Array(this.pendingBytes.length + data.length);
      input.set(this.pendingBytes);
      input.set(data, this.pendingBytes.length);
      this.pendingBytes = new Uint8Array(0);
    } else {
      input = data;
    }

    const chunks: Uint8Array[] = [];
    let pos = 0;

    while (pos < input.length) {
      if (this.state === "passthrough") {
        // Scan for BSU marker
        const bsuPos = findSequence(input, BSU, pos);

        if (bsuPos === -1) {
          // No BSU found — check for partial marker at end
          const trailingEsc = this.findTrailingEscStart(input, pos);
          if (trailingEsc >= 0) {
            // Output everything before the potential partial marker
            if (trailingEsc > pos) {
              chunks.push(input.slice(pos, trailingEsc));
            }
            this.pendingBytes = input.slice(trailingEsc);
            pos = input.length;
          } else {
            // Output everything
            if (pos < input.length) {
              chunks.push(input.slice(pos));
            }
            pos = input.length;
          }
        } else {
          // Output passthrough content before BSU
          if (bsuPos > pos) {
            chunks.push(input.slice(pos, bsuPos));
          }
          // Enter buffering state
          this.state = "buffering";
          this.syncBuffer = new Uint8Array(0);
          pos = bsuPos + BSU.length;
        }
      } else {
        // BUFFERING — looking for ESU marker
        const esuPos = findSequence(input, ESU, pos);

        if (esuPos === -1) {
          // No ESU found — accumulate remaining bytes
          const remaining = input.slice(pos);
          const newBuf = new Uint8Array(this.syncBuffer.length + remaining.length);
          newBuf.set(this.syncBuffer);
          newBuf.set(remaining, this.syncBuffer.length);
          this.syncBuffer = newBuf;
          pos = input.length;

          // Safety valve: if buffer exceeds limit, flush as passthrough
          if (this.syncBuffer.length > MAX_SYNC_BUFFER) {
            chunks.push(BSU);
            chunks.push(this.syncBuffer);
            this.syncBuffer = new Uint8Array(0);
            this.state = "passthrough";
          }
        } else {
          // Found ESU — complete sync block
          const blockEnd = input.slice(pos, esuPos);
          let fullBlock: Uint8Array;
          if (this.syncBuffer.length > 0) {
            fullBlock = new Uint8Array(this.syncBuffer.length + blockEnd.length);
            fullBlock.set(this.syncBuffer);
            fullBlock.set(blockEnd, this.syncBuffer.length);
          } else {
            fullBlock = blockEnd;
          }

          // Hash and compare
          const hash = fnv1aHash(fullBlock, 0, fullBlock.length);
          const len = fullBlock.length;

          if (this.hasBaseline && hash === this.prevHash && len === this.prevLength) {
            // Redundant sync block — suppress
            this._suppressed++;
          } else {
            // New content — pass through with markers
            chunks.push(BSU);
            chunks.push(fullBlock);
            chunks.push(ESU);
            this._passed++;
          }

          this.prevHash = hash;
          this.prevLength = len;
          this.hasBaseline = true;
          this.syncBuffer = new Uint8Array(0);
          this.state = "passthrough";
          pos = esuPos + ESU.length;
        }
      }
    }

    // Concatenate output chunks
    if (chunks.length === 0) return new Uint8Array(0);
    if (chunks.length === 1) return chunks[0];

    let totalLen = 0;
    for (const c of chunks) totalLen += c.length;
    const result = new Uint8Array(totalLen);
    let offset = 0;
    for (const c of chunks) {
      result.set(c, offset);
      offset += c.length;
    }
    return result;
  }

  // Check if the end of the buffer could be a partial BSU marker starting
  // with ESC (0x1B). Returns the index of the potential partial start, or -1.
  private findTrailingEscStart(data: Uint8Array, from: number): number {
    // Only need to check the last 7 bytes (BSU is 8 bytes, so a partial
    // match is at most 7 bytes)
    const checkFrom = Math.max(from, data.length - 7);
    for (let i = data.length - 1; i >= checkFrom; i--) {
      if (data[i] === 0x1b) {
        // Check if bytes from i match the start of BSU
        const remaining = data.length - i;
        let match = true;
        for (let j = 0; j < remaining; j++) {
          if (data[i + j] !== BSU[j]) {
            match = false;
            break;
          }
        }
        if (match) return i;
      }
    }
    return -1;
  }
}
