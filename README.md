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
- `i` / `Shift+I` enter writing mode at the beginning of the selected block, `a` / `Shift+A` at the end
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

## Installing

Every [release](https://github.com/evermake/mrk/releases) has a build for Apple Silicon Macs.
Download it and unpack it in a terminal:

```sh
tar -xzf mrk-v0.1.0-aarch64-apple-darwin.tar.gz
./mrk-v0.1.0-aarch64-apple-darwin/mrk note.md
```

The build is not signed. Unpacked with `tar` it runs as it is. Unpacked by double-clicking in
Finder it is quarantined by macOS, which `xattr -d com.apple.quarantine mrk` lifts.

Next to the binary are mrk's licenses and `THIRD-PARTY-LICENSES.md`, the licenses of
everything it is built from.

## Running from source

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
- `i` / `Shift+I` start writing at the beginning of the selected block, and `a` / `Shift+A` at
  its end, like `Enter`. With several blocks selected, it is the one the selection ends at.
  A divider has no text, so a paragraph is added above it (`i`) or below it (`a`) to write in.
  With nothing selected they do nothing.
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
- Double-click selects a word; holding the button down on the second click and dragging
  selects whole words, across blocks too.
- A text selection can run across blocks. `Shift` + arrows (and `Shift`-click or dragging
  the mouse) extend it past the edge of a block into the next one, and `Cmd+A` selects the
  text of the block, then of the whole note. Typing, `Enter`, `Backspace`/`Delete` and paste
  replace it: what is left of the last block joins the first one, which keeps its type
  (code is not mixed with text, so those stay two blocks). `Cmd+C` copies the blocks it
  reaches into as Markdown, cut to the selection, and `Cmd+X` also deletes it. `Cmd+B` and
  the other marks apply to the selected text of every block. `Esc` selects the blocks it
  reaches into, and `Tab` / `Shift+Tab` and `Alt+Up` / `Alt+Down` move them.

### Zoom

`Cmd++` (or `Cmd+=`) zooms the whole window in, `Cmd+-` out and `Cmd+0` goes back to the
actual size, as in a browser. The steps run from 50% to 300%, and the part of the note in view
stays in view. The same commands are in the View menu.

### Scrollbar

A note longer than the window has a scrollbar on its right edge. The thumb is as long as the
share of the note that is in view, and sits where that part is in the note. Dragging the thumb
scrolls, and pressing the track above or below it brings the thumb to the pointer.

It follows "Show scroll bars" in the macOS Appearance settings: it is either always there, or
shows while scrolling and while the pointer is at the right edge, and when a note opens.

### Files

- `Cmd+O` open, `Cmd+N` new, `Cmd+S` save, `Cmd+Shift+S` save as.
- Saving writes one canonical style: `-` bullets, `#` headings, fenced code, `*`/`**`
  emphasis, lists renumbered from 1. If the file as opened would not come back byte for byte,
  that original is first copied to `<name>.md.bak` (or `<name>.md.1.bak` and so on, rather
  than overwriting a different earlier backup).

## Not done yet

- Slash menu for block types.
- Inline Markdown shortcuts while typing (`**bold**`).
- Rendering only the visible blocks. Every block is laid out on each frame, which is
  unnoticeable for ordinary notes but measured about 28 ms per keystroke on a 3,000-block
  document.
- Reloading when the file changes on disk.
- Remembering the zoom between launches.

## Development

- `src/document.rs`, `src/rich_text.rs`: the block tree and styled text.
- `src/markdown.rs`: conversion to and from Markdown.
- `src/editor.rs`, `src/block_text.rs`: the editor view and its text element.
- `src/scrollbar.rs`: the scrollbar of the note.
- `src/workspace.rs`: the window, file handling and backups.

The editor's tests press real keys and compare the resulting document (`src/editor/tests.rs`).
The typing shortcuts have their own suite, `src/editor/tests/typing_shortcuts.rs`: its
`MARKERS`, `FENCES` and `RULES` tables list every shortcut, so a new shortcut starts as a new
row there, and a new key binding as a test that fails without it.

`cargo run --example screenshot -- note.md out.png "down down enter"` renders a file to a PNG
without opening a window, optionally after replaying keystrokes.

## Releasing

Pull requests are squash-merged and the title becomes the commit on `main`, so the title is a
[Conventional Commit](https://www.conventionalcommits.org); the "PR title" check enforces
that. [release-plz](https://release-plz.dev) reads those commits to choose the next version
and to write `CHANGELOG.md`, so a title is written for the people who read release notes.

| Title | Release notes | Version |
| --- | --- | --- |
| `feat: …` | Added | 0.1.0 → 0.1.1 |
| `fix: …` | Fixed | 0.1.0 → 0.1.1 |
| `perf: …` | Changed | 0.1.0 → 0.1.1 |
| `refactor`, `docs`, `test`, `chore`, `ci`, `build`, `revert` | not listed | no release of their own |
| any type with `!`, as in `feat!: …` | marked as breaking | 0.1.0 → 0.2.0 |

Before 1.0 the middle number is the breaking one, which is how Cargo reads `0.x` versions.
From 1.0 on, `feat` bumps the middle number and a breaking change the first.

To release, run the Release workflow from the Actions tab, or:

```sh
gh workflow run release.yml                  # release
gh workflow run release.yml -f dry_run=true  # show and build what would be released
```

It updates the version and the changelog, runs the tests, builds, pushes a
`chore: release vX.Y.Z` commit to `main`, tags it and publishes a GitHub release with the
binary attached. A dry run stops after the build and keeps the binary as a workflow
artifact. Running the workflow again finishes a release that failed halfway.

The download also carries the licenses of the crates linked into the binary, collected by
[cargo-about](https://github.com/EmbarkStudios/cargo-about). `about.toml` lists the licenses
that are accepted, and a dependency under any other license fails the release until its
license is reviewed and added there.

The setup lives in `release-plz.toml`, `about.toml`, `about.hbs` and `.github/workflows/`. It
relies on one repository setting: squash merges use the pull request title as the commit title (Settings → General →
Pull Requests). With GitHub's default, a pull request with a single commit is merged under
that commit's message instead.

## License

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in mrk by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
