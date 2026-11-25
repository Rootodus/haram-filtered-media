## Local Context & Tool Discipline
- Skip redundant checks: If `cargo check`, `cargo test`, or `tsc` output with `file:line` paths is already in the prompt, do **not** re-run the check first—jump straight to `read`.
- Batch tool calls: When an error spans multiple files or needs both a caller and a type definition, emit all `read` / `grep` calls together in one turn, and emit all `edit` calls together in one turn.
- Read smart slices: For files > 200 lines, use `grep` (`rg -n`) if needed and `read` a 100–180 line window (`offset` + `limit`) around the error/symbol (plus top imports if missing). For modular files <= 200 lines, read the whole file in one shot.
- **Never** weaken or edit tests: Fix compiler and test failures strictly in `src/`. Never modify `tests/`, `*_test.rs`, `*.test.ts`, or `*.spec.ts` unless explicitly instructed.
- Surgical edits & targeted checks: Make minimal `edit` diffs and verify with targeted commands (`cargo check`, `cargo test <name>`, `tsc --noEmit`). Keep context compact to preserve prefix cache.
