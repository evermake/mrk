//! Regression tests for the Markdown typing shortcuts: a marker typed at the start of a
//! paragraph and followed by a space converts the block, and a code fence or a rule converts
//! it on Enter.
//!
//! [`MARKERS`], [`FENCES`] and [`RULES`] list every shortcut the editor promises. A new
//! shortcut gets a row there, and the tests below then cover it.

use gpui::{Entity, TestAppContext, VisualTestContext};
use pretty_assertions::assert_eq;

use super::*;

/// Every marker that converts a paragraph when a space follows it, with the document that
/// typing it into an empty note leaves behind.
const MARKERS: &[(&str, &str)] = &[
    ("#", "h1 ˇ\n"),
    ("##", "h2 ˇ\n"),
    ("###", "h3 ˇ\n"),
    ("####", "h4 ˇ\n"),
    ("#####", "h5 ˇ\n"),
    ("######", "h6 ˇ\n"),
    ("-", "- ˇ\n"),
    ("*", "- ˇ\n"),
    ("+", "- ˇ\n"),
    ("1.", "1. ˇ\n"),
    ("1)", "1. ˇ\n"),
    ("42.", "1. ˇ\n"),
    ("[]", "[ ] ˇ\n"),
    ("[ ]", "[ ] ˇ\n"),
    ("[x]", "[x] ˇ\n"),
    ("[X]", "[x] ˇ\n"),
    (">", "> ˇ\n"),
    ("```", "code() ˇ\n"),
    ("---", "---\np ˇ\n"),
];

/// What converts a paragraph into a code block on Enter, with the block's language.
const FENCES: &[(&str, &str)] = &[("```", ""), ("```rust", "rust"), ("```c++", "c++")];

/// What converts a paragraph into a divider on Enter.
const RULES: &[&str] = &["---", "***", "___"];

/// A new note with the caret in its only, empty paragraph.
fn empty_note(cx: &mut TestAppContext) -> (Entity<Editor>, VisualTestContext) {
    let (editor, mut cx) = open("", cx);
    cx.simulate_keystrokes("enter");
    (editor, cx)
}

/// Opens `source` with the caret at the start of its first block.
fn writing_at_start(source: &str, cx: &mut TestAppContext) -> (Entity<Editor>, VisualTestContext) {
    let (editor, mut cx) = open(source, cx);
    cx.simulate_keystrokes("down enter cmd-left");
    (editor, cx)
}

#[gpui::test]
fn a_marker_and_a_space_convert_a_paragraph(cx: &mut TestAppContext) {
    for &(marker, converted) in MARKERS {
        let (editor, mut cx) = empty_note(cx);
        type_text(marker, &mut cx);
        assert_eq!(
            state(&editor, &mut cx),
            format!("p {marker}ˇ\n"),
            "{marker:?} converted before the space"
        );
        type_text(" ", &mut cx);
        assert_eq!(state(&editor, &mut cx), converted, "after {marker:?}");
        assert_eq!(mode(&editor, &mut cx), Mode::Writing, "after {marker:?}");
    }
}

#[gpui::test]
fn undoing_a_conversion_brings_back_the_typed_marker(cx: &mut TestAppContext) {
    for &(marker, converted) in MARKERS {
        let (editor, mut cx) = empty_note(cx);
        type_text(marker, &mut cx);
        type_text(" ", &mut cx);
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(
            state(&editor, &mut cx),
            format!("p {marker} ˇ\n"),
            "undoing {marker:?}"
        );
        cx.simulate_keystrokes("cmd-shift-z");
        assert_eq!(state(&editor, &mut cx), converted, "redoing {marker:?}");
    }
}

#[gpui::test]
fn backspace_turns_a_converted_block_back_into_a_paragraph(cx: &mut TestAppContext) {
    for &(marker, _) in MARKERS {
        let (editor, mut cx) = empty_note(cx);
        type_text(marker, &mut cx);
        type_text(" ", &mut cx);
        cx.simulate_keystrokes("backspace");
        assert_eq!(state(&editor, &mut cx), "p ˇ\n", "after {marker:?}");
    }
}

#[gpui::test]
fn text_that_only_resembles_a_marker_stays_text(cx: &mut TestAppContext) {
    for text in [
        "#######",
        "#a",
        "--",
        "----",
        "1",
        ".",
        "a.",
        "1..",
        "1.2.",
        // Markdown numbers lists with at most nine digits.
        "1234567890.",
        "[y]",
        "[  ]",
        "-[]",
        ">>",
        "``",
        "````",
    ] {
        let (editor, mut cx) = empty_note(cx);
        type_text(text, &mut cx);
        type_text(" ", &mut cx);
        assert_eq!(
            state(&editor, &mut cx),
            format!("p {text} ˇ\n"),
            "after {text:?}"
        );
    }
}

#[gpui::test]
fn a_marker_typed_before_existing_text_converts_the_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = writing_at_start("hello **world**\n", cx);
    type_text("## ", &mut cx);
    assert_eq!(state(&editor, &mut cx), "h2 ˇhello <b>world</b>\n");
    assert_eq!(saved(&editor, &mut cx), "## hello **world**\n");
}

#[gpui::test]
fn a_code_block_takes_over_existing_text_without_its_styles(cx: &mut TestAppContext) {
    let (editor, mut cx) = writing_at_start("hello **world**\n", cx);
    type_text("``` ", &mut cx);
    assert_eq!(state(&editor, &mut cx), "code() ˇhello world\n");
    assert_eq!(saved(&editor, &mut cx), "```\nhello world\n```\n");
}

#[gpui::test]
fn a_marker_typed_over_selected_text_converts_the_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = writing_at_start("old text\n", cx);
    cx.simulate_keystrokes("cmd-a");
    type_text("> new", &mut cx);
    assert_eq!(state(&editor, &mut cx), "> newˇ\n");
}

#[gpui::test]
fn a_marker_converts_only_at_the_start_of_the_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = empty_note(cx);
    type_text("a # b - c > d", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p a # b - c > dˇ\n");

    // The start of a later line of the same block does not count.
    cx.simulate_keystrokes("shift-enter");
    type_text("# ", &mut cx);
    assert_eq!(state(&editor, &mut cx), "p a # b - c > d⏎# ˇ\n");
}

#[gpui::test]
fn markers_are_text_in_blocks_that_are_not_paragraphs(cx: &mut TestAppContext) {
    for (source, typed, expected) in [
        ("# Title\n", "- ", "h1 - ˇTitle\n"),
        ("# Title\n", "## ", "h1 ## ˇTitle\n"),
        ("- item\n", "# ", "- # ˇitem\n"),
        ("- item\n", "1. ", "- 1. ˇitem\n"),
        ("1. item\n", "- ", "1. - ˇitem\n"),
        ("- [ ] task\n", "# ", "[ ] # ˇtask\n"),
        ("> quote\n", "- ", "> - ˇquote\n"),
        ("```\ncode\n```\n", "# ", "code() # ˇcode\n"),
        ("```\ncode\n```\n", "- ", "code() - ˇcode\n"),
        ("```\ncode\n```\n", "[] ", "code() [] ˇcode\n"),
    ] {
        let (editor, mut cx) = writing_at_start(source, cx);
        type_text(typed, &mut cx);
        assert_eq!(
            state(&editor, &mut cx),
            expected,
            "typing {typed:?} into {source:?}"
        );
    }
}

#[gpui::test]
fn a_task_marker_converts_a_bullet(cx: &mut TestAppContext) {
    // This is how a to-do is written in Markdown: the bullet first, then the checkbox.
    for (typed, expected) in [
        ("- [ ] task", "[ ] taskˇ\n"),
        ("- [] task", "[ ] taskˇ\n"),
        ("- [x] task", "[x] taskˇ\n"),
        ("* [X] task", "[x] taskˇ\n"),
    ] {
        let (editor, mut cx) = empty_note(cx);
        type_text(typed, &mut cx);
        assert_eq!(state(&editor, &mut cx), expected, "typing {typed:?}");
    }
}

#[gpui::test]
fn a_bullet_keeps_its_children_when_it_becomes_a_to_do(cx: &mut TestAppContext) {
    let (editor, mut cx) = writing_at_start("- parent\n  - child\n", cx);
    type_text("[] ", &mut cx);
    assert_eq!(state(&editor, &mut cx), "[ ] ˇparent\n  - child\n");
    assert_eq!(saved(&editor, &mut cx), "- [ ] parent\n  - child\n");
}

#[gpui::test]
fn a_code_fence_converts_on_enter(cx: &mut TestAppContext) {
    for &(fence, language) in FENCES {
        let (editor, mut cx) = empty_note(cx);
        type_text(fence, &mut cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(
            state(&editor, &mut cx),
            format!("code({language}) ˇ\n"),
            "after {fence:?}"
        );
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(
            state(&editor, &mut cx),
            format!("p {fence}ˇ\n"),
            "undoing {fence:?}"
        );
    }
}

#[gpui::test]
fn a_rule_converts_on_enter(cx: &mut TestAppContext) {
    for &rule in RULES {
        let (editor, mut cx) = empty_note(cx);
        type_text(rule, &mut cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(state(&editor, &mut cx), "---\np ˇ\n", "after {rule:?}");
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(
            state(&editor, &mut cx),
            format!("p {rule}ˇ\n"),
            "undoing {rule:?}"
        );
    }
}

#[gpui::test]
fn enter_splits_what_is_not_quite_a_fence_or_a_rule(cx: &mut TestAppContext) {
    for (typed, expected) in [
        ("--\nx", "p --\np xˇ\n"),
        ("----\nx", "p ----\np xˇ\n"),
        ("```a`b\nx", "p ```a`b\np xˇ\n"),
        // Fences and rules convert paragraphs only.
        ("- ---\nx", "- ---\n- xˇ\n"),
        ("> ***\nx", "> ***\np xˇ\n"),
        ("# ```\nx", "h1 ```\np xˇ\n"),
    ] {
        let (editor, mut cx) = empty_note(cx);
        type_text(typed, &mut cx);
        assert_eq!(state(&editor, &mut cx), expected, "typing {typed:?}");

        // Text that looks like Markdown must still be text after saving and reopening.
        let outline = editor.read_with(&cx, |editor, _| editor.document().outline());
        assert_eq!(
            markdown::parse(&saved(&editor, &mut cx)).outline(),
            outline,
            "reopening what typing {typed:?} saves"
        );
    }
}

#[gpui::test]
fn enter_and_tab_are_text_in_a_code_block(cx: &mut TestAppContext) {
    let (editor, mut cx) = empty_note(cx);
    type_text("```rust\nlet x;\n", &mut cx);
    cx.simulate_keystrokes("tab");
    type_text("# y", &mut cx);
    assert_eq!(state(&editor, &mut cx), "code(rust) let x;⏎    # yˇ\n");

    // Cmd+Enter is the way out.
    cx.simulate_keystrokes("cmd-enter");
    type_text("after", &mut cx);
    assert_eq!(
        state(&editor, &mut cx),
        "code(rust) let x;⏎    # y\np afterˇ\n"
    );
    assert_eq!(
        saved(&editor, &mut cx),
        "```rust\nlet x;\n    # y\n```\n\nafter\n"
    );
}

#[gpui::test]
fn enter_on_an_empty_converted_block_turns_it_back_into_a_paragraph(cx: &mut TestAppContext) {
    for marker in ["#", "-", "1.", "[]", "[x]", ">"] {
        let (editor, mut cx) = empty_note(cx);
        type_text(marker, &mut cx);
        type_text(" \n", &mut cx);
        assert_eq!(state(&editor, &mut cx), "p ˇ\n", "after {marker:?}");
    }
}

#[gpui::test]
fn enter_continues_a_list_and_ends_other_blocks(cx: &mut TestAppContext) {
    for (typed, expected) in [
        ("- a\n", "- a\n- ˇ\n"),
        ("1. a\n", "1. a\n1. ˇ\n"),
        ("[] a\n", "[ ] a\n[ ] ˇ\n"),
        // The next to-do is still to be done.
        ("[x] a\n", "[x] a\n[ ] ˇ\n"),
        ("# a\n", "h1 a\np ˇ\n"),
        ("> a\n", "> a\np ˇ\n"),
    ] {
        let (editor, mut cx) = empty_note(cx);
        type_text(typed, &mut cx);
        assert_eq!(state(&editor, &mut cx), expected, "typing {typed:?}");
    }
}

#[gpui::test]
fn typing_markdown_writes_that_markdown(cx: &mut TestAppContext) {
    for (typed, markdown) in [
        ("# Title", "# Title\n"),
        ("###### Small", "###### Small\n"),
        ("- item", "- item\n"),
        ("* item", "- item\n"),
        ("+ item", "- item\n"),
        ("1. item", "1. item\n"),
        ("7) item", "1. item\n"),
        ("[] task", "- [ ] task\n"),
        ("[x] task", "- [x] task\n"),
        ("- [ ] task", "- [ ] task\n"),
        ("> quote", "> quote\n"),
        ("```sh\nls -la", "```sh\nls -la\n```\n"),
        ("above\n---\nbelow", "above\n\n---\n\nbelow\n"),
    ] {
        let (editor, mut cx) = empty_note(cx);
        type_text(typed, &mut cx);
        assert_eq!(saved(&editor, &mut cx), markdown, "typing {typed:?}");
    }
}

#[gpui::test]
fn a_whole_note_can_be_typed_as_markdown(cx: &mut TestAppContext) {
    let (editor, mut cx) = empty_note(cx);
    type_text("# Groceries\n", &mut cx);
    // Enter on the empty item that follows a list ends the list.
    type_text("[] milk\neggs\n\n", &mut cx);
    type_text("1. first\nsecond\n\n", &mut cx);
    type_text("> quote\n", &mut cx);
    type_text("```sh\necho hi", &mut cx);
    cx.simulate_keystrokes("cmd-enter");
    type_text("---\ndone", &mut cx);

    assert_eq!(
        state(&editor, &mut cx),
        "\
h1 Groceries
[ ] milk
[ ] eggs
1. first
1. second
> quote
code(sh) echo hi
---
p doneˇ
"
    );
    let markdown = saved(&editor, &mut cx);
    assert_eq!(
        markdown,
        "\
# Groceries

- [ ] milk
- [ ] eggs

1. first
2. second

> quote

```sh
echo hi
```

---

done
"
    );
    let outline = editor.read_with(&cx, |editor, _| editor.document().outline());
    assert_eq!(markdown::parse(&markdown).outline(), outline);
}

#[test]
fn shortcut_kinds() {
    let paragraph = BlockKind::Paragraph;
    let unchecked = BlockKind::Todo { checked: false };
    let checked = BlockKind::Todo { checked: true };
    let code = BlockKind::Code {
        language: String::new(),
    };

    for level in 1..=6 {
        assert_eq!(
            shortcut_kind(&"#".repeat(level), &paragraph),
            Some(BlockKind::Heading(level as u8))
        );
    }
    assert_eq!(shortcut_kind("", &paragraph), None);
    assert_eq!(shortcut_kind("#######", &paragraph), None);

    for marker in ["-", "*", "+"] {
        assert_eq!(shortcut_kind(marker, &paragraph), Some(BlockKind::Bullet));
    }
    for marker in ["1.", "1)", "0.", "12.", "123456789."] {
        assert_eq!(shortcut_kind(marker, &paragraph), Some(BlockKind::Numbered));
    }
    for marker in ["1", ".", ")", "a.", "1..", "1.)", "1.2.", "1234567890."] {
        assert_eq!(shortcut_kind(marker, &paragraph), None, "{marker:?}");
    }
    assert_eq!(shortcut_kind(">", &paragraph), Some(BlockKind::Quote));
    assert_eq!(shortcut_kind("```", &paragraph), Some(code.clone()));
    assert_eq!(shortcut_kind("---", &paragraph), Some(BlockKind::Divider));

    // A to-do can be started in a paragraph or, as Markdown spells it, in a bullet.
    for current in [&paragraph, &BlockKind::Bullet] {
        for marker in ["[]", "[ ]"] {
            assert_eq!(shortcut_kind(marker, current), Some(unchecked.clone()));
        }
        for marker in ["[x]", "[X]"] {
            assert_eq!(shortcut_kind(marker, current), Some(checked.clone()));
        }
    }

    // Nothing else converts a bullet, and nothing at all converts the other blocks.
    let markers = ["#", "-", "1.", "[]", "[x]", ">", "```", "---"];
    for marker in markers {
        if !marker.starts_with('[') {
            assert_eq!(
                shortcut_kind(marker, &BlockKind::Bullet),
                None,
                "{marker:?}"
            );
        }
        for current in [
            BlockKind::Heading(1),
            BlockKind::Numbered,
            unchecked.clone(),
            BlockKind::Quote,
            code.clone(),
            BlockKind::Divider,
            BlockKind::Raw,
        ] {
            assert_eq!(
                shortcut_kind(marker, &current),
                None,
                "{marker:?} in {current:?}"
            );
        }
    }
}
