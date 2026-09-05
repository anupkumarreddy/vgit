mod demo;
mod graph;
mod theme;

use demo::{COMMITS, FileChange};
use gpui::{
    App, Application, Bounds, Context, Div, FocusHandle, FontWeight, KeyBinding, SharedString,
    Stateful, TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div, prelude::*, px,
    rgb, size,
};
use theme::{Palette, Theme, badge, column, row, section_label};

actions!(
    vgit,
    [
        Quit,
        Close,
        NextCommit,
        PreviousCommit,
        ShowDiff,
        ShowSource,
        ToggleStage,
        ToggleSettings,
        Escape
    ]
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorView {
    Diff,
    Source,
}

struct Workspace {
    focus: FocusHandle,
    history_scroll: gpui::ScrollHandle,
    theme: Theme,
    editor_view: EditorView,
    commit: usize,
    file: usize,
    files: Vec<FileChange>,
    settings_open: bool,
    message: String,
}

fn button(
    colors: Palette,
    id: impl Into<gpui::ElementId>,
    text: impl Into<String>,
) -> Stateful<Div> {
    row()
        .id(id)
        .h(px(26.))
        .px_2()
        .gap_2()
        .rounded(px(4.))
        .border_1()
        .border_color(rgb(colors.line_strong))
        .bg(rgb(colors.elevated))
        .text_color(rgb(colors.text))
        .text_size(px(11.))
        .cursor_pointer()
        .hover(move |this| this.bg(rgb(colors.hover)))
        .active(|this| this.opacity(0.8))
        .child(text.into())
}

fn icon_button(
    colors: Palette,
    id: impl Into<gpui::ElementId>,
    symbol: &'static str,
    selected: bool,
) -> Stateful<Div> {
    row()
        .id(id)
        .size(px(34.))
        .justify_center()
        .rounded(px(4.))
        .text_size(px(17.))
        .text_color(rgb(if selected {
            colors.text_bright
        } else {
            colors.muted
        }))
        .bg(rgb(if selected {
            colors.hover
        } else {
            colors.activity
        }))
        .cursor_pointer()
        .hover(move |this| {
            this.bg(rgb(colors.hover))
                .text_color(rgb(colors.text_bright))
        })
        .child(symbol)
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        Self {
            focus,
            history_scroll: gpui::ScrollHandle::new(),
            theme: Theme::Dark,
            editor_view: EditorView::Diff,
            commit: 0,
            file: 0,
            files: demo::files(),
            settings_open: false,
            message: "main*  ·  3 changes  ·  2 staged".into(),
        }
    }

    fn colors(&self) -> Palette {
        self.theme.palette()
    }

    fn select_commit(&mut self, index: usize, cx: &mut Context<Self>) {
        self.commit = index;
        self.file = COMMITS[index].file;
        self.history_scroll.scroll_to_item(index);
        self.message = format!("{}  ·  {}", COMMITS[index].hash, COMMITS[index].subject);
        cx.notify();
    }

    fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        self.file = index;
        self.editor_view = EditorView::Diff;
        self.message = format!("{}  ·  working tree", self.files[index].path);
        cx.notify();
    }

    fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.message = format!("Appearance changed to {}", theme.label());
        cx.notify();
    }

    fn toggle_stage(&mut self, index: usize, cx: &mut Context<Self>) {
        let file = &mut self.files[index];
        file.staged = !file.staged;
        self.message = format!(
            "{}  ·  {}",
            file.path,
            if file.staged { "staged" } else { "unstaged" }
        );
        cx.notify();
    }

    fn titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        row()
            .h(px(36.))
            .flex_none()
            .pl(px(76.))
            .pr_2()
            .gap_2()
            .bg(rgb(colors.titlebar))
            .border_b_1()
            .border_color(rgb(colors.line))
            .child(
                row()
                    .w(px(252.))
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(rgb(colors.text_bright))
                    .child(div().text_color(rgb(colors.local)).child("◇"))
                    .child("vgit")
                    .child(div().text_color(rgb(colors.dim)).child("—"))
                    .child(
                        div()
                            .text_color(rgb(colors.muted))
                            .child("design-workspace"),
                    ),
            )
            .child(
                row()
                    .flex_1()
                    .max_w(px(470.))
                    .h(px(24.))
                    .px_3()
                    .justify_center()
                    .rounded(px(5.))
                    .border_1()
                    .border_color(rgb(colors.line_strong))
                    .bg(rgb(colors.elevated))
                    .text_size(px(11.))
                    .text_color(rgb(colors.muted))
                    .child("design-workspace   ·   e7a91c2   ·   main"),
            )
            .child(div().flex_1())
            .child(
                button(colors, "fetch", "↓  Fetch").on_click(cx.listener(|this, _, _, cx| {
                    this.message = "origin/main is up to date · demo only".into();
                    cx.notify();
                })),
            )
            .child(button(colors, "push", "↑  Push 1"))
    }

    fn activity_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        column()
            .w(px(46.))
            .flex_none()
            .h_full()
            .items_center()
            .py_2()
            .gap_1()
            .bg(rgb(colors.activity))
            .border_r_1()
            .border_color(rgb(colors.line))
            .child(icon_button(colors, "activity-source", "⑂", true))
            .child(icon_button(colors, "activity-search", "⌕", false))
            .child(icon_button(colors, "activity-files", "▱", false))
            .child(icon_button(colors, "activity-remote", "◎", false))
            .child(div().flex_1())
            .child(
                icon_button(colors, "activity-settings", "⚙", self.settings_open).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.settings_open = !this.settings_open;
                        cx.notify();
                    }),
                ),
            )
    }

    fn graph_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        column()
            .w(px(420.))
            .flex_none()
            .h_full()
            .min_h_0()
            .bg(rgb(colors.sidebar))
            .border_r_1()
            .border_color(rgb(colors.line))
            .child(
                column()
                    .h(px(74.))
                    .flex_none()
                    .px_3()
                    .pt_3()
                    .gap_2()
                    .child(
                        row()
                            .justify_between()
                            .child(section_label(colors, "SOURCE CONTROL GRAPH"))
                            .child(
                                div()
                                    .text_color(rgb(colors.muted))
                                    .text_size(px(14.))
                                    .child("⋯"),
                            ),
                    )
                    .child(
                        row()
                            .h(px(28.))
                            .px_2()
                            .gap_2()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(rgb(colors.line))
                            .bg(rgb(colors.editor))
                            .text_size(px(11.))
                            .text_color(rgb(colors.muted))
                            .child("⌕")
                            .child("Filter commits, authors, refs"),
                    ),
            )
            .child(
                row()
                    .h(px(28.))
                    .flex_none()
                    .px_3()
                    .border_y_1()
                    .border_color(rgb(colors.line))
                    .child(section_label(colors, "GRAPH & COMMIT"))
                    .child(div().flex_1())
                    .child(section_label(colors, "WHEN")),
            )
            .child(
                column()
                    .id("graph-history")
                    .track_scroll(&self.history_scroll)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .relative()
                    .children(COMMITS.iter().enumerate().map(|(index, commit)| {
                        let selected = self.commit == index;
                        let ref_color = if commit.reference.contains("origin/") {
                            colors.remote
                        } else if commit.reference.starts_with('v') {
                            colors.tag
                        } else if commit.parents.len() > 1 {
                            colors.merge
                        } else {
                            colors.local
                        };
                        row()
                            .id(("commit-row", index))
                            .h(px(graph::ROW_HEIGHT))
                            .flex_none()
                            .pl(px(graph::GRAPH_WIDTH))
                            .pr_2()
                            .gap_2()
                            .cursor_pointer()
                            .bg(rgb(if selected {
                                colors.selection
                            } else {
                                colors.sidebar
                            }))
                            .hover(move |this| this.bg(rgb(colors.hover)))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(10.))
                                    .text_color(rgb(colors.dim))
                                    .child(commit.hash),
                            )
                            .when(!commit.reference.is_empty(), |this| {
                                this.child(badge(colors, commit.reference, ref_color))
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(rgb(if selected {
                                        colors.text_bright
                                    } else {
                                        colors.text
                                    }))
                                    .child(commit.subject),
                            )
                            .child(
                                div()
                                    .w(px(46.))
                                    .flex_none()
                                    .text_right()
                                    .text_size(px(9.))
                                    .text_color(rgb(colors.dim))
                                    .child(commit.time),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_commit(index, cx);
                            }))
                    }))
                    .child(graph::sidebar_graph(colors)),
            )
            .child(
                column()
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .child(
                        row()
                            .h(px(30.))
                            .px_3()
                            .gap_2()
                            .text_size(px(11.))
                            .text_color(rgb(colors.text))
                            .child(div().text_color(rgb(colors.local)).child("⑂"))
                            .child("main")
                            .child(div().flex_1())
                            .child(div().text_color(rgb(colors.local)).child("1↑")),
                    )
                    .child(
                        row()
                            .h(px(28.))
                            .px_3()
                            .gap_3()
                            .text_size(px(10.))
                            .text_color(rgb(colors.dim))
                            .child(
                                row()
                                    .gap_1()
                                    .child(div().text_color(rgb(colors.local)).child("●"))
                                    .child("Local"),
                            )
                            .child(
                                row()
                                    .gap_1()
                                    .child(div().text_color(rgb(colors.remote)).child("●"))
                                    .child("Remote"),
                            )
                            .child(
                                row()
                                    .gap_1()
                                    .child(div().text_color(rgb(colors.merge)).child("○"))
                                    .child("Merge"),
                            ),
                    ),
            )
    }

    fn editor_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let file = &self.files[self.file];
        row()
            .h(px(35.))
            .flex_none()
            .bg(rgb(colors.editor_alt))
            .border_b_1()
            .border_color(rgb(colors.line))
            .child(
                row()
                    .id("diff-tab")
                    .h_full()
                    .px_3()
                    .gap_2()
                    .border_r_1()
                    .border_color(rgb(colors.line))
                    .when(self.editor_view == EditorView::Diff, |this| {
                        this.border_t_1()
                            .border_color(rgb(colors.local))
                            .bg(rgb(colors.editor))
                    })
                    .text_size(px(11.))
                    .text_color(rgb(colors.text))
                    .cursor_pointer()
                    .child(div().text_color(rgb(colors.local)).child("M"))
                    .child(format!(
                        "{} (Working Tree)",
                        file.path.rsplit('/').next().unwrap_or("file")
                    ))
                    .child(div().text_color(rgb(colors.dim)).child("×"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.editor_view = EditorView::Diff;
                        cx.notify();
                    })),
            )
            .child(
                row()
                    .id("source-tab")
                    .h_full()
                    .px_3()
                    .gap_2()
                    .border_r_1()
                    .border_color(rgb(colors.line))
                    .when(self.editor_view == EditorView::Source, |this| {
                        this.border_t_1()
                            .border_color(rgb(colors.remote))
                            .bg(rgb(colors.editor))
                    })
                    .text_size(px(11.))
                    .text_color(rgb(colors.text))
                    .cursor_pointer()
                    .child(div().text_color(rgb(colors.remote)).child("RS"))
                    .child("workspace.rs")
                    .child(div().text_color(rgb(colors.dim)).child("×"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.editor_view = EditorView::Source;
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
    }

    fn breadcrumb(&self) -> impl IntoElement {
        let colors = self.colors();
        let file = &self.files[self.file];
        row()
            .h(px(28.))
            .flex_none()
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(rgb(colors.line))
            .bg(rgb(colors.editor))
            .text_size(px(10.))
            .text_color(rgb(colors.muted))
            .children(file.path.split('/').enumerate().flat_map(|(index, part)| {
                let mut elements = Vec::new();
                if index > 0 {
                    elements.push(div().text_color(rgb(colors.dim)).child("›"));
                }
                elements.push(div().child(part.to_string()));
                elements
            }))
            .child(div().text_color(rgb(colors.dim)).child("›"))
            .child(if self.editor_view == EditorView::Diff {
                "Working Tree"
            } else {
                "Source"
            })
    }

    fn code_line(
        &self,
        line_number: usize,
        kind: &'static str,
        code: &'static str,
    ) -> impl IntoElement {
        let colors = self.colors();
        row()
            .h(px(23.))
            .flex_none()
            .font_family("Menlo")
            .text_size(px(11.))
            .bg(rgb(match kind {
                "+" => colors.added_bg,
                "-" => colors.removed_bg,
                _ => colors.editor,
            }))
            .child(
                div()
                    .w(px(50.))
                    .flex_none()
                    .text_right()
                    .pr_3()
                    .text_color(rgb(colors.code_gutter))
                    .child(line_number.to_string()),
            )
            .child(
                div()
                    .w(px(22.))
                    .flex_none()
                    .text_color(rgb(if kind == "-" {
                        colors.red
                    } else {
                        colors.local
                    }))
                    .child(kind),
            )
            .child(
                div()
                    .whitespace_nowrap()
                    .text_color(rgb(match kind {
                        "+" => colors.local,
                        "-" => colors.red,
                        _ => colors.text,
                    }))
                    .child(code),
            )
    }

    fn diff_editor(&self) -> impl IntoElement {
        let colors = self.colors();
        let file = &self.files[self.file];
        column()
            .id("diff-editor")
            .flex_1()
            .min_h_0()
            .overflow_scroll()
            .bg(rgb(colors.editor))
            .child(
                row()
                    .h(px(38.))
                    .flex_none()
                    .px_4()
                    .gap_3()
                    .bg(rgb(colors.editor_alt))
                    .border_b_1()
                    .border_color(rgb(colors.line))
                    .child(section_label(colors, "WORKING TREE ↔ INDEX"))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_color(rgb(colors.local))
                            .text_size(px(11.))
                            .child(format!("+{}", file.added)),
                    )
                    .child(
                        div()
                            .text_color(rgb(colors.red))
                            .text_size(px(11.))
                            .child(format!("−{}", file.removed)),
                    ),
            )
            .child(
                div()
                    .h(px(28.))
                    .flex_none()
                    .px_4()
                    .py_1()
                    .font_family("Menlo")
                    .text_size(px(10.))
                    .text_color(rgb(colors.remote))
                    .child("@@ -18,10 +18,12 @@ impl Render for Workspace"),
            )
            .children(
                file.patch
                    .iter()
                    .enumerate()
                    .map(|(offset, &(kind, code))| self.code_line(offset + 18, kind, code)),
            )
    }

    fn source_editor(&self) -> impl IntoElement {
        let colors = self.colors();
        let source = [
            (1, "use gpui::{Context, IntoElement, Render};"),
            (2, "use crate::{graph::HistoryGraph, theme::Theme};"),
            (3, ""),
            (4, "pub struct Workspace {"),
            (5, "    graph: HistoryGraph,"),
            (6, "    theme: Theme,"),
            (7, "    selected_commit: usize,"),
            (8, "}"),
            (9, ""),
            (10, "impl Render for Workspace {"),
            (
                11,
                "    fn render(&mut self, cx: &mut Context<Self>) -> impl IntoElement {",
            ),
            (12, "        let colors = self.theme.palette();"),
            (13, ""),
            (14, "        workspace_shell(colors)"),
            (15, "            .left(self.graph.render(cx))"),
            (16, "            .center(self.diff_editor(cx))"),
            (17, "            .right(self.repository_state(cx))"),
            (18, "    }"),
            (19, "}"),
            (20, ""),
            (
                21,
                "// The interface is native Rust and all data is still fictional.",
            ),
        ];
        column()
            .id("source-editor")
            .flex_1()
            .min_h_0()
            .overflow_scroll()
            .bg(rgb(colors.editor))
            .py_2()
            .children(source.map(|(line, code)| self.code_line(line, " ", code)))
    }

    fn editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        column()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(rgb(colors.editor))
            .child(self.editor_tabs(cx))
            .child(self.breadcrumb())
            .child(match self.editor_view {
                EditorView::Diff => self.diff_editor().into_any_element(),
                EditorView::Source => self.source_editor().into_any_element(),
            })
    }

    fn file_row(&self, index: usize, staged: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let file = &self.files[index];
        row()
            .id((
                if staged {
                    "staged-file"
                } else {
                    "changed-file"
                },
                index,
            ))
            .h(px(26.))
            .px_2()
            .gap_2()
            .cursor_pointer()
            .bg(rgb(if self.file == index {
                colors.selection
            } else {
                colors.panel
            }))
            .hover(move |this| this.bg(rgb(colors.hover)))
            .child(
                div()
                    .w(px(13.))
                    .flex_none()
                    .text_size(px(10.))
                    .text_color(rgb(if file.status == "M" {
                        colors.remote
                    } else {
                        colors.local
                    }))
                    .child(file.status),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.))
                    .text_color(rgb(colors.text))
                    .child(file.path.rsplit('/').next().unwrap_or(file.path)),
            )
            .child(
                div().text_size(px(9.)).text_color(rgb(colors.dim)).child(
                    file.path
                        .rsplit_once('/')
                        .map(|value| value.0)
                        .unwrap_or(""),
                ),
            )
            .child(
                div()
                    .id(("stage-toggle", index))
                    .w(px(18.))
                    .text_center()
                    .text_size(px(14.))
                    .text_color(rgb(colors.muted))
                    .cursor_pointer()
                    .child(if staged { "−" } else { "+" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_stage(index, cx);
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_file(index, cx);
            }))
    }

    fn repository_state(&self) -> impl IntoElement {
        let colors = self.colors();
        let commit = &COMMITS[self.commit];
        column()
            .child(
                row()
                    .h(px(26.))
                    .px_3()
                    .justify_between()
                    .child(section_label(colors, "REPOSITORY STATE"))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(colors.dim))
                            .child("⌃"),
                    ),
            )
            .child(
                column()
                    .px_3()
                    .pb_3()
                    .gap_2()
                    .child(
                        row()
                            .text_size(px(11.))
                            .child(div().w(px(74.)).text_color(rgb(colors.muted)).child("HEAD"))
                            .child(div().text_color(rgb(colors.local)).child("main")),
                    )
                    .child(
                        row()
                            .text_size(px(11.))
                            .child(
                                div()
                                    .w(px(74.))
                                    .text_color(rgb(colors.muted))
                                    .child("Upstream"),
                            )
                            .child(div().text_color(rgb(colors.remote)).child("origin/main")),
                    )
                    .child(
                        row()
                            .text_size(px(11.))
                            .child(
                                div()
                                    .w(px(74.))
                                    .text_color(rgb(colors.muted))
                                    .child("Status"),
                            )
                            .child(
                                div()
                                    .text_color(rgb(colors.merge))
                                    .child("1 ahead · 0 behind"),
                            ),
                    )
                    .child(
                        row()
                            .text_size(px(11.))
                            .child(
                                div()
                                    .w(px(74.))
                                    .text_color(rgb(colors.muted))
                                    .child("Commit"),
                            )
                            .child(commit.hash),
                    )
                    .child(
                        column()
                            .mt_2()
                            .pt_2()
                            .gap_1()
                            .border_t_1()
                            .border_color(rgb(colors.line))
                            .child(section_label(colors, "SELECTED COMMIT"))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(colors.text))
                                    .child(commit.subject),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(colors.dim))
                                    .child(format!("{} · {}", commit.author, commit.description)),
                            ),
                    ),
            )
    }

    fn right_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let staged_count = self.files.iter().filter(|file| file.staged).count();
        let changed_count = self.files.len() - staged_count;
        let changed_rows = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| !file.staged)
            .map(|(index, _)| self.file_row(index, false, cx).into_any_element())
            .collect::<Vec<_>>();
        let staged_rows = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| file.staged)
            .map(|(index, _)| self.file_row(index, true, cx).into_any_element())
            .collect::<Vec<_>>();
        column()
            .id("repository-sidebar")
            .w(px(320.))
            .flex_none()
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .bg(rgb(colors.panel))
            .border_l_1()
            .border_color(rgb(colors.line))
            .child(
                row()
                    .h(px(35.))
                    .flex_none()
                    .px_3()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(colors.line))
                    .child(section_label(colors, "SOURCE CONTROL"))
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(rgb(colors.muted))
                            .child("⋯"),
                    ),
            )
            .child(self.repository_state())
            .child(div().h(px(1.)).bg(rgb(colors.line)))
            .child(
                row()
                    .h(px(28.))
                    .px_3()
                    .gap_2()
                    .child(div().text_color(rgb(colors.muted)).child("⌄"))
                    .child(section_label(colors, format!("CHANGES  {changed_count}")))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(15.))
                            .text_color(rgb(colors.muted))
                            .child("＋"),
                    ),
            )
            .children(changed_rows)
            .child(
                row()
                    .h(px(28.))
                    .px_3()
                    .gap_2()
                    .mt_2()
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .child(div().text_color(rgb(colors.muted)).child("⌄"))
                    .child(section_label(
                        colors,
                        format!("STAGED CHANGES  {staged_count}"),
                    ))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(15.))
                            .text_color(rgb(colors.muted))
                            .child("−"),
                    ),
            )
            .children(staged_rows)
            .child(
                column()
                    .mt_2()
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .child(
                        row()
                            .h(px(28.))
                            .px_3()
                            .gap_2()
                            .child(div().text_color(rgb(colors.muted)).child("⌄"))
                            .child(section_label(colors, "SOURCE FILE TREE")),
                    )
                    .children(
                        [
                            ("⌄", "src", colors.text),
                            ("  ⌄", "ui", colors.text),
                            ("    RS", "workspace.rs", colors.remote),
                            ("    RS", "theme.rs", colors.remote),
                            ("  ⌄", "graph", colors.text),
                            ("    RS", "renderer.rs", colors.remote),
                            ("  RS", "core/session.rs", colors.remote),
                            ("⌄", "docs", colors.text),
                            ("  #", "design-notes.md", colors.muted),
                        ]
                        .map(|(prefix, name, color)| {
                            row()
                                .h(px(24.))
                                .px_3()
                                .gap_2()
                                .text_size(px(11.))
                                .text_color(rgb(color))
                                .child(div().font_family("Menlo").text_size(px(9.)).child(prefix))
                                .child(name)
                        }),
                    ),
            )
            .child(div().flex_1())
            .child(
                column()
                    .p_3()
                    .gap_2()
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .child(
                        div()
                            .h(px(52.))
                            .p_2()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(rgb(colors.line_strong))
                            .bg(rgb(colors.editor))
                            .text_size(px(11.))
                            .text_color(rgb(colors.dim))
                            .child("Message (Ctrl+Enter to commit)"),
                    )
                    .child(
                        row()
                            .h(px(26.))
                            .justify_center()
                            .rounded(px(4.))
                            .bg(rgb(colors.local))
                            .text_color(rgb(colors.editor))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(11.))
                            .child("Commit  ·  demo"),
                    ),
            )
    }

    fn settings_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        column()
            .id("settings-panel")
            .absolute()
            .left(px(50.))
            .bottom(px(30.))
            .w(px(266.))
            .p_3()
            .gap_3()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(colors.line_strong))
            .bg(rgb(colors.elevated))
            .shadow_lg()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(|_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                row()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(12.))
                            .text_color(rgb(colors.text_bright))
                            .child("Settings"),
                    )
                    .child(
                        div()
                            .id("close-settings")
                            .px_2()
                            .text_color(rgb(colors.muted))
                            .cursor_pointer()
                            .child("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(div().h(px(1.)).bg(rgb(colors.line)))
            .child(section_label(colors, "APPEARANCE"))
            .child(
                row()
                    .gap_2()
                    .children([Theme::Dark, Theme::Light].map(|theme| {
                        let selected = self.theme == theme;
                        column()
                            .id(SharedString::from(format!("theme-{}", theme.label())))
                            .flex_1()
                            .p_2()
                            .gap_2()
                            .rounded(px(5.))
                            .border_1()
                            .border_color(rgb(if selected {
                                colors.local
                            } else {
                                colors.line_strong
                            }))
                            .bg(rgb(if selected {
                                colors.selection
                            } else {
                                colors.panel
                            }))
                            .cursor_pointer()
                            .child(
                                column()
                                    .h(px(44.))
                                    .rounded(px(3.))
                                    .overflow_hidden()
                                    .bg(rgb(theme.palette().editor))
                                    .child(row().h(px(10.)).bg(rgb(theme.palette().titlebar)))
                                    .child(
                                        row()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .w(px(18.))
                                                    .h_full()
                                                    .bg(rgb(theme.palette().sidebar)),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .h_full()
                                                    .bg(rgb(theme.palette().editor)),
                                            ),
                                    ),
                            )
                            .child(
                                row()
                                    .justify_between()
                                    .text_size(px(11.))
                                    .text_color(rgb(colors.text))
                                    .child(theme.label())
                                    .when(selected, |this| {
                                        this.child(div().text_color(rgb(colors.local)).child("✓"))
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_theme(theme, cx);
                            }))
                    })),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(colors.dim))
                    .child("Theme is kept for this preview session."),
            )
    }

    fn statusbar(&self) -> impl IntoElement {
        let colors = self.colors();
        row()
            .h(px(22.))
            .flex_none()
            .px_2()
            .gap_3()
            .bg(rgb(colors.local))
            .text_color(rgb(colors.editor))
            .text_size(px(10.))
            .child("⑂ main*")
            .child("↻")
            .child("ⓧ 0")
            .child("△ 0")
            .child(div().flex_1().child(self.message.clone()))
            .child("Rust")
            .child("UTF-8")
            .child("LF")
            .child("GPUI")
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        column()
            .id("workspace")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(colors.app))
            .font_family(".SystemUIFont")
            .text_color(rgb(colors.text))
            .text_size(px(12.))
            .on_action(cx.listener(|this, _: &NextCommit, _, cx| {
                this.select_commit((this.commit + 1) % COMMITS.len(), cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousCommit, _, cx| {
                this.select_commit((this.commit + COMMITS.len() - 1) % COMMITS.len(), cx);
            }))
            .on_action(cx.listener(|this, _: &ShowDiff, _, cx| {
                this.editor_view = EditorView::Diff;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ShowSource, _, cx| {
                this.editor_view = EditorView::Source;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleStage, _, cx| {
                this.toggle_stage(this.file, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSettings, _, cx| {
                this.settings_open = !this.settings_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                this.settings_open = false;
                cx.notify();
            }))
            .on_action(|_: &Close, window, _| window.remove_window())
            .child(self.titlebar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.activity_bar(cx))
                    .child(self.graph_sidebar(cx))
                    .child(self.editor(cx))
                    .child(self.right_sidebar(cx)),
            )
            .child(self.statusbar())
            .when(self.settings_open, |this| {
                this.child(self.settings_panel(cx))
            })
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-w", Close, None),
            KeyBinding::new("down", NextCommit, None),
            KeyBinding::new("up", PreviousCommit, None),
            KeyBinding::new("cmd-1", ShowDiff, None),
            KeyBinding::new("cmd-2", ShowSource, None),
            KeyBinding::new("ctrl-1", ShowDiff, None),
            KeyBinding::new("ctrl-2", ShowSource, None),
            KeyBinding::new("space", ToggleStage, None),
            KeyBinding::new("cmd-comma", ToggleSettings, None),
            KeyBinding::new("ctrl-comma", ToggleSettings, None),
            KeyBinding::new("escape", Escape, None),
        ]);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1440.), px(900.)),
                    cx,
                ))),
                window_min_size: Some(size(px(1080.), px(700.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("VGit — Native Preview".into()),
                    ..Default::default()
                }),
                app_id: Some("dev.vgit.native-preview".into()),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Workspace::new(window, cx)),
        )
        .expect("Unable to open the VGit window");

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
