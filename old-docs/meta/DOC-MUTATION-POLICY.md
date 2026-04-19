# Doc Mutation Policy
ID: DOC-MUT-POLICY  
Status: STABLE  
Depends on: NONE

## Purpose
Defines when documentation structure is allowed to change.

## Rule: Default Stability
- Documentation structure is **stable by default**
- Moving, renaming, or regrouping files is **prohibited unless a trigger condition is met**

## Allowed triggers for structural change
A change is only allowed if at least one condition is true:

### 1. Conflict
Two or more documents define the same responsibility in incompatible ways.

### 2. Dependency break
A document depends on another document that no longer exists, or creates a circular dependency.

### 3. Repeated inconsistency
The same concept requires repeated edits across multiple files (>2–3) to stay consistent.

### 4. Access failure
A concept cannot be reliably located or updated without searching multiple unrelated folders.

## Non-reasons (invalid triggers)
These do NOT justify structural change:
- stylistic preference
- perceived “cleaner” organization
- external critique without a concrete violation above
- optimization without observed failure

## Change requirement
If a trigger is valid:
- the change MUST be recorded in `LOG-DECISIONS.md`
- the reason MUST reference the specific trigger type
