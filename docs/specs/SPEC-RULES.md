# Specification Rules [NORMATIVE]
ID: SPEC-RULES  
Status: STABLE  
Depends on: STD-DOC

## Implementation Readiness
- A SPEC is valid ONLY IF an AI can generate a functional Rust module from it without asking for clarification.
- Binary layouts (Endianness, Bit-width, Alignment) MUST be defined for all IPC data.

## Mechanical Sympathy
- Specs MUST prioritize Zero-Copy paths and Cache Locality.
- Use of high-level abstractions (Traits, Generics) is PROHIBITED if they introduce hidden indirection or cloning.
