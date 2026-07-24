# Experiment: Real DOM Extraction
ID: EXP-SPIKE-07-REAL-DOM-EXTRACTION  
Status: SUCCESS  
Depends on: @STD-DOC, @EXP-RULES, @EXP-SPIKE-06-FLATBUFFERS-BRIDGE

## Hypothesis
A Puppeteer script that walks the real DOM using `document.querySelectorAll` and returns node data (tag, text, absolute bounding rectangle) to Node.js for FlatBuffer construction can produce a valid snapshot compatible with the Rust runtime, with Rust verification time remaining independent of node count and below 10 µs for typical pages.

## Evidence

### Environment
- Hardware: Intel Iris Xe (Vulkan), Windows 11.
- Software: Rust runtime (tokio, wgpu, winit), Node.js loader (Puppeteer, flatbuffers).
- Target URL: `https://en.wikipedia.org/wiki/HTML5`
- Selector: `"p"` (all paragraph elements).
- Viewport: 1920x1080.
- Snapshot count: 1 (single extraction per run).

### Quantitative Data
- DOM nodes extracted: 51.
- FlatBuffer payload size: 17.46 KB (17,880 bytes).
- Rust verification time (`root_unchecked`): 2.1 µs (mean over 1 sample).
- Node length access time: 900 ns.
- ACK received: Yes.
- No errors or disconnections.

### Code Snippet (Node.js extraction)
```typescript
async function extractDomNodes(page: puppeteer.Page, selector: string): Promise<DomNodeData[]> {
    return await page.evaluate((sel) => {
        const elements = document.querySelectorAll(sel);
        const nodes = [];
        let id = 0;
        for (const el of elements) {
            const rect = el.getBoundingClientRect();
            const text = el.textContent?.trim() ?? null;
            nodes.push({
                id: id++,
                tag: el.tagName.toLowerCase(),
                has_text: text !== null && text.length > 0,
                text: text ? text.slice(0, 500) : null,
                rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
            });
        }
        return nodes;
    }, selector);
}
```

Note: Actual implementation includes debug logging and sample collection.

## Analysis
- The extraction method correctly identifies 51 `<p>` elements on the Wikipedia HTML5 page.
- Each node provides absolute pixel coordinates via `getBoundingClientRect()` and truncated text.
- The FlatBuffer built in Node matches the schema (Metadata containing DomNode vector).
- Rust verification time (2.1 µs) is well below the 10 µs target and independent of node count (consistent with Spike‑06 results for 5000 nodes, which also showed 1–3 µs).
- The single snapshot was acknowledged, confirming wire protocol compatibility.
- End‑to‑end latency was not measured; this spike focused on functional correctness and Rust verification overhead. Performance measurement is deferred to a dedicated spike (Spike‑08).

## Conclusion
Real DOM extraction via Puppeteer `evaluate` is validated. The extracted data can be serialized to FlatBuffer and processed by the Rust runtime with negligible verification cost. No architectural gaps are introduced.

### Triggered Decisions
- Adopt `page.evaluate` with `querySelectorAll` as the extraction mechanism for the loader (to be formalized in `SPEC-FETCHER.md` after further performance characterization).
- Truncate text content to 500 characters to limit payload size (configurable in future).
- Use `getBoundingClientRect()` to supply absolute pixel coordinates for each node, resolving the coordinate ambiguity noted in ARCH-REQ::GAPS::DOM-MAPPING.

### Follow-up Items
- Performance measurement (extraction time, builder time, end‑to‑end latency over continuous snapshots).
- Formalize selector passing via manifest (currently hardcoded for spike).
- Add support for multiple selectors and regex text filtering (future spikes).
