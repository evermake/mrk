use std::path::PathBuf;

use gpui::{
    App, AppContext as _, Bounds, Menu, MenuItem, TitlebarOptions, WindowBounds, WindowOptions, px,
    size,
};

use mrk::editor::{self, actions as edit};
use mrk::workspace::{
    self, CloseWindow, NewFile, OpenFile, Quit, ResetZoom, Save, SaveAs, Workspace, ZoomIn, ZoomOut,
};

fn main() {
    // A file can be given on the command line; otherwise the file picker opens on launch.
    let path = std::env::args_os().nth(1).map(|argument| {
        let path = PathBuf::from(argument);
        std::fs::canonicalize(&path).unwrap_or(path)
    });

    gpui_platform::application().run(move |cx: &mut App| {
        editor::bind_keys(cx);
        workspace::bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.set_menus(menus());

        let bounds = Bounds::centered(None, size(px(900.), px(760.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("mrk".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let window = cx.open_window(options, |window, cx| {
            cx.new(|cx| Workspace::new(window, cx))
        });
        let opened = window.and_then(|window| {
            window.update(cx, |workspace, window, cx| match path {
                Some(path) => workspace.open_path(path, window, cx),
                None => workspace.prompt_to_open(window, cx),
            })
        });
        match opened {
            Ok(()) => cx.activate(true),
            Err(error) => {
                eprintln!("mrk: could not open a window: {error:#}");
                cx.quit();
            }
        }
    });
}

fn menus() -> Vec<Menu> {
    vec![
        Menu::new("mrk").items([MenuItem::action("Quit mrk", Quit)]),
        Menu::new("File").items([
            MenuItem::action("New", NewFile),
            MenuItem::action("Open…", OpenFile),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
            MenuItem::action("Save As…", SaveAs),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("Edit").items([
            MenuItem::action("Undo", edit::Undo),
            MenuItem::action("Redo", edit::Redo),
            MenuItem::separator(),
            MenuItem::action("Cut", edit::Cut),
            MenuItem::action("Copy", edit::Copy),
            MenuItem::action("Paste", edit::Paste),
            MenuItem::action("Select All", edit::SelectAll),
        ]),
        Menu::new("Format").items([
            MenuItem::action("Bold", edit::ToggleBold),
            MenuItem::action("Italic", edit::ToggleItalic),
            MenuItem::action("Strikethrough", edit::ToggleStrikethrough),
            MenuItem::action("Code", edit::ToggleCode),
        ]),
        Menu::new("View").items([
            MenuItem::action("Actual Size", ResetZoom),
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
        ]),
    ]
}
