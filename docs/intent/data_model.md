# Data Model
ID: DAT-MOD  
Status: PRELIMINARY  
Depends on: DOC-STD

## Core Type
Type: `ContentBuffer`  
Description: container for all pipeline data  
Fields:
- `content_type`: enum (`Image`, `VideoFrame`, `AudioChunk`, `Text`)
- `source_id`: string
- `timestamp_ms`: integer, optional
- `payload`: binary or structured data
- `metadata`: map<string, string>, optional

## Content Types
Type: `Image`  
Fields:
- `width`: integer [pixels]
- `height`: integer [pixels]
- `channels`: integer [3 OR 4]
- `data`: byte array OR GPU buffer reference
- `format`: enum (`PNG`, `JPEG`, `WEBP`)

Type: `VideoFrame`  
Fields:
- `width`: integer [pixels]
- `height`: integer [pixels]
- `channels`: integer [3 OR 4]
- `timestamp_ms`: integer
- `data`: byte array OR GPU buffer reference
- `format`: enum (`RGB`, `YUV`)

Type: `AudioChunk`  
Fields:
- `sample_rate_hz`: integer
- `channels`: integer [1 OR 2]
- `timestamp_ms`: integer
- `duration_ms`: integer
- `data`: PCM byte array OR GPU buffer reference

Type: `Text`  
Fields:
- `content`: UTF-8 string
- `language`: string, optional
- `timestamp_ms`: integer, optional

## Auxiliary Types
Type: `URL`  
Fields:
- `value`: string
- `protocol`: enum (`HTTP`, `HTTPS`)

Type: `Metadata`  
Fields:
- `key`: string
- `value`: string OR number

## Ownership Rules
Rule: `ContentBuffer` payload SHOULD use shared reference where possible  
Rule: copying large buffers is PROHIBITED unless REQUIRED  
Rule: GPU buffers MUST be released after processing  
Rule: MLProcessor MAY modify payload in-place

## Constraints
ContentType: MUST match actual payload type  
Timestamp: MUST be monotonic within stream  
Format: MUST be explicitly defined for binary data

## Notes / Explanatory
- [EXPLANATORY] Shared references reduce memory overhead in high-throughput pipelines.
- [EXPLANATORY] Explicit formats prevent decoding ambiguity across stages.
