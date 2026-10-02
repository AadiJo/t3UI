# Markdown rendering check

This message exercises **every element** the chat renders, with *emphasis*, ***both at once***, ~~struck text~~, and `inline code`. Links look like [the GPUI docs](https://www.gpui.rs/) and bare URLs such as https://github.com/AadiJo/t3UI become links too. File references render as chips: [ChatMarkdown.tsx](apps/web/src/components/ChatMarkdown.tsx), [main.rs](/Users/user/t3code/crates/app/src/main.rs:42) and [lib.rs](crates/t3-ui/src/lib.rs#L12C3).

A second paragraph with a long unbroken token `crates/t3-markdown/src/really/deeply/nested/module/path/that/keeps/going/and/going.rs` to check wrapping, followed by a footnote reference.[^1]

## Lists

- First item with `code`
- Second item with a [link](https://example.com)
  - Nested item
    - Third level
- Back at the top level

1. Install the toolchain
2. Run the checks
   1. Lint with clippy
   2. Run the tests
3. Land the change

- [x] Parse markdown with pulldown-cmark
- [ ] Highlight code with the Pierre theme

### Quotes and rules

> Selection and copy should work the way the browser does,
> including across paragraphs.

---

#### Table

| Language | Grammar | Parity | Notes |
| :-- | :-- | --: | --- |
| TypeScript | bat | 99.1% | `Map` and `Date` differ |
| Rust | Shiki | 100% | translated with syntect-tmlanguage |
| Bash | bat + aliases | 89.3% | Shiki's grammar needs `\G` anchors that syntect cannot express, so a few tokens differ |

##### Code

```ts title="src/turns.ts"
export class TurnTracker<T extends { threadId: string }> {
  private readonly turns = new Map<string, T>();

  update(turn: T): void {
    if (this.turns.get(turn.threadId) === turn) return;
    this.turns.set(turn.threadId, turn); // keep the latest
  }
}
```

```rust
fn main() -> anyhow::Result<()> {
    let text = std::fs::read_to_string("README.md")?;
    println!("{} lines", text.lines().count());
    Ok(())
}
```

```python
async def fetch(thread_id: str) -> list[str]:
    """Load messages for a thread."""
    return [f"hello from {thread_id}"]
```

```bash
cargo build --release -p t3-app && echo "built $(date +%s)"
```

```diff
@@ -1,3 +1,3 @@
-const enabled = false;
+const enabled = true;
 export default enabled;
```

```
plain text block with a very long line that should scroll horizontally instead of wrapping because wrap is off by default in the fork
```

###### Details

<details>
<summary>Implementation notes</summary>

Collapsed content stays hidden until the summary is clicked.

</details>

[^1]: Footnotes render at the end of the message.
