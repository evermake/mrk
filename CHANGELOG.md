# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/evermake/mrk/compare/v0.1.0...v0.1.1) - 2026-10-06

### Added

- make a divider as soon as `---` is typed and keep it on Backspace below it ([#16](https://github.com/evermake/mrk/pull/16))
- toggle the selected to-dos with Space in navigation mode ([#15](https://github.com/evermake/mrk/pull/15))
- add a document scrollbar ([#14](https://github.com/evermake/mrk/pull/14))
- enter writing mode with i and a, like in Vim ([#13](https://github.com/evermake/mrk/pull/13))
- select whole words by dragging after a double-click ([#12](https://github.com/evermake/mrk/pull/12))
- zoom the window in and out with ⌘+, ⌘- and ⌘0 ([#11](https://github.com/evermake/mrk/pull/11))
- select text across blocks ([#10](https://github.com/evermake/mrk/pull/10))

## [0.1.0](https://github.com/evermake/mrk/releases/tag/v0.1.0) - 2026-10-06

The first release: a block-based Markdown editor for macOS.

### Added

- Notes made of blocks: paragraphs, headings, bulleted and numbered lists, to-dos, quotes, code blocks and dividers, with nesting.
- Navigation mode, which selects, moves, nests, reorders, deletes and opens blocks from the keyboard, with Vim-style keys.
- Writing mode, with Markdown typing shortcuts (`#`, `-`, `1.`, `[]`, `>`, <code>```</code>, `---`), bold, italic, inline code, strikethrough and links.
- Opening and saving `.md` files. A file that saving would not reproduce byte for byte is first copied to a `.bak`.
- Undo and redo.
