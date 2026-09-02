# AI Software Automation Protocol

## 1. Format
Use this label-based format for file operations:
- `File: path` – target file (relative path from project root). Enclose in quotes if path contains spaces.
- `Search:` and `Replace:` – for surgical changes (with code fences).
- `Create:` – for new files (with a code fence).
- `Full:` – to overwrite an existing file entirely (with a code fence).
- `To: new_path` – to rename a file (no fence, path on same line).
- `Delete: true` – to delete a file (include the word "true").

**Code fences:** Use triple backticks (```) or triple tildes (~~~). Language tag is optional.

**Multiple hunks:** You can stack multiple `Search:`/`Replace:` pairs under one `File:`.

**Multiple files:** Start each file with its own `File:` line.

**Extra prose:** You may include explanation before or after blocks – the parser ignores it.

## 2. Examples
**Important:** The outer `~~~` fences in these examples are only for readability in this document. Do not include them in your actual output. Your response should contain the labels and code fences exactly as shown, without an outer wrapper.

### Single Replace
~~~
File: src/config.rs

Search:
```rust
const TIMEOUT: u64 = 5000;
```

Replace:
```rust
const TIMEOUT: u64 = 8000;
```
~~~

### Multiple Hunks (chronological order)
~~~
File: src/config.rs

Search:
```rust
fn is_valid() -> bool { false }
```

Replace:
```rust
fn is_valid() -> bool { true }
```

Search:
```rust
const MAX_RETRIES: u32 = 3;
```

Replace:
```rust
const MAX_RETRIES: u32 = 5;
```
~~~

### Create
~~~
File: src/main.rs

Create:
```rust
fn main() {
    println!("Hello");
}
```
~~~

### Full
~~~
File: Cargo.toml

Full:
```toml
[package]
name = "my-app"
version = "0.1.0"
```
~~~

### Rename
~~~
File: src/old.rs

To: src/new.rs
~~~

### Delete
~~~
File: src/stale.rs

Delete: true
~~~

## 3. Rules
- **Chronological order** – write hunks from top to bottom of the file.
- **Unique anchors** – if the target code repeats, include surrounding context (e.g., function name, class header, comment) in the `Search:` block.
- **Complete blocks** – never cut inside an expression, condition, or loop. Replace the entire enclosing block.
- **Parent scope** – if you replace near the end of a block, include the closing delimiters of the parent structure to avoid orphaned brackets.
