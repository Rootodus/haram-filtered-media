# System Glossary
ID: ARCH-GLOSSARY  
Status: STABLE  
Depends on: STD-DOC

## Core Data Container

### `ContentBuffer`
Description: The primary data structure passing through the pipeline.  
Fields:
- `content_type`: `ContentType` [Enum].
- `source_id`: String identifier for the content origin.
- `timestamp_ms`: Monotonic integer representing ingestion time.
- `payload`: Binary OR structured data payload.
- `metadata`: `MetadataMap` [Registry-backed Map].

## Interface Registries [NORMATIVE]

### `ContentType` [Enum Variants]
- `IMAGE`: Static visual data.
- `VIDEO_FRAME`: Single temporal visual frame.
- `AUDIO_CHUNK`: Segment of continuous audio.
- `TEXT`: UTF-8 encoded string.

### `PayloadFormat` [Enum Variants]
- `PNG`: Portable Network Graphics.
- `JPEG`: Joint Photographic Experts Group.
- `WEBP`: Web Picture format.
- `RGB`: Raw Red-Green-Blue pixels.
- `YUV`: Raw Luma-Chroma pixels.
- `PCM`: Pulse Code Modulation audio.
- `UTF8`: Standard text encoding.

### `UnitStatus` [Enum Variants]
- `SUCCESS`: Item processed without terminal errors.
- `FAIL`: Item encountered error AND transformation is bypassed.

### `SourceType` [Enum Variants]
- `STATIC`: Content retrieved via `Fetcher`.
- `DYNAMIC`: Content retrieved via `Loader`.

## Metadata Key Registry [NORMATIVE]
EACH key used in the `ContentBuffer` metadata map MUST be defined here.

| Key | Value Type | Description |
| --- | --- | --- |
| `status` | `UnitStatus` | Current processing state of the unit. |
| `error_code` | Integer | Numeric system OR HTTP error code. |
| `error_msg` | String | Descriptive error text. |
| `trace_id` | String | Unique UUID for life-cycle tracking. |
| `source_type` | `SourceType` | Origin classification of the content. |
| `payload_format` | `PayloadFormat` | Specific encoding of the binary payload. |
| `latency_marker` | Integer | Epoch timestamp [MS] for stage entry/exit. |

## Content Classifications

### `Image`
Represents static visual data.  
Fields: `width`, `height`, `channels`, `data`, `format` [`PayloadFormat`].

### `VideoFrame`
Represents a single frame in a visual stream.  
Fields: `width`, `height`, `channels`, `timestamp_ms`, `data`, `format` [`PayloadFormat`].

### `AudioChunk`
Represents a segment of an audio stream.  
Fields: `sample_rate_hz`, `channels`, `timestamp_ms`, `duration_ms`, `data`.

### `Text`
Represents UTF-8 encoded string data.  
Fields: `content`, `language`, `timestamp_ms`.

## Auxiliary Entities

### `URL`
Represents a web resource location.  
Fields: `value`, `protocol`.  
Supported protocols: `HTTP`, `HTTPS`.

### `Metadata`
Represents arbitrary stage-specific information.  
Format: `key` [String] AND `value` [String OR Number].

## Memory Ownership Principles
Shared references are REQUIRED for large payloads to prevent copying.  
`GPU` buffers MUST be explicitly released after Renderer completion.  
`MLProcessor` is PERMITTED to modify `ContentBuffer` payloads in-place.

## Notes / Explanatory
- [EXPLANATORY] Standardizing enum variants AND metadata keys prevents stage-to-stage communication failure.
- [EXPLANATORY] AI generation MUST use the exact string representations of keys AND enum variants defined in the Registries.
