# DOM to Tensor Parser
ID: SPEC-PARSER-DOM  
Status: STABLE-FOR-IMPLEMENTATION  
Depends on: @ARCH-REQ, @SPEC-ML-PROC, @STD-DOC

## Purpose
- Define the normative algorithm for converting user‑selected DOM nodes (from `Metadata.nodes`) into a fixed‑width 2D tensor suitable for ONNX model input.
- Resolve the ARCH-REQ::GAPS::DOM-MAPPING ambiguity.
- This specification applies only to `ContentBuffer` instances containing DOM snapshots. For other media types (video frames, static images, audio data), the system uses direct pixel/stream manipulation without tensor conversion (see @ARCH-SYS-MAP::STREAM-LAYER).

## Inputs
- `nodes`: Vector of `DomNode` objects as defined in @SPEC-ML-PROC::SCHEMA-BUFFER-SPIKE.
- `viewport_width`, `viewport_height`: Provided by `Metadata.width` and `Metadata.height` (absolute pixel dimensions).
- `max_nodes`: Integer (e.g., 256) specified in the model manifest (@ARCH-REQ::PLUGIN-DECLARATIVE).
- `feature_dim`: Integer (e.g., 128) specified in the model manifest.

## Output
- `InferenceTensor`: Contiguous `Arc<[f32]>` of length `max_nodes * feature_dim`, laid out in row‑major order (node index `i`, feature index `j` at position `i * feature_dim + j`).

## Algorithm

### Ordering - PARSER-ORDER
- Default ordering: Depth‑first pre‑order traversal of the DOM tree as reported by the loader (order of nodes in `Metadata.nodes`).
- User manifest MAY override with an alternative ordering:
  - `"order": "area-descending"` – sort by `rect.width * rect.height` descending.
  - `"order": "document"` – explicit document order (default).

### Truncation and Padding - PARSER-SIZE
- If `len(nodes) > max_nodes`: Keep the first `max_nodes` nodes in the chosen order. Discard remaining.
- If `len(nodes) < max_nodes`: Pad with sentinel nodes at the end.
- Sentinel node fields:
  - `tag = ""`
  - `has_text = false`
  - `text = ""`
  - `rect = { x = 0.0, y = 0.0, width = 0.0, height = 0.0 }`

### Feature Extraction per Node - PARSER-FEATURES
Each node SHALL be converted to a 1D vector of length `feature_dim` by concatenating the following features (in order):

| Feature | Source | Normalization | Length |
| --- | --- | --- | --- |
| Tag embedding | One‑hot of common tags: `div`, `p`, `span`, `a`, `img`, `button`, `input`, `li`, `ul`, `ol`, `h1`, `h2`, `h3`, `h4`, `h5`, `h6`, `section`, `article`, `nav`, `header`, `footer`. Unknown tags map to a zero vector. | None | 20 |
| Text presence | `1.0` if `has_text` is true, else `0.0` | None | 1 |
| Text embedding | Fixed sentence transformer (e.g., `all-MiniLM-L6-v2`) output truncated to 384 dimensions. If no text, zero vector. | None | 384 |
| Bounding rect | `x`, `y`, `width`, `height` | Each divided by `viewport_width` or `viewport_height` (range 0–1) | 4 |
| Area | `width * height` | Divided by `viewport_width * viewport_height` (range 0–1) | 1 |
| Total default length |  |  | 20 + 1 + 384 + 4 + 1 = 410 |

If `feature_dim` is less than the default length, truncate the vector (drop trailing features). If greater, pad with zeros.

### Tensor Assembly - PARSER-TENSOR
- Allocate `Vec<f32>` with capacity `max_nodes * feature_dim`, initialised to zero.
- For each node index `i` (0‑based) up to `min(max_nodes, len(nodes))`:
  - Compute feature vector `fv` of length `feature_dim` using `extract_node_features(node, feature_dim, viewport_width, viewport_height)`.
  - Copy `fv` into `tensor[i * feature_dim .. i * feature_dim + feature_dim]`.
- Wrap the `Vec<f32>` in `Arc<[f32]>` for zero‑copy handoff.

## Implementation Constraints
- The parser SHALL NOT allocate per‑node temporary vectors in the hot path if performance requires reuse. Pre‑allocated scratch space is permitted.
- The text embedding step is computationally expensive. For spikes, a placeholder embedding (e.g., random or constant vector) MAY be used until a real embedding model is integrated.
- The parser SHALL respect @ARCH-REQ::GPU-PRIORITY if it performs GPU work; otherwise it runs on CPU.

## Integration with Pipeline
- The `Parser` stage (as defined in @ARCH-SYS-MAP::STAGE-TRANSFORM) SHALL implement this specification.
- Output `InferenceTensor` is passed to `MLProcessor` via `Arc<[f32]>`.

## Notes / Explanatory
- [EXPLANATORY] The default embedding dimension 384 matches `all-MiniLM-L6-v2`. A future spike may benchmark alternative embedding models.
- [EXPLANATORY] The one‑hot tag set covers approximately 90% of elements on typical web pages. The list can be extended via configuration.
- [GAP] The exact mechanism for loading and executing the sentence transformer is deferred to `SPEC-ML-CORE`.
- [EXPLANATORY] Video, image, and audio processing do not require tensor conversion; they are handled by the Stream Layer via coordinate masks and temporal segments. Therefore this document does not define parsing for those media.
