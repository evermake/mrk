# mrk

[GPUI](https://gpui.rs)-based rich-text editor with Notion-like block structure.

I want to have a fast and minimalistic rich-text editor with *convenient keybindings*.

**Limitation**: Markdown conversion is *not loseless*.

## What is "convenient keybindings"

Everything is a block (like in Notion), and there 2 modes: *navigation* & *writing*.

When document (a note) is open, no mode by default:
- `Down` enters *navigation* and selects the first block in the document
- `Up` enters *navigation* and selects the last block in the document

## Navigation mode

Like visual mode in Vim, but blocks are selected instead of lines.

- `Down`/`Up` arrows select either next/prev child (for nested blocks) or sibling (of selected block or parent's one if the last child is selected)
- `Right`/`Left` arrows select 1st child/ancestor
- `Enter` enters writing mode at the end of text content of the selected block
- `Tab` nests the selected block inside the sibling above (if block type supports nesting).
- `Shift+Tab` moves the selected block out of it's parent. If there are other blocks that were after the selected block inside nested parent, they became nested in the moved block (i.e. vertical order of blocks stays the same)
- `Shift` + arrows selects multiple blocks at once.
- `Backspace` / `Delete` deletes selected blocks
- `Alt+Up`, `Alt+Down` re-order blocks

`h`, `j`, `k`, `l` keys work like arrows.

## Writing mode

- Arrow keys move between characters in text (left, right) and between lines (up, down) in a similar way as one would expect in any text editor app. Moving between blocks is seamless.
- When text inside editor is selected and valid URL is pasted (e.g. Cmd+V), selected text is wrapped with the pasted link
- `Esc` enters the navigation mode and selects the block
- `Cmd+b` - bold, `Cmd+i` - italics

## MVP scope

- MacOS only.
- Edit history (`Cmd+Z` undo, `Cmd+Shift+Z` redo) is supported (easiest possible implementation for now).
- When app is launched, suggests to select a file. Only `.md` files are supported.
- 1 open document at the time.
- Internal doc structure is represented as blocks and conversion happens on open/save/export.
- Before saving, if conversion is loses something (not 1:1), should create a `.bak`-suffixed backup of the file.

## Running

Needs a recent stable Rust and Xcode.

```sh
cargo run --release              # asks for a file
cargo run --release -- note.md   # opens a file directly
cargo test
```

GPUI is a git dependency pinned to one Zed commit, so the first build downloads the Zed
repository. Shaders are compiled at startup (GPUI's `runtime_shaders` feature), which avoids
needing Xcode's optional Metal Toolchain component.

## How the MVP fills in the details

### Blocks

Paragraphs, headings, bulleted and numbered lists, to-dos, quotes, code blocks and dividers.
Only list items, to-dos and quotes can hold nested blocks, because those are the nestings
Markdown can express.

Markdown that has no block (tables, HTML, front matter, footnotes) is shown as a verbatim
"markdown" block, and images and inline HTML as verbatim text. They are editable as source
and saved back unchanged.

### Navigation mode

- A selected block includes everything nested in it; `Shift` + `Up`/`Down` extends the
  selection over siblings.
- `Enter` with nothing selected starts writing at the end of the document.
- `Esc` clears the selection.
- `d` deletes the selected blocks, like `Backspace`.
- `o` / `Shift+O` add an empty block below / above the selection, at its nesting level, and
  start writing in it. Next to a list item, numbered item or to-do it is another one (a to-do
  starts unchecked); next to anything else it is a paragraph. Below a block means below
  everything nested in it. With nothing selected the block goes to the end / start of the
  document.
- `Cmd+C` / `Cmd+X` / `Cmd+V` copy, cut and paste blocks as Markdown.

### Writing mode

Typing a marker at the start of a paragraph, followed by a space, converts the block:

| Typed | Becomes |
| --- | --- |
| `#` … `######` | heading |
| `-`, `*`, `+` | bullet |
| `1.`, `1)` | numbered item |
| `[]`, `[ ]`, `[x]` (also inside a bullet) | to-do |
| `>` | quote |
| ` ``` ` | code block |
| `---` | divider |

` ```rust ` followed by `Enter` makes a code block with a language, and `---` followed by
`Enter` a divider.

- `Enter` splits the block; on an empty list item it ends the list.
- `Shift+Enter` breaks the line inside the block; `Cmd+Enter` starts a new paragraph below
  (the way out of a code block, where `Enter` is a line break).
- `Backspace` at the start of a block turns it into a paragraph, then joins it with the block
  above.
- `Tab` / `Shift+Tab` and `Alt+Up` / `Alt+Down` act on the block being written in.
- `Cmd+E` toggles inline code and `Cmd+Shift+X` strikethrough.
- `Cmd`-click opens a link.
- A text selection stays within one block.

### Files

- `Cmd+O` open, `Cmd+N` new, `Cmd+S` save, `Cmd+Shift+S` save as.
- Saving writes one canonical style: `-` bullets, `#` headings, fenced code, `*`/`**`
  emphasis, lists renumbered from 1. If the file as opened would not come back byte for byte,
  that original is first copied to `<name>.md.bak` (or `<name>.md.1.bak` and so on, rather
  than overwriting a different earlier backup).

## Not done yet

- Slash menu for block types.
- Text selection across blocks.
- Inline Markdown shortcuts while typing (`**bold**`).
- Rendering only the visible blocks. Every block is laid out on each frame, which is
  unnoticeable for ordinary notes but measured about 28 ms per keystroke on a 3,000-block
  document.
- Reloading when the file changes on disk.

## Development

- `src/document.rs`, `src/rich_text.rs`: the block tree and styled text.
- `src/markdown.rs`: conversion to and from Markdown.
- `src/editor.rs`, `src/block_text.rs`: the editor view and its text element.
- `src/workspace.rs`: the window, file handling and backups.

The editor's tests press real keys and compare the resulting document (`src/editor/tests.rs`).
The typing shortcuts have their own suite, `src/editor/tests/typing_shortcuts.rs`: its
`MARKERS`, `FENCES` and `RULES` tables list every shortcut, so a new shortcut starts as a new
row there, and a new key binding as a test that fails without it.

`cargo run --example screenshot -- note.md out.png "down down enter"` renders a file to a PNG
without opening a window, optionally after replaying keystrokes.
