# Loader Orchestration (Fetcher)
ID: SPEC-FETCHER  
Status: STABLE-FOR-IMPLEMENTATION  
Depends on: @ARCH-REQ, @SPEC-ML-PROC, @STYLE-RUST

## Process Management - FETCH-PROCESS
- The `Fetcher` module SHALL spawn a `Headless Chrome` binary as a child process.
- Flag Requirements: `--headless=new`, `--disable-gpu`, `--mute-audio`.
- The `Fetcher` SHALL maintain a persistent CDP (Chrome DevTools Protocol) session via a suitable client.
- [EXPLANATORY] Current implementation uses Node.js + Puppeteer for rapid prototyping. Production target is Rust + `chromiumoxide`.

## DOM Extraction - FETCH-EXTRACT
- The `Fetcher` SHALL inject an extraction script into every navigated page.
- The script SHALL use `document.querySelectorAll` with a user‑provided CSS selector (from a manifest) to select DOM nodes.
- For each selected node, the script SHALL capture:
  - `tag`: element tag name (lowercase).
  - `has_text`: boolean indicating non‑empty trimmed text content.
  - `text`: the trimmed text content, truncated to a maximum of 500 characters.
  - `rect`: absolute pixel coordinates from `getBoundingClientRect()`: `x`, `y`, `width`, `height`.
- The extracted nodes SHALL be returned to the main loader process.

## FlatBuffer Construction - FETCH-BUILDER
- The loader SHALL construct a FlatBuffer `Metadata` table containing:
  - `timestamp`: current time in milliseconds.
  - `width`, `height`: viewport dimensions (from page screenshot).
  - `nodes`: vector of `DomNode` tables.
- The `Rect` structure SHALL be a FlatBuffer `struct` (or `table` if limited by JS/TS generator).
- After building, the loader SHALL obtain a raw RGBA8 screenshot via `Page.captureScreenshot`.

## IPC Handshake - FETCH-HANDSHAKE
- Wire framing SHALL follow @SPEC-ML-PROC::PROTOCOL-SPIKE:
  1. Write `[FB_Length: u32]` (Little‑Endian).
  2. Write `[FlatBuffer_Payload: bytes]`.
  3. Write `[Raw_Pixel_Bytes: bytes]` (RGBA8).
  4. Wait for `0x01` ACK byte from the Runtime.
- The loader MUST NOT send a new snapshot until the ACK is received (hard‑sync backpressure).
- Byte Order: Little‑Endian for all length prefixes and numerical values.

## Operational Invariants
- Navigation Gating: New snapshot only after ACK from previous.
- Error Handling: If Chrome process exits unexpectedly, attempt a single restart before propagating `FATAL`.
- Snapshot Trigger: As defined in @ARCH-REQ::SNAPSHOT-TRIGGER (navigation complete, DOM idle >200 ms, or user interaction). For spikes, a single snapshot on page load suffices.

## Temporary Implementation Notes
- Current loader is Node.js + Puppeteer. This will be replaced by a Rust binary using `chromiumoxide` in a future spike.
- The extraction script is currently hardcoded with a selector; production will read selectors from a user manifest.
- `Rect` is a `table` in the current schema due to JS/TS generation limitations; revert to `struct` when generator supports it.
