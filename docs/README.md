# Docs Overview
Project: MLFilteredBrowser (MLFB)  
ID: DOCS-README  
Status: STABLE  
Depends on: STD-DOC

## Purpose
- Define the organization of the documentation directory.
- Describe the relationships between document classes.
- Explain the Atomic Specification model for AI-driven development.

## Directory Structure
- `architecture/`: High-level reasoning AND system-wide maps.
- `specs/`: Canonical instructions, interfaces, AND constraints for components.
- `log/`: Append-only historical records AND architectural decisions.
- `experiments/`: Execution records, benchmarks, AND testing evidence.
- `meta/`: Syntax, formatting, AND governance standards.

## ID-Atomic Model
The system uses an ID-Atomic model to minimize clerical overhead AND prevent synchronization loops.

### Semantic Fusion
- EACH `SPEC` file MUST fuse intent, contract, AND decision roles into a single contiguous block.
- Redundancy is PERMITTED within a `SPEC` file to ensure contextual self-sufficiency for AI consumption.
- Cross-document pointers MUST be minimized to reduce manual maintenance.

### Single Authority
- `SPEC` files are the ONLY authoritative source of truth for system behavior.
- `ARCH` files provide context AND mental models but MUST NOT define normative constraints.
- `LOG` files provide history but MUST NOT be used to override current `SPEC` content.

## Workflow Logic

### Code Generation
- IF a component requires generation OR refactoring, THEN provide the corresponding `SPEC` file to the AI.
- The AI MUST treat the `SPEC` as a hard instruction set.

### Iterative Refinement
- IF an experiment in `docs/experiments/` reveals a logic flaw, THEN update the relevant `SPEC` file in `docs/specs/`.
- IF a decision is significant OR irreversible, THEN record the rationale in `docs/log/LOG-DECISIONS.md`.
- IF a `SPEC` is updated, THEN the previous state is preserved in Git history ONLY.

## Experiments Directory Rule
Contains only execution results and validation traces.

Allowed content:
- run logs [Phase 0–n executions]
- measurements [latency, drop rate, throughput]
- observed behavior deviations
- reproduction notes

Not allowed:
- architectural design
- system specifications
- rule definitions
- future plans or phases

## Documentation Governance
- ALL documents MUST adhere to the syntax AND casing rules defined in `STD-DOC`.
- Document IDs MUST be globally unique AND follow the `CLASS-IDENTIFIER` format.
- Semantic Referencing is the primary method for linking concepts across the system.

## Notes / Explanatory
- [EXPLANATORY] This document replaces the previous `META-CORE` layer-based governance.
- [EXPLANATORY] Historical decision tracking is moved to a centralized log to prevent the loop of fixing in SPEC files.
- [EXPLANATORY] The ID-Atomic model prioritizes AI context window density over global normalization.
