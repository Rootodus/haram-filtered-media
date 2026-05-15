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
- Responsibility: The script walks the DOM, generates a `FlatBuffer` (Metadata + Nodes), captures a `Page.captureScreenshot` in raw `RGBA`, and writes the 3-part payload to the IPC socket.

## 3. IPC Handshake - FETCH-HANDSHAKE
- The `Fetcher` SHALL implement the client-side of the @SPEC-ML-PROC::PROTOCOL-SPIKE.
- Sequence:
  1. Write `[Meta_FB_Length: u32]`.
  2. Write `[Meta_FB_Bytes]`.
  3. Write `[Pixels: Raw]`.
  4. Wait for `0x01` ACK from the `Runtime`.

## 4. Operational Invariants
- Navigation Gating: The `Fetcher` SHALL NOT initiate a new snapshot until the `Runtime` has acknowledged the previous one to prevent buffer bloat.
- Error Handling: If the Chrome process exits unexpectedly, the `Fetcher` SHALL attempt a single restart before propagating a `FATAL` error to the `Monolith`.
