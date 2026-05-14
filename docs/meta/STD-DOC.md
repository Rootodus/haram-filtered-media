# Documentation Standard [NORMATIVE]
ID: STD-DOC  
Status: STABLE  
Depends on: NONE

## Purpose
- Define requirements for token-efficient, logically dense, AND structurally consistent documentation.
- Ensure documents are machine-readable by LLMs while remaining human-traceable.

## Syntax & Naming Conventions
| Category | Casing | Example |
| --- | --- | --- |
| System Entities | PascalCase | Fetcher, MLProcessor, Renderer, Loader |
| System States | PascalCase | PipelineActive, Processing, Completed |
| Types / Structs | Backticks + PascalCase | `ContentBuffer`, `AudioChunk` |
| Variables / Fields | Backticks + snake_case | `queue_size`, `timestamp_ms` |
| Constants | Backticks + SCREAMING_SNAKE_CASE | `MAX_QUEUE_SIZE` |
| File Paths | Backticks + native syntax | `docs/architecture.md`, `src/main.rs` |
| Concept Properties | lowercase | status, mode, type, segment |
| Document Keys | Sentence case | Status, Project, Depends on, Input, Output |

- Initialism/acronym exception: Initialisms AND acronyms (ID, URL, ML, GPU, JS, HTTP) MUST remain ALL CAPS regardless of category.
- Casing hierarchy: Initialism/acronym exception > System Entity syntax > Document Key (Sentence case).
- Key-value mapping: Text appearing before a colon (:) MUST follow the Casing hierarchy. Text appearing after a colon follows its categorical casing (Example: `Component: Fetcher` OR `Fetcher: Active` OR `ID: 0001`).
- Context precedence: IF a token refers to functional behavior, THEN use System Entity syntax. IF a token refers to code implementation, THEN use Type syntax.
- Collision rule: IF a statement describes both behavior AND implementation, THEN Type syntax MUST take precedence.
- Identifier parity: Documentation MUST use exact source code naming for all types, variables, AND constants.
- Markdown tokens: Syntax characters (---, |, >, #) are exempt from casing rules.

## Structural Geometry
- NO emphasis: NO bold, NO italics. Casing is the ONLY emphasis.
- Fenced blocks: Use backticks for all technical snippets, variables, types, AND file paths.
- Bracketing: Use square brackets [] primarily for References `[Re: ID]` AND Metadata tags.
- Parentheses: Use parentheses () for asides, context, and logical grouping.
- Horizontal rules: Horizontal rules (---) are PROHIBITED EXCEPT as Markdown table delimiters.
- List markers: Unordered lists MUST use hyphen (-) markers. Ordered lists MUST use number AND dot (1.) markers.
- Indentation: Use exactly TWO spaces per level for sub-items AND list continuations.
- Vertical spacing: Exactly ONE newline between logical blocks. A sequence of single-spaced key/value lines is treated as ONE logical block.

## English Reduction (Dense Logic Prose)
- STE usage: Use Simplified Technical English. Narrative fluff is PROHIBITED.
- Active voice: Subject-Verb-Object REQUIRED.
- Atomic logic: EACH line MUST convey exactly ONE fact OR constraint. This applies to `IF/THEN` actions AND key-value assignments.
- Explicit operators: Implicit logic is PROHIBITED. Lists AND definitions MUST use explicit operators (AND, OR) to define relationships.
- Logical flattening:
  - `IF/THEN` is the outermost operator.
  - EACH `IF/THEN` line MUST contain exactly ONE outcome (action).
  - Compound conditions using `AND` are PERMITTED within a single `IF` segment. Conditions using `OR` MUST be split into multiple lines.
- Contract syntax: Preconditions AND postconditions MUST be written as boolean predicates using System Entities, System States, OR Variables.

## Banned Words [PROHIBITED to use]
- `philosophy`: BANNED
- `philosophical`: BANNED
- `determinism`: BANNED
- `deterministic`: BANNED
- `magic`: BANNED
- `holy`: BANNED
- `god`: BANNED

## Metadata & Traceability
- Placement: Metadata MUST be placed at the top of the document immediately following the H1 header.
- Required fields: ID, Status, Depends on.
- Metadata key-names: MUST follow the Section 2 Casing hierarchy.
- Document changes: Reference specific Section numbers OR Logic Identifiers.
- Circularity: Circular dependencies are PROHIBITED.

## Tables & Lists
- Data mapping: Use single-spaced key/value lines WITHOUT bullets for metadata OR static system properties (Example: `Mode: read-only`).
- Instructional lists: Use hyphen (-) markers ONLY for constraints, requirements, OR rationale.
- Tables: Maintain column consistency. Use exactly ONE space between pipes AND content `| Content |`.

## Units
- A space SHALL separate a numeric value from its unit: `10 ms`, `5 MB`, `11 ms`.
- Exceptions (no space): percent (`10%`), degree (`90°`).

## Document ID System
- Document IDs MUST follow format: `CLASS-IDENTIFIER`
- Each ID MUST start with a valid Category Class token.
- `IDENTIFIER` MUST be a unique semantic name using ALL CAPS and hyphens.
- IDs MUST be stable and globally unique within the system.

### Category Classes
- ARCH = high-level reasoning, system maps, and terminology
- SPEC = atomic component instructions, interfaces, and constraints
- LOG = append-only journals of historical decisions and rationale
- EXP = structured execution records, benchmarks, and evidence
- STD = documentation syntax, formatting, and governance standards
- STYLE = coding conventions, language‑specific rules, and performance patterns

### Construction Rules
- IDs MUST use exactly one Class token followed by a hyphen and a semantic name.
- Class tokens MUST NOT be modified or extended.
- Semantic names MUST be concise and descriptive of the component or concept.

### Uniqueness Rule
- Each document ID MUST be globally unique at the full string level.

## Anchor & Reference System

### Anchor Definitions
Definition Bullets (FACTS, DECISIONS, GAPS, or any block marked `[DEFINITIONS]`):
- EVERY bullet MUST begin with an implicit anchor `ID:` at line start.
- `ID` ALWAYS in `UPPER-KEBAB-CASE` format.
- Example: `ENV-EXT-LATENCY: Text.`

Ordinary Bullets (other normative lists, e.g., implementation steps):
- No implicit anchor required.
- If a bullet needs to be referenceable, add an explicit anchor `[ID]` at the end of the line.
- Example: ``- Use `wgpu` for rendering. [WGPU-USE]``

Non‑Normative Bullets (e.g., within `[EXPLANATORY]` blocks):
- No anchors.

Explicit Anchors (headers, paragraphs, tables, code blocks):
- `[ID]` immediately after the element.
- Header: `## Title [ID]`
- Paragraph/table/code block: `[ID]` on its own line or at end.

Uniqueness:
- `ID` MUST be unique within a document.
- Different documents MAY reuse the same `ID`.

### References
A reference is any `ID` or `DOC-ID::ID` not in a definition position (not after `:` at bullet start, not inside `[ID]`).

- Same document: `ID`
- Cross document: `DOC-ID::ID`
- No backticks required.

Context shortening:
- `[Using: DOC-ID::*]` at section/file start.
- After that, `ID` expands to `DOC-ID::ID`.

### Scope
- Anchor scope is single document.
- Global identification uses `DOC-ID` + `ID`.
- Headers are not stable identifiers. Do not reference them.

## Notes / Explanatory
- `[EXPLANATORY]` tags denote rationale OR non-binding meta-information.
- Prescriptive assumption: IF the H1 header is tagged `[NORMATIVE]`, THEN ALL statements in the document are prescriptive EXCEPT those tagged `[EXPLANATORY]`.
- Prototyping relaxation: STE simplification may allow short explanatory sentences ONLY within this Notes section.
