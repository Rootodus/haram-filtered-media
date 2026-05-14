# Rust Coding Style [NORMATIVE]
ID: STYLE-RUST  
Status: DRAFT  
Depends on: STD-DOC, ARCH-REQ

## Unsafe
- `unsafe` is PROHIBITED unless a benchmark proves a >15% performance gain.
- Every `unsafe` block MUST have a safety comment referencing which invariants are upheld.
- Prefer safe alternatives (`slice::get_unchecked` is NOT permitted without the above).

## Function Design
- Pass state explicitly (`&mut AppState`, `&Arc<ContentBuffer>`). Avoid global variables.
- Split functions when a single function has >6 parameters OR >40 lines.
- Splitting MUST NOT introduce heap allocations or clones in the hot path (Inference/Renderer).

## Abstractions
- Traits are PERMITTED only if they have a single implementation (no dynamic dispatch overhead).
- Generics are PERMITTED only for zero‑cost abstractions (e.g., `AsRef<[u8]>`).
- Do NOT use `Rc`; use `Arc` when shared ownership is required.

## Error Handling
- Use `anyhow` for binary errors (main). Use `thiserror` for library‑style modules.
- Hot path (Renderer, MLProcessor) MUST NOT allocate errors per frame; use `bool` or `Option` returns.

## Performance‑Sensitive Patterns
- Prefer `Vec::with_capacity` when size is known.
- Avoid `format!` and string operations in the render or inference loops.
- Use `crossbeam_channel` instead of `std::mpsc` for lower latency.
