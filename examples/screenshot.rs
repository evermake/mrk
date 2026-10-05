//! Renders a Markdown file in the editor without showing a window and writes the result to a
//! PNG, optionally after replaying keystrokes. This is a development aid for checking how the
//! editor looks.
//!
//! ```sh
//! cargo run --example screenshot -- note.md out.png "down down enter"
//! ```

use anyhow::{Context as _, Result};
use gpui::{AppContext as _, Focusable as _, VisualTestAppContext, px, size};
use mrk::editor::{self, Editor};
use mrk::markdown;

fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let source_path = arguments.next().context("missing the Markdown file")?;
    let output_path = arguments.next().context("missing the output path")?;
    let keystrokes = arguments.next().unwrap_or_default();
    let source =
        std::fs::read_to_string(&source_path).with_context(|| format!("reading {source_path}"))?;

    let mut cx = VisualTestAppContext::new(gpui_platform::current_platform(false));
    cx.update(editor::bind_keys);
    let window = cx.open_offscreen_window(size(px(900.), px(760.)), |window, cx| {
        let editor = cx.new(|cx| Editor::new(markdown::parse(&source), cx));
        window.focus(&editor.focus_handle(cx), cx);
        editor
    })?;
    cx.run_until_parked();
    // There is no display driving frames here, so the work the editor defers to the next
    // frame (scrolling the selection into view) is run by hand after every keystroke.
    for keystroke in keystrokes.split_whitespace() {
        cx.simulate_keystrokes(window.into(), keystroke);
        cx.update_window(window.into(), |_, window, cx| {
            window.simulate_next_frame(cx)
        })?;
        cx.run_until_parked();
    }

    cx.capture_screenshot(window.into())?
        .save(&output_path)
        .with_context(|| format!("writing {output_path}"))?;
    Ok(())
}
