# Documentation Standard [NORMATIVE]
Project: MLFilteredBrowser (MLFB)  
ID: STD-DOC  
Status: STABLE  
Depends on: NONE

## Purpose
- Define requirements for token-efficient, logically dense, AND structurally consistent documentation.
- Ensure documents are machine-readable by LLMs while remaining human-traceable.

## Syntax & Naming Conventions
| Category | Casing | Example |
| --- | --- | --- |
| Normative Verbs | ALL CAPS | MUST, SHALL, REQUIRED, PROHIBITED, PERMITTED |
| Logical Operators | ALL CAPS | IF, THEN, AND, OR, XOR, NOT |
| Quantifiers | ALL CAPS | ONLY, ALL, ANY, EACH, NONE, UNIQUE |
| System Entities | PascalCase | Fetcher, MLProcessor, Renderer, Loader |
| System States | PascalCase | PipelineActive, Processing, Completed |
| Types / Structs | Backticks + PascalCase | `ContentBuffer`, `AudioChunk` |
| Variables / Fields | Backticks + snake_case | `queue_size`, `timestamp_ms` |
| Constants | Backticks + SCREAMING_SNAKE_CASE | `MAX_QUEUE_SIZE` |
| File Paths | Backticks + native syntax | `docs/architecture.md`, `src/main.rs` |
| Concept Properties | lowercase | status, mode, type, segment |
| Document Keys | Sentence case | Status, Project, Depends on, Input, Output |

- Initialism/acronym exception: Initialisms AND acronyms [ID, URL, ML, GPU, JS, HTTP] MUST remain ALL CAPS regardless of category.
- Casing hierarchy: Initialism/acronym exception > System Entity syntax > Document Key [Sentence case].
- Key-value mapping: Text appearing before a colon [:] MUST follow the Casing hierarchy. Text appearing after a colon follows its categorical casing [Example: `Component: Fetcher` OR `Fetcher: Active` OR `ID: 0001`].
- Context precedence: IF a token refers to functional behavior, THEN use System Entity syntax. IF a token refers to code implementation, THEN use Type syntax.
- Collision rule: IF a statement describes both behavior AND implementation, THEN Type syntax MUST take precedence.
- Identifier parity: Documentation MUST use exact source code naming for all types, variables, AND constants.
- Markdown tokens: Syntax characters [---, |, >, #] are exempt from casing rules.

## Structural Geometry
- NO emphasis: NO bold, NO italics. Casing is the ONLY emphasis.
- Fenced blocks: Use backticks for all technical snippets, variables, types, AND file paths.
- Bracketing: Use square brackets [] for meta-information [Units, Examples, Tags, Narrative].
- Parentheses: Use parentheses () ONLY for logical grouping OR acronym definitions.
- Horizontal rules: Horizontal rules [---] are PROHIBITED EXCEPT as Markdown table delimiters.
- List markers: Unordered lists MUST use hyphen [-] markers. Ordered lists MUST use number AND dot [1.] markers.
- Indentation: Use exactly TWO spaces per level for sub-items AND list continuations.
- Vertical spacing: Exactly ONE newline between logical blocks. A sequence of single-spaced key/value lines is treated as ONE logical block.

## English Reduction [Dense Logic Prose]
- STE usage: Use Simplified Technical English. Narrative fluff is PROHIBITED.
- Active voice: Subject-Verb-Object REQUIRED.
- Atomic logic: EACH line MUST convey exactly ONE fact OR constraint. This applies to `IF/THEN` actions AND key-value assignments.
- Explicit operators: Implicit logic is PROHIBITED. Lists AND definitions MUST use explicit operators [AND, OR] to define relationships.
- Logical flattening:
  - `IF/THEN` is the outermost operator.
  - EACH `IF/THEN` line MUST contain exactly ONE outcome [action].
  - Compound conditions using `AND` are PERMITTED within a single `IF` segment. Conditions using `OR` MUST be split into multiple lines.
- Contract syntax: Preconditions AND postconditions MUST be written as boolean predicates using System Entities, System States, OR Variables.

## Metadata & Traceability
- Placement: Metadata MUST be placed at the top of the document immediately following the H1 header.
- Required fields: ID, Status, Depends on.
- Metadata key-names: MUST follow the Section 2 Casing hierarchy.
- Document changes: Reference specific Section numbers OR Logic Identifiers.
- Circularity: Circular dependencies are PROHIBITED.

## Tables & Lists
- Data mapping: Use single-spaced key/value lines WITHOUT bullets for metadata OR static system properties [Example: `Mode: read-only`].
- Instructional lists: Use hyphen [-] markers ONLY for constraints, requirements, OR rationale.
- Tables: Maintain column consistency. Use exactly ONE space between pipes AND content `| Content |`.

## Document ID System
- Document IDs MUST follow format: `LAYER-NAME`
- Each ID MUST start with a valid Layer token.
- No hierarchical segmentation (DOMAIN / SUBDOMAIN / SEQUENCE) is permitted.
- IDs MUST be stable and globally unique within the system.

### Layer Tokens
- META = documentation structure and governance rules
- INTENT = intent and assumptions
- CONTRACT = executable constraints
- EXPERIMENT = raw outputs and measurements
- DECISION = validated conclusions

### Construction Rules
- IDs MUST use only the LAYER-NAME format.
- No additional segmentation, hierarchy, or token expansion is permitted.
- Tokens MUST NOT be extended beyond the Layer level.
- New Layers MUST be explicitly added before use.

### Uniqueness Rule
- Each document ID MUST be globally unique.
- Uniqueness is enforced at full string level of `LAYER-NAME`.
- No structural uniqueness constraints apply.

### Stability Rule
- Once assigned, a Document ID MUST NOT be modified.
- Renaming requires creation of a new ID and deprecation of the old document.

### Anchor System
- Anchors SHOULD NOT be defined for all sections.
- Anchors MUST be defined ONLY for concepts requiring external reference or cross-document linkage.
- Anchor format MUST be: `[ANCHOR: <ANCHOR_ID>]`
- Anchor IDs MUST be unique within a single document.
- Anchor IDs MUST NOT include document IDs.
- Anchor IDs MUST be stable within the document.
- Anchor IDs MUST be immutable once defined.
- Anchor creation MUST be minimized and used ONLY when a concept is referenced outside the current document.
- Anchors SHOULD NOT be created for purely structural or descriptive sections.
- Excessive anchor creation is PROHIBITED when no cross-reference requirement exists.

### Reference System
- Cross-document references MUST use format: `[REF: DOC-ID::ANCHOR_ID]`
- References MUST include full document ID.
- References MUST include anchor ID.
- Partial references without DOC-ID are PROHIBITED for cross-document linking.
- References MUST NOT depend on section numbers.

### Scope Rule
- Anchor scope is limited to a single document.
- Anchor names MUST NOT assume global uniqueness.
- Global uniqueness is enforced ONLY through DOC-ID + ANCHOR combination.

### Header Independence Rule
- Section headers MAY change without affecting anchors.
- Section numbering MUST NOT be used as a reference system.
- Headers are presentation-only and MUST NOT be referenced externally.

## Notes / Explanatory
- `[EXPLANATORY]` tags denote rationale OR non-binding meta-information.
- Prescriptive assumption: IF the H1 header is tagged `[NORMATIVE]`, THEN ALL statements in the document are prescriptive EXCEPT those tagged `[EXPLANATORY]`.
- Prototyping relaxation: STE simplification may allow short explanatory sentences ONLY within this Notes section.
