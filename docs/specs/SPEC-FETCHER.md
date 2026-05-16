# Loader Orchestration (Fetcher)
ID: SPEC-FETCHER  
Status: STABLE-FOR-IMPLEMENTATION  
Depends on: @ARCH-REQ, @SPEC-ML-PROC, @STYLE-RUST

## 1. Process Management - FETCH-PROCESS
- The `Fetcher` module SHALL spawn the `Headless Chrome` binary as a child process.
- Flag Requirements: `--headless=new`, `--disable-gpu`, `--mute-audio`.
- The `Fetcher` SHALL maintain a persistent CDP (Chrome DevTools Protocol) session via `chromiumoxide`.

## 2. Injected Extractor - FETCH-JS-INJECT
- The `Fetcher` SHALL inject a JavaScript "Extractor" script into every navigated page.
- Responsibility: The script walks the DOM, builds a FlatBuffer (Metadata table including DOM nodes with absolute pixel coordinates), captures a `Page.captureScreenshot` in raw `RGBA`, and writes the 2-part payload to the IPC socket.
- DOM coordinates: For each element, `getBoundingClientRect()` provides absolute pixel `x`, `y`, `width`, `height` relative to viewport. These values SHALL populate the `Rect` struct in the FlatBuffer.
- Optional text: For each DOM node, set `has_text` to true if the node contains non‑empty text content; otherwise false. The `text` field may be an empty string when `has_text` is false.

## 3. IPC Handshake - FETCH-HANDSHAKE
- The `Fetcher` SHALL implement the client-side of the FlatBuffers wire protocol defined in @SPEC-ML-PROC::PROTOCOL-SPIKE.
- Sequence:
  1. Build the FlatBuffer containing the `Metadata` table (timestamp, width, height, nodes).
  2. Write `[FB_Length: u32]` (Little‑Endian byte length of the FlatBuffer).
  3. Write `[FlatBuffer_Payload: bytes]`.
  4. Write `[Raw_Pixels: bytes]` (RGBA8 bitstream from screenshot).
  5. Wait for `0x01` ACK byte from the `Runtime`.
- Constraint: The `Loader` MUST NOT send a new snapshot until the ACK is received.
- Byte Order: All length prefixes and numerical values in the FlatBuffer SHALL be Little‑Endian.

## 4. Operational Invariants
- Navigation Gating: The `Fetcher` SHALL NOT initiate a new snapshot until the `Runtime` has acknowledged the previous one (ACK `0x01`) to prevent buffer bloat.
- Error Handling: If the Chrome process exits unexpectedly, the `Fetcher` SHALL attempt a single restart before propagating a `FATAL` error to the `Monolith`.
