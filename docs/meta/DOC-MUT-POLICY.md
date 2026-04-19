# Doc Mutation Policy [NORMATIVE]
ID: DOC-MUT-POLICY  
Status: STABLE  
Depends on: NONE

## Purpose
Defines when documentation structure is encouraged to change.

## Rule: Default Stability
- Documentation structure is stable by default
- Moving, renaming, or regrouping files is discouraged unless a trigger condition is met

## Allowed triggers for structural change
A change is only encouraged if at least one condition is true:

### 1. Conflict
Two or more documents define the same responsibility in incompatible ways.

### 2. Dependency break
A document depends on another document that no longer exists, or creates a circular dependency.

### 3. Repeated inconsistency
The same concept requires repeated edits across multiple files (>2–3) to stay consistent.

### 4. Access failure
A concept cannot be reliably located or updated without searching multiple unrelated folders.

## Non-reasons (mostly invalid triggers)
These generally do not justify structural change:
- stylistic preference
- perceived cleaner organization
- external critique without a concrete violation above
- optimization without observed failure
