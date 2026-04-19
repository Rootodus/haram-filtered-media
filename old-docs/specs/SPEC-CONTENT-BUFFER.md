# Content Buffer Specification
ID: SPEC-CONTENT-BUFFER  
Status: STABLE  
Depends on: STD-DOC, ARCH-GLOSSARY

## Core Type Definition

### `ContentBuffer` Struct
Fields:
- `content_type`: Enum (Image, VideoFrame, AudioChunk, Text).
- `source_id`: String.
- `timestamp_ms`: Integer [Monotonic].
- `payload`: Byte array OR GPU buffer reference.
- `metadata`: Map<String, String>.

## Payload Constraints

### Data Integrity
Constraint:
- `content_type` MUST match the actual binary structure of the `payload`.
- `timestamp_ms` MUST be monotonic within a single content stream.
- Binary payloads MUST have an explicitly defined format in `metadata`.

Rationale:
- Explicit formatting AND type-safety prevent decoding ambiguity across pipeline stages.

## Communication Primitives

### `PipelineMessage` [Enum]
Description: The top-level unit of exchange between all pipeline stages.  
Variants:
- `DATA(ContentBuffer)`: Carries a standard processing unit.
- `SIGNAL(PipelineSignal)`: Carries system-level control instructions.

### `PipelineSignal` [Enum]
Description: Explicit control signals for pipeline lifecycle management.  
Variants:
- `SHUTDOWN`: Indicates the end of the stream. Stages MUST cease processing AND propagate this signal immediately.

## Memory AND Resource Management

### Shared Ownership
Constraint:
- Implementation MUST prefer shared ownership references for large `payload` data.
- Deep copying of `ContentBuffer` is PROHIBITED unless required for cross-thread isolation.

Rationale:
- Shared references reduce memory allocation overhead AND improve throughput in high-volume streaming.

### GPU Resource Lifecycle
Constraint:
- GPU buffer references MUST be explicitly released after the Renderer stage returns.
- MLProcessor is PERMITTED to modify `payload` data in-place IF ownership is exclusive.

Rationale:
- Manual release prevents resource leaks in GPU-accelerated processing paths.
- In-place modification reduces the need for intermediate buffer allocations.

## Auxiliary Types

### `URL`
Fields:
- `value`: String.
- `protocol`: Enum (HTTP, HTTPS).

### `Metadata`
Fields:
- `key`: String.
- `value`: String OR Number.

## Metadata Registry
To ensure semantic compatibility across stages, the following reserved keys MUST be used in the `metadata` map.

### Reserved Keys
| Key | Value Type | Description |
| --- | --- | --- |
| `status` | Enum | [SUCCESS, FAIL] Current state of the UnitOfWork. |
| `error_code` | Integer | System OR HTTP error code. |
| `error_msg` | String | Descriptive error text for logging. |
| `trace_id` | String | Unique UUID for tracking a UnitOfWork through the pipeline. |
| `source_type` | Enum | [Static, Dynamic] Indicates if content originated from Fetcher OR Loader. |
| `payload_format` | Enum | [PNG, JPEG, WEBP, RGB, YUV, PCM, UTF8]. |
| `latency_marker` | Integer | Epoch timestamp in MS for specific stage entry. |

### Usage Rules
Constraint:
- Stages MUST NOT use custom keys for data already covered by Reserved Keys.
- `Fetcher` AND `Loader` MUST initialize `trace_id` AND `status`.
- `MLProcessor` MUST update `payload_format` IF the payload structure changes.
- `Renderer` MUST read `status` before attempting serialization.

Rationale:
- Standardizing keys prevents stage-to-stage communication failure AND enables consistent logging in `SPEC-MAIN`.

## Error State and Failure Semantics

### Fail Status Handling
Constraint:
- IF `PipelineMessage` is `DATA(ContentBuffer)` AND `ContentBuffer.metadata.status` is `FAIL`, THEN:
  1. The `MLProcessor` MUST NOT attempt any transformation of the `payload`.
  2. The `MLProcessor` MUST immediately pass the message downstream.
  3. The `Renderer` MUST log the failure AND signal resource release.

### Payload State on Failure
Constraint:
- IF `status` is `FAIL`, THEN the `payload` MUST be an empty byte array.
- The `error_msg` in `metadata` MUST contain the reason for failure.

### Signal Priority
Constraint:
- IF `PipelineMessage` is `SIGNAL(SHUTDOWN)`, THEN the stage MUST NOT inspect any `ContentBuffer` metadata AND MUST pass the signal downstream immediately.

Rationale:
- Centralizing failure logic prevents "Semantic Mismatch" where different stages handle errors inconsistently.

## Interface Anchor Rule
Constraint:
- Stage-specific specifications (`SPEC-FETCHER`, etc.) MUST NOT redefine OR override `ContentBuffer` fields.
- Generated code MUST use the exact field names AND types defined in this document.
- ANY attempt by the AI to "optimize" the buffer structure locally MUST be rejected during the prompt-evaluation phase.

Rationale:
- Centralizing the buffer logic prevents semantic fragmentation where different stages assume different data layouts for the same `trace_id`.

## Notes / Explanatory
- [EXPLANATORY] The AI MUST use exact field names as defined in this specification.
- [EXPLANATORY] This document provides the machine-executable definition for the primary data container.
