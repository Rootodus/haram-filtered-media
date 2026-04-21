# MLProcessor Data Contract
ID: SPEC-ML-PROC  
Status: DRAFT  
Depends on: ARCH-REQ, ARCH-SYS-MAP

## Purpose
Defines the binary schema for data moving across the `IPC-MSGPACK` pipe and through the monolithic pipeline.

## 1. ContentBuffer Schema [SCHEMA-BUFFER]
The `ContentBuffer` is the primary unit of processing. It must be serialized by the `Loader` and deserialized by the `Parser`.

| Field | Type | Description |
| --- | --- | --- |
| `document_url` | String | Source URL for `MODEL-ROUTING`. |
| `timestamp` | U64 | Acquisition time in epoch milliseconds. |
| `elements` | List<Node> | The subset of DOM nodes selected by the user. |
| `viewport` | Rect | The dimensions of the active render area. |

### 1.1 Node Structure
| Field | Type | Description |
| --- | --- | --- |
| `id` | String | Unique DOM identifier. |
| `tag` | String | HTML tag name (e.g., "DIV", "VIDEO"). |
| `text_content` | Optional<String> | Raw text within the element. |
| `computed_styles` | Map<String, String> | Map of CSS properties to absolute values. |
| `bounding_box` | Rect | Viewport-relative coordinates (x, y, w, h). |

## 2. ProcessedBuffer Schema [SCHEMA-OUTPUT]
The output produced by the `MLProcessor` to be consumed by the `Renderer`.

| Field | Type | Description |
| --- | --- | --- |
| `model_id` | String | Identifier of the model that produced the result. |
| `instructions` | List<Action> | List of operations for the `Renderer`. |

### 2.1 Action Structure
| Type | Parameters | Target |
| --- | --- | --- |
| `VISUAL_MASK` | `type` (Blur/Black), `alpha` | `Rect` coordinates |
| `AUDIO_MASK` | `type` (Mute/Beep), `volume` | `TemporalSegment` (ms) |
| `TEXT_REPLACE` | `new_text` | `Node_ID` |

## 3. Admission Policy [ADMISSION-LOGIC]
- Mechanism: A `crossbeam-channel` with `capacity(1)`.
- Logic:
  - `Sender` (Parser) uses `try_send()`.
  - IF `Full` -> `Receiver` (Inference) is busy -> `Sender` drops the previous item and sends the new `Arc<ContentBuffer>`.
- Reason: To satisfy `MODE-LATENCY` by ensuring the most recent data always has priority.
