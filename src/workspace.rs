use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, KeyBinding, MouseButton,
    PathPromptOptions, PromptLevel, SharedString, Subscription, Task, Window, actions, div,
    prelude::*, px,
};

use crate::editor::Editor;
use crate::markdown;
use crate::theme::Theme;

actions!(mrk, [NewFile, OpenFile, Save, SaveAs, CloseWindow, Quit]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewFile, None),
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-shift-s", SaveAs, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
}

const UNTITLED: &str = "Untitled";

/// The window's root view: the single open note, or the welcome screen when there is none.
pub struct Workspace {
    focus_handle: FocusHandle,
    note: Option<Note>,
    /// The title and edited state last given to the window, to only update it on change.
    window_title: Option<(String, bool)>,
    _appearance_observation: Subscription,
}

struct Note {
    editor: Entity<Editor>,
    path: Option<PathBuf>,
    /// The file's content as it was opened, kept only while saving would not reproduce it
    /// byte for byte. It is written to a backup file before the first save overwrites it.
    unreproducible_original: Option<String>,
    _editor_observation: Subscription,
}

impl Focusable for Workspace {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |this, cx| this.confirm_close(window, cx))
                .unwrap_or(true)
        });
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        Self {
            focus_handle,
            note: None,
            window_title: None,
            // The colors follow the system's light or dark appearance.
            _appearance_observation: cx.observe_window_appearance(window, |this, _, cx| {
                if let Some(note) = &this.note {
                    note.editor.update(cx, |_, cx| cx.notify());
                }
                cx.notify();
            }),
        }
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.note
            .as_ref()
            .is_some_and(|note| note.editor.read(cx).is_dirty())
    }

    fn title(&self) -> String {
        match &self.note {
            Some(note) => note
                .path
                .as_deref()
                .and_then(Path::file_name)
                .map_or(UNTITLED.to_string(), |name| {
                    name.to_string_lossy().into_owned()
                }),
            None => "mrk".to_string(),
        }
    }

    fn show_note(
        &mut self,
        path: Option<PathBuf>,
        content: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let document = markdown::parse(&content);
        let reproducible = markdown::serialize(&document) == content;
        let editor = cx.new(|cx| Editor::new(document, cx));
        window.focus(&editor.focus_handle(cx), cx);
        self.note = Some(Note {
            _editor_observation: cx.observe(&editor, |_, _, cx| cx.notify()),
            editor,
            path,
            unreproducible_original: (!reproducible).then_some(content),
        });
        cx.notify();
    }

    fn show_error(
        &mut self,
        message: &str,
        detail: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The prompt only informs; there is nothing to do with its answer.
        drop(window.prompt(PromptLevel::Critical, message, Some(detail), &["OK"], cx));
    }

    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if !is_markdown_path(&path) {
            self.show_error(
                "This file cannot be opened",
                "mrk only opens Markdown files with the .md extension.",
                window,
                cx,
            );
            return;
        }
        let read = cx.background_spawn({
            let path = path.clone();
            async move { std::fs::read_to_string(&path) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let content = read.await;
            this.update_in(cx, |this, window, cx| match content {
                Ok(content) => this.show_note(Some(path), content, window, cx),
                Err(error) => this.show_error(
                    &format!("Could not open {}", path.display()),
                    &error.to_string(),
                    window,
                    cx,
                ),
            })
        })
        .detach_and_log_err(cx);
    }

    /// Shows the system file picker and opens the chosen file.
    pub fn prompt_to_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match paths.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => return Err(error),
            };
            if let Some(path) = path {
                this.update_in(cx, |this, window, cx| this.open_path(path, window, cx))?;
            }
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    /// Asks what to do with unsaved changes, if there are any. Resolves to whether the note
    /// may now be closed or replaced.
    fn confirm_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        if !self.is_dirty(cx) {
            return Task::ready(true);
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Do you want to save the changes made to {}?", self.title()),
            Some("Your changes will be lost if you don't save them."),
            &["Save", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| match answer.await {
            Ok(0) => {
                let save = this.update_in(cx, |this, window, cx| this.save_note(false, window, cx));
                match save {
                    Ok(save) => save.await,
                    Err(_) => false,
                }
            }
            Ok(1) => true,
            _ => false,
        })
    }

    /// Whether the window may close right away. With unsaved changes it may not: the user is
    /// asked first, and the window is closed afterwards if they agree.
    fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.is_dirty(cx) {
            return true;
        }
        let confirmed = self.confirm_discard(window, cx);
        cx.spawn_in(window, async move |_, cx| {
            if confirmed.await {
                cx.update(|window, _| window.remove_window())?;
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
        false
    }

    /// Writes the note to its file, asking for a location when it has none or when
    /// `choose_path` is set. Resolves to whether the note was saved.
    fn save_note(
        &mut self,
        choose_path: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        let Some(note) = &self.note else {
            return Task::ready(false);
        };
        let editor = note.editor.read(cx);
        let revision = editor.revision();
        let content = markdown::serialize(editor.document());
        let known_path = note.path.clone().filter(|_| !choose_path);
        let original = note.unreproducible_original.clone();
        let chosen_path = known_path.is_none().then(|| {
            let directory = note
                .path
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_default();
            let name = match &note.path {
                Some(_) => self.title(),
                None => format!("{UNTITLED}.md"),
            };
            cx.prompt_for_new_path(&directory, Some(&name))
        });

        cx.spawn_in(window, async move |this, cx| {
            let (path, backup) = match (known_path, chosen_path) {
                // Only the file the note was opened from holds content that could be lost.
                (Some(path), _) => (path, original),
                (None, Some(chosen_path)) => match chosen_path.await {
                    Ok(Ok(Some(path))) => (with_markdown_extension(path), None),
                    _ => return false,
                },
                (None, None) => return false,
            };
            let written = cx
                .background_spawn({
                    let path = path.clone();
                    let content = content.clone();
                    async move { write_note(&path, &content, backup.as_deref()) }
                })
                .await;
            this.update_in(cx, |this, window, cx| match written {
                Ok(()) => {
                    if let Some(note) = &mut this.note {
                        // What is on disk now is the canonical form, unless serializing is
                        // not stable for this document, in which case it stays protected.
                        let reparsed = markdown::serialize(&markdown::parse(&content));
                        note.unreproducible_original = (reparsed != content).then_some(content);
                        note.path = Some(path);
                        note.editor
                            .update(cx, |editor, cx| editor.mark_saved(revision, cx));
                    }
                    cx.notify();
                    true
                }
                Err(error) => {
                    this.show_error(
                        &format!("Could not save {}", path.display()),
                        &format!("{error:#}"),
                        window,
                        cx,
                    );
                    false
                }
            })
            .unwrap_or(false)
        })
    }

    fn new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        let confirmed = self.confirm_discard(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if confirmed.await {
                this.update_in(cx, |this, window, cx| {
                    this.show_note(None, String::new(), window, cx)
                })?;
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let confirmed = self.confirm_discard(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if confirmed.await {
                this.update_in(cx, |this, window, cx| this.prompt_to_open(window, cx))?;
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.save_note(false, window, cx).detach();
    }

    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        self.save_note(true, window, cx).detach();
    }

    fn close_window(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirm_close(window, cx) {
            window.remove_window();
        }
    }

    fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        let confirmed = self.confirm_discard(window, cx);
        cx.spawn_in(window, async move |_, cx| {
            if confirmed.await {
                cx.update(|_, cx| cx.quit())?;
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn render_welcome(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let button = |label: &'static str, shortcut: &'static str| {
            div()
                .id(SharedString::from(label))
                .flex()
                .flex_row()
                .justify_between()
                .gap(px(32.))
                .px(px(12.))
                .py(px(6.))
                .rounded(px(6.))
                .border_1()
                .border_color(theme.faint)
                .cursor_pointer()
                .hover(|style| style.bg(theme.code_background))
                .child(label)
                .child(div().text_color(theme.muted).child(shortcut))
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .bg(theme.background)
            .text_color(theme.text)
            .text_size(px(14.))
            .child(div().text_size(px(28.)).child("mrk"))
            .child(
                div()
                    .mb(px(12.))
                    .text_color(theme.muted)
                    .child("Open a Markdown file to start."),
            )
            .child(button("Open…", "⌘O").w(px(220.)).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.open_file(&OpenFile, window, cx)),
            ))
            .child(button("New note", "⌘N").w(px(220.)).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.new_file(&NewFile, window, cx)),
            ))
            .into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = (self.title(), self.is_dirty(cx));
        if self.window_title.as_ref() != Some(&title) {
            window.set_window_title(&title.0);
            window.set_window_edited(title.1);
            self.window_title = Some(title);
        }
        let theme = Theme::for_appearance(window.appearance());
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .on_action(cx.listener(Self::new_file))
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::close_window))
            .on_action(cx.listener(Self::quit))
            .child(match &self.note {
                Some(note) => note.editor.clone().into_any_element(),
                None => self.render_welcome(&theme, cx),
            })
    }
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn with_markdown_extension(path: PathBuf) -> PathBuf {
    if is_markdown_path(&path) {
        path
    } else {
        let mut name = path.clone().into_os_string();
        name.push(".md");
        PathBuf::from(name)
    }
}

/// Writes `content` to `path`, first saving `backup` (the content the file had when it was
/// opened) next to it. The file is replaced in one step, so a failure part-way leaves the
/// previous version in place.
fn write_note(path: &Path, content: &str, backup: Option<&str>) -> Result<()> {
    // A symlink is written through, not replaced.
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(original) = backup
        && let Some(backup_path) = backup_path(&path, original)?
    {
        std::fs::write(&backup_path, original)
            .with_context(|| format!("writing the backup {}", backup_path.display()))?;
    }

    let file_name = path
        .file_name()
        .with_context(|| format!("{} is not a file", path.display()))?
        .to_string_lossy();
    let temporary_path = path.with_file_name(format!(".{file_name}.mrk-tmp"));
    std::fs::write(&temporary_path, content)
        .with_context(|| format!("writing {}", temporary_path.display()))?;
    if let Ok(metadata) = std::fs::metadata(&path) {
        std::fs::set_permissions(&temporary_path, metadata.permissions())
            .with_context(|| format!("setting permissions of {}", temporary_path.display()))?;
    }
    std::fs::rename(&temporary_path, &path)
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// Where to back up `original`: `<name>.bak`, or `<name>.<n>.bak` when earlier backups with
/// different content are in the way. `None` means an identical backup already exists.
fn backup_path(path: &Path, original: &str) -> Result<Option<PathBuf>> {
    let name = path
        .file_name()
        .with_context(|| format!("{} is not a file", path.display()))?
        .to_string_lossy();
    for index in 0.. {
        let candidate = if index == 0 {
            path.with_file_name(format!("{name}.bak"))
        } else {
            path.with_file_name(format!("{name}.{index}.bak"))
        };
        match std::fs::read(&candidate) {
            Ok(existing) if existing == original.as_bytes() => return Ok(None),
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Some(candidate));
            }
            Err(error) => {
                return Err(error).with_context(|| format!("reading {}", candidate.display()));
            }
        }
    }
    unreachable!("the loop only ends by returning")
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;

    /// Opens a window with a workspace, as the application does on launch.
    fn open_workspace(cx: &mut TestAppContext) -> (Entity<Workspace>, VisualTestContext) {
        cx.update(|cx| {
            crate::editor::bind_keys(cx);
            bind_keys(cx);
        });
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                cx.new(|cx| Workspace::new(window, cx))
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = window.root(&mut cx).unwrap();
        (workspace, cx)
    }

    fn open_file(path: &Path, workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_path(path.to_path_buf(), window, cx)
        });
        cx.run_until_parked();
    }

    fn files_in(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[gpui::test]
    fn saving_a_file_in_another_style_backs_it_up_first(cx: &mut TestAppContext) {
        let directory = temporary_directory("save-other-style");
        let path = directory.join("note.md");
        std::fs::write(&path, "* one\n* two\n").unwrap();
        let (workspace, mut cx) = open_workspace(cx);
        open_file(&path, &workspace, &mut cx);
        assert_eq!(cx.window_title().as_deref(), Some("note.md"));

        cx.simulate_keystrokes("down enter");
        cx.simulate_input("!");
        cx.simulate_keystrokes("cmd-s");
        cx.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "- one!\n- two\n");
        assert_eq!(
            std::fs::read_to_string(directory.join("note.md.bak")).unwrap(),
            "* one\n* two\n"
        );
        assert!(!workspace.read_with(&cx, |workspace, cx| workspace.is_dirty(cx)));

        // What is on disk now is in the canonical style, so saving again needs no backup.
        cx.simulate_input("?");
        cx.simulate_keystrokes("cmd-s");
        cx.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "- one!?\n- two\n");
        assert_eq!(files_in(&directory), ["note.md", "note.md.bak"]);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[gpui::test]
    fn saving_a_canonical_file_makes_no_backup(cx: &mut TestAppContext) {
        let directory = temporary_directory("save-canonical");
        let path = directory.join("note.md");
        std::fs::write(&path, "# Title\n\n- one\n- two\n").unwrap();
        let (workspace, mut cx) = open_workspace(cx);
        open_file(&path, &workspace, &mut cx);

        cx.simulate_keystrokes("down enter");
        cx.simulate_input("!");
        cx.simulate_keystrokes("cmd-s");
        cx.run_until_parked();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Title!\n\n- one\n- two\n"
        );
        assert_eq!(files_in(&directory), ["note.md"]);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[gpui::test]
    fn a_new_note_asks_where_to_save(cx: &mut TestAppContext) {
        let directory = temporary_directory("save-new");
        let (workspace, mut cx) = open_workspace(cx);
        cx.simulate_keystrokes("cmd-n");
        cx.run_until_parked();
        assert_eq!(cx.window_title().as_deref(), Some("Untitled"));

        cx.simulate_keystrokes("enter");
        cx.simulate_input("hello");
        cx.simulate_keystrokes("cmd-s");
        cx.run_until_parked();
        assert!(cx.did_prompt_for_new_path());
        cx.simulate_new_path_selection(|_| Some(directory.join("fresh")));
        cx.run_until_parked();

        assert_eq!(
            std::fs::read_to_string(directory.join("fresh.md")).unwrap(),
            "hello\n"
        );
        assert_eq!(cx.window_title().as_deref(), Some("fresh.md"));
        assert!(!workspace.read_with(&cx, |workspace, cx| workspace.is_dirty(cx)));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[gpui::test]
    fn unsaved_changes_are_confirmed_before_they_are_replaced(cx: &mut TestAppContext) {
        let directory = temporary_directory("confirm");
        let path = directory.join("note.md");
        std::fs::write(&path, "text\n").unwrap();
        let (workspace, mut cx) = open_workspace(cx);
        open_file(&path, &workspace, &mut cx);
        cx.simulate_keystrokes("down enter");
        cx.simulate_input("!");

        cx.simulate_keystrokes("cmd-n");
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert_eq!(cx.window_title().as_deref(), Some("note.md"));
        assert!(workspace.read_with(&cx, |workspace, cx| workspace.is_dirty(cx)));

        cx.simulate_keystrokes("cmd-n");
        cx.run_until_parked();
        cx.simulate_prompt_answer("Save");
        cx.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "text!\n");
        assert_eq!(cx.window_title().as_deref(), Some("Untitled"));

        cx.simulate_keystrokes("enter");
        cx.simulate_input("x");
        cx.simulate_keystrokes("cmd-n");
        cx.run_until_parked();
        cx.simulate_prompt_answer("Don't Save");
        cx.run_until_parked();
        assert!(!workspace.read_with(&cx, |workspace, cx| workspace.is_dirty(cx)));
        assert_eq!(files_in(&directory), ["note.md"]);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[gpui::test]
    fn only_md_files_can_be_opened(cx: &mut TestAppContext) {
        let directory = temporary_directory("extension");
        let path = directory.join("note.txt");
        std::fs::write(&path, "text\n").unwrap();
        let (workspace, mut cx) = open_workspace(cx);
        open_file(&path, &workspace, &mut cx);
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("OK");
        assert!(workspace.read_with(&cx, |workspace, _| workspace.note.is_none()));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    fn temporary_directory(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("mrk-test-{name}-{}", std::process::id()));
        if directory.exists() {
            std::fs::remove_dir_all(&directory).unwrap();
        }
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn saving_replaces_the_file_and_backs_up_the_original_once() {
        let directory = temporary_directory("backup");
        let path = directory.join("note.md");
        std::fs::write(&path, "* original\n").unwrap();

        write_note(&path, "- original\n", Some("* original\n")).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "- original\n");
        assert_eq!(
            std::fs::read_to_string(directory.join("note.md.bak")).unwrap(),
            "* original\n"
        );

        // The same original is not backed up twice.
        write_note(&path, "- edited\n", Some("* original\n")).unwrap();
        assert!(!directory.join("note.md.1.bak").exists());

        // A different original does not overwrite the earlier backup.
        write_note(&path, "- again\n", Some("+ other\n")).unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.join("note.md.bak")).unwrap(),
            "* original\n"
        );
        assert_eq!(
            std::fs::read_to_string(directory.join("note.md.1.bak")).unwrap(),
            "+ other\n"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "- again\n");

        let leftovers: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".mrk-tmp"))
            .collect();
        assert!(leftovers.is_empty());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn saving_without_a_backup_creates_no_extra_files() {
        let directory = temporary_directory("plain");
        let path = directory.join("note.md");
        write_note(&path, "text\n", None).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "text\n");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn only_md_files_are_markdown() {
        assert!(is_markdown_path(Path::new("note.md")));
        assert!(is_markdown_path(Path::new("NOTE.MD")));
        assert!(!is_markdown_path(Path::new("note.txt")));
        assert!(!is_markdown_path(Path::new("md")));
        assert_eq!(
            with_markdown_extension(PathBuf::from("notes/todo")),
            PathBuf::from("notes/todo.md")
        );
        assert_eq!(
            with_markdown_extension(PathBuf::from("notes/todo.md")),
            PathBuf::from("notes/todo.md")
        );
    }
}
