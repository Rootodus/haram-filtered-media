WIP

When fixing compile or test errors, run cargo check / tsc first, use rg -n to locate definitions/callers, and read only 40–80 line slices around the reported file:line locations instead of full files.

## Local Context & Tool Discipline
- Never read entire large files at once. Always use `grep` first to locate exact symbols/line numbers, then `read` only the relevant 80–150 line window (`offset` + `limit`).
- Make small, surgical `edit` changes and verify immediately with targeted checks (`cargo check`, `npm test`, `pytest`).
- Keep active context concise so prompt prefill stays fast and cached.
