mod demo;
mod graph;
mod theme;

use demo::{COMMITS, FileChange};
use gpui::{
    App, Application, Bounds, Context, Div, FocusHandle, FontWeight, KeyBinding, MouseButton,
    MouseDownEvent, MouseMoveEvent, SharedString, Stateful, TitlebarOptions, Window, WindowBounds,
    WindowOptions, actions, div, prelude::*, px, rgb, size,
};
use std::collections::HashSet;
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

/// One open editor tab. Diff tabs follow a changed file; source tabs are
/// opened from the file tree and stay open until closed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Diff { file: usize },
    Source { path: &'static str },
}

/// Widths of the fixed chrome around the editor, and the narrowest the editor
/// itself is allowed to become.
const ACTIVITY_WIDTH: f32 = 46.;
const RESIZER_WIDTH: f32 = 5.;
const RIGHT_SIDEBAR_WIDTH: f32 = 320.;
const EDITOR_MIN: f32 = 320.;

/// Drag bounds for the history sidebar. It opens at its full width.
const SIDEBAR_MIN: f32 = 280.;
const SIDEBAR_MAX: f32 = 760.;
const SIDEBAR_DEFAULT: f32 = SIDEBAR_MAX;

const _: () = assert!(SIDEBAR_DEFAULT >= SIDEBAR_MIN && SIDEBAR_DEFAULT <= SIDEBAR_MAX);

/// A column of the history table. Every cell in a column is the same width on
/// every row, so hashes, branches, authors, and messages line up down the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Column {
    Commit,
    Branch,
    Author,
    Message,
    When,
}

/// The columns in the order they are laid out.
const COLUMNS: &[Column] = &[
    Column::Commit,
    Column::Branch,
    Column::Author,
    Column::Message,
    Column::When,
];

impl Column {
    fn label(self) -> &'static str {
        match self {
            Column::Commit => "COMMIT",
            Column::Branch => "BRANCH",
            Column::Author => "AUTHOR",
            Column::Message => "MESSAGE",
            Column::When => "WHEN",
        }
    }

    fn width(self) -> f32 {
        match self {
            Column::Commit => 78.,
            Column::Branch => 158.,
            Column::Author => 118.,
            Column::Message => 430.,
            Column::When => 70.,
        }
    }

    /// The message is what the list is for, so it cannot be hidden.
    fn hideable(self) -> bool {
        self != Column::Message
    }
}

/// Which panel, if any, is open over the workspace.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Popover {
    None,
    Settings,
    Columns,
    Branches,
}

/// Editor typography. A single monospace measure is shared by the diff and
/// source views so both line up column for column, with the ~1.55 line height
/// a code editor uses rather than the tighter UI leading.
const EDITOR_FONT: &str = "Menlo";
const EDITOR_FONT_SIZE: f32 = 13.;
const EDITOR_LINE_HEIGHT: f32 = 20.;

/// The source file opened in the second tab at startup.
const DEFAULT_SOURCE: &str = "src/ui/workspace.rs";

struct Workspace {
    focus: FocusHandle,
    history_scroll: gpui::ScrollHandle,
    theme: Theme,
    tabs: Vec<Tab>,
    active_tab: usize,
    commit: usize,
    file: usize,
    files: Vec<FileChange>,
    /// Directories collapsed in the source tree, by path.
    collapsed: HashSet<&'static str>,
    /// Branches drawn in the graph, at most [`graph::LANE_CAPACITY`] of them.
    visible_branches: Vec<&'static str>,
    /// Columns the user has hidden from the history table.
    hidden_columns: HashSet<Column>,
    sidebar_width: f32,
    /// Pointer x and sidebar width captured when a resize drag begins.
    resize_origin: Option<(f32, f32)>,
    popover: Popover,
    message: String,
}

fn button(
    colors: Palette,
    id: impl Into<gpui::ElementId>,
    text: impl Into<String>,
) -> Stateful<Div> {
    row()
        .id(id)
        .h(px(28.))
        .px_2()
        .gap_2()
        .rounded(px(4.))
        .border_1()
        .border_color(rgb(colors.line_strong))
        .bg(rgb(colors.elevated))
        .text_color(rgb(colors.text))
        .text_size(px(13.))
        .cursor_pointer()
        .hover(move |this| this.bg(rgb(colors.hover)))
        .active(|this| this.opacity(0.8))
        .child(text.into())
}

/// Clamps a dragged sidebar width to what `viewport` can spare while leaving
/// the editor at least [`EDITOR_MIN`].
fn clamp_sidebar_width(stored: f32, viewport: f32) -> f32 {
    let available = viewport - ACTIVITY_WIDTH - RESIZER_WIDTH - RIGHT_SIDEBAR_WIDTH - EDITOR_MIN;
    stored.min(available.max(SIDEBAR_MIN))
}

/// One fixed-width cell of the history table.
fn cell(width: f32) -> Div {
    row().w(px(width)).flex_none().pr(px(10.)).overflow_hidden()
}

fn header_cell(colors: Palette, width: f32, label: &'static str) -> Div {
    cell(width).child(section_label(colors, label))
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
        .text_size(px(19.))
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
            tabs: vec![
                Tab::Diff { file: 0 },
                Tab::Source {
                    path: DEFAULT_SOURCE,
                },
            ],
            active_tab: 0,
            commit: 0,
            file: 0,
            files: demo::files(),
            collapsed: HashSet::new(),
            visible_branches: demo::BRANCHES
                .iter()
                .take(graph::LANE_CAPACITY)
                .copied()
                .collect(),
            hidden_columns: HashSet::new(),
            sidebar_width: SIDEBAR_DEFAULT,
            resize_origin: None,
            popover: Popover::None,
            message: "main*  ·  3 changes  ·  2 staged".into(),
        }
    }

    fn colors(&self) -> Palette {
        self.theme.palette()
    }

    /// The sidebar width to actually paint. The stored width is what the user
    /// dragged to; this additionally clamps it to whatever the current window
    /// can spare while leaving the editor at least [`EDITOR_MIN`]. Narrowing
    /// the window squeezes the sidebar instead of pushing the right sidebar
    /// off-screen, and widening it again restores the dragged width.
    fn painted_sidebar_width(&self, window: &Window) -> f32 {
        clamp_sidebar_width(self.sidebar_width, f32::from(window.viewport_size().width))
    }

    fn select_commit(&mut self, index: usize, cx: &mut Context<Self>) {
        self.commit = index;
        self.file = COMMITS[index].file;
        // The scroll handle indexes visible rows, not the whole fixture.
        if let Some(row) = self.graph_rows().iter().position(|row| row.commit == index) {
            self.history_scroll.scroll_to_item(row);
        }
        self.message = format!("{}  ·  {}", COMMITS[index].hash, COMMITS[index].subject);
        cx.notify();
    }

    fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        self.file = index;
        self.message = format!("{}  ·  working tree", self.files[index].path);
        self.open_tab(Tab::Diff { file: index }, cx);
    }

    /// Focuses `tab` if it is already open, otherwise appends it.
    fn open_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.active_tab = match self.tabs.iter().position(|open| *open == tab) {
            Some(index) => index,
            None => {
                self.tabs.push(tab);
                self.tabs.len() - 1
            }
        };
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        // Keep one tab open so the editor always has something to show.
        if self.tabs.len() == 1 {
            return;
        }
        self.tabs.remove(index);
        if self.active_tab > index || self.active_tab == self.tabs.len() {
            self.active_tab -= 1;
        }
        cx.notify();
    }

    fn open_source(&mut self, path: &'static str, cx: &mut Context<Self>) {
        self.message = format!("{path}  ·  source");
        self.open_tab(Tab::Source { path }, cx);
    }

    /// Moves the selection `delta` rows through the visible history, wrapping
    /// at each end. Commits on hidden branches are skipped.
    fn step_commit(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = self.graph_rows();
        if rows.is_empty() {
            return;
        }
        let current = rows
            .iter()
            .position(|row| row.commit == self.commit)
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(rows.len() as isize) as usize;
        self.select_commit(rows[next].commit, cx);
    }

    fn toggle_popover(&mut self, popover: Popover, cx: &mut Context<Self>) {
        self.popover = if self.popover == popover {
            Popover::None
        } else {
            popover
        };
        cx.notify();
    }

    /// The columns currently shown, in layout order.
    fn visible_columns(&self) -> Vec<Column> {
        COLUMNS
            .iter()
            .copied()
            .filter(|column| !self.hidden_columns.contains(column))
            .collect()
    }

    /// Total width of a history row: the graph gutter plus every shown column.
    fn row_width(&self) -> f32 {
        graph::GRAPH_WIDTH
            + self
                .visible_columns()
                .iter()
                .map(|column| column.width())
                .sum::<f32>()
    }

    fn toggle_column(&mut self, column: Column, cx: &mut Context<Self>) {
        if !column.hideable() {
            return;
        }
        if !self.hidden_columns.remove(&column) {
            self.hidden_columns.insert(column);
        }
        self.message = format!(
            "{} column {}",
            column.label(),
            if self.hidden_columns.contains(&column) {
                "hidden"
            } else {
                "shown"
            }
        );
        cx.notify();
    }

    /// Adds or removes a branch from the graph. The gutter only has room for
    /// [`graph::LANE_CAPACITY`] lanes, so a full selection refuses to grow.
    fn toggle_branch(&mut self, branch: &'static str, cx: &mut Context<Self>) {
        if let Some(index) = self.visible_branches.iter().position(|b| *b == branch) {
            if self.visible_branches.len() == 1 {
                self.message = "At least one branch stays visible".into();
            } else {
                self.visible_branches.remove(index);
                self.message = format!("{branch} hidden");
                // The selection may have just been hidden along with it.
                let rows = self.graph_rows();
                if !rows.iter().any(|row| row.commit == self.commit)
                    && let Some(first) = rows.first()
                {
                    self.commit = first.commit;
                    self.file = COMMITS[first.commit].file;
                }
            }
        } else if self.visible_branches.len() < graph::LANE_CAPACITY {
            // Keep the fixture's branch order so lanes stay predictable.
            self.visible_branches.push(branch);
            self.visible_branches
                .sort_by_key(|name| demo::BRANCHES.iter().position(|b| b == name));
            self.message = format!("{branch} shown");
        } else {
            self.message = format!(
                "The graph shows {} branches at a time",
                graph::LANE_CAPACITY
            );
        }
        cx.notify();
    }

    fn graph_rows(&self) -> Vec<graph::Row> {
        graph::rows(&self.visible_branches)
    }

    fn toggle_folder(&mut self, path: &'static str, cx: &mut Context<Self>) {
        if !self.collapsed.remove(path) {
            self.collapsed.insert(path);
        }
        cx.notify();
    }

    /// Tree rows that are not inside a collapsed directory.
    fn visible_tree(&self) -> Vec<&'static demo::TreeEntry> {
        let mut rows = Vec::new();
        let mut hidden_below: Option<usize> = None;
        for entry in demo::TREE {
            if let Some(depth) = hidden_below {
                if entry.depth > depth {
                    continue;
                }
                hidden_below = None;
            }
            rows.push(entry);
            if entry.directory && self.collapsed.contains(entry.path) {
                hidden_below = Some(entry.depth);
            }
        }
        rows
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
                    .text_size(px(14.))
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
                    .h(px(26.))
                    .px_3()
                    .justify_center()
                    .rounded(px(5.))
                    .border_1()
                    .border_color(rgb(colors.line_strong))
                    .bg(rgb(colors.elevated))
                    .text_size(px(13.))
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
            .w(px(ACTIVITY_WIDTH))
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
                icon_button(
                    colors,
                    "activity-settings",
                    "⚙",
                    self.popover == Popover::Settings,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_popover(Popover::Settings, cx);
                })),
            )
    }

    /// A thin drag handle that resizes the history sidebar.
    fn sidebar_resizer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let dragging = self.resize_origin.is_some();
        div()
            .id("sidebar-resizer")
            .w(px(RESIZER_WIDTH))
            .flex_none()
            .h_full()
            .cursor_col_resize()
            .bg(rgb(if dragging { colors.local } else { colors.line }))
            .hover(move |this| this.bg(rgb(colors.local)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.resize_origin = Some((f32::from(event.position.x), this.sidebar_width));
                    cx.notify();
                }),
            )
    }

    fn graph_sidebar(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let columns = self.visible_columns();
        let row_width = self.row_width();
        let rows = self.graph_rows();
        column()
            .w(px(width))
            .flex_none()
            .h_full()
            .min_h_0()
            .bg(rgb(colors.sidebar))
            .child(
                column()
                    .h(px(74.))
                    .flex_none()
                    .px_3()
                    .pt_3()
                    .gap_2()
                    .child(
                        row()
                            .gap_2()
                            .child(section_label(colors, "SOURCE CONTROL GRAPH"))
                            .child(div().flex_1())
                            .child(button(colors, "pick-branches", "⑂  Branches").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.toggle_popover(Popover::Branches, cx);
                                }),
                            ))
                            .child(button(colors, "pick-columns", "▦  Columns").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.toggle_popover(Popover::Columns, cx);
                                }),
                            )),
                    )
                    .child(
                        row()
                            .h(px(30.))
                            .px_2()
                            .gap_2()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(rgb(colors.line))
                            .bg(rgb(colors.editor))
                            .text_size(px(13.))
                            .text_color(rgb(colors.muted))
                            .child("⌕")
                            .child("Filter commits, authors, refs"),
                    ),
            )
            .child(
                div()
                    .id("history-columns")
                    .flex_1()
                    .min_h_0()
                    .overflow_x_scroll()
                    .child(
                        column()
                            .w(px(row_width))
                            .h_full()
                            .min_h_0()
                            .child(
                                row()
                                    .h(px(28.))
                                    .w(px(row_width))
                                    .flex_none()
                                    .pl(px(graph::GRAPH_WIDTH))
                                    .border_y_1()
                                    .border_color(rgb(colors.line))
                                    .children(columns.iter().map(|column| {
                                        header_cell(colors, column.width(), column.label())
                                    })),
                            )
                            .child(
                                column()
                                    .id("graph-history")
                                    .track_scroll(&self.history_scroll)
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .relative()
                                    .children(rows.iter().enumerate().map(|(index, graph_row)| {
                                        let commit = &COMMITS[graph_row.commit];
                                        let selected = self.commit == graph_row.commit;
                                        let head = graph_row.commit == demo::HEAD_COMMIT;
                                        let target = graph_row.commit;
                                        row()
                                            .id(("commit-row", index))
                                            .h(px(graph::ROW_HEIGHT))
                                            .w(px(row_width))
                                            .flex_none()
                                            .pl(px(graph::GRAPH_WIDTH))
                                            .cursor_pointer()
                                            .bg(rgb(if selected {
                                                colors.selection
                                            } else {
                                                colors.sidebar
                                            }))
                                            .hover(move |this| this.bg(rgb(colors.hover)))
                                            .children(columns.iter().map(|column| {
                                                let width = column.width();
                                                match column {
                                                    Column::Commit => cell(width)
                                                        .text_size(px(12.))
                                                        .text_color(rgb(colors.dim))
                                                        .child(commit.hash),
                                                    Column::Branch => cell(width)
                                                        .gap_1()
                                                        .child(badge(
                                                            colors,
                                                            commit.branch,
                                                            colors.branch(graph_row.lane),
                                                        ))
                                                        .when(head, |this| {
                                                            this.child(badge(
                                                                colors,
                                                                "HEAD",
                                                                colors.text_bright,
                                                            ))
                                                        }),
                                                    Column::Author => cell(width)
                                                        .text_size(px(12.))
                                                        .text_color(rgb(colors.muted))
                                                        .child(
                                                            div()
                                                                .min_w_0()
                                                                .truncate()
                                                                .child(commit.author),
                                                        ),
                                                    Column::Message => cell(width)
                                                        .text_size(px(13.))
                                                        .text_color(rgb(if selected {
                                                            colors.text_bright
                                                        } else {
                                                            colors.text
                                                        }))
                                                        .child(
                                                            div()
                                                                .min_w_0()
                                                                .truncate()
                                                                .child(commit.subject),
                                                        ),
                                                    Column::When => cell(width)
                                                        .text_size(px(11.))
                                                        .text_color(rgb(colors.dim))
                                                        .child(commit.time),
                                                }
                                            }))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.select_commit(target, cx);
                                            }))
                                    }))
                                    .child(graph::sidebar_graph(rows.clone(), colors)),
                            ),
                    ),
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
                            .text_size(px(13.))
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
                            .text_size(px(12.))
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
        let tabs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let active = self.active_tab == index;
                let (marker, marker_color, title) = match *tab {
                    Tab::Diff { file } => {
                        let file = &self.files[file];
                        (
                            file.status,
                            colors.local,
                            format!(
                                "{} (Working Tree)",
                                file.path.rsplit('/').next().unwrap_or("file")
                            ),
                        )
                    }
                    Tab::Source { path } => (
                        if path.ends_with(".md") { "MD" } else { "RS" },
                        colors.remote,
                        path.rsplit('/').next().unwrap_or(path).to_string(),
                    ),
                };
                row()
                    .id(("editor-tab", index))
                    .h_full()
                    .px_3()
                    .gap_2()
                    .flex_none()
                    .border_r_1()
                    .border_color(rgb(colors.line))
                    .when(active, |this| {
                        this.border_t_1()
                            .border_color(rgb(marker_color))
                            .bg(rgb(colors.editor))
                    })
                    .text_size(px(13.))
                    .text_color(rgb(if active {
                        colors.text_bright
                    } else {
                        colors.muted
                    }))
                    .cursor_pointer()
                    .hover(move |this| this.bg(rgb(colors.hover)))
                    .child(div().text_color(rgb(marker_color)).child(marker))
                    .child(title)
                    .child(
                        div()
                            .id(("close-tab", index))
                            .px_1()
                            .rounded(px(3.))
                            .text_color(rgb(colors.dim))
                            .cursor_pointer()
                            .hover(move |this| {
                                this.bg(rgb(colors.line_strong))
                                    .text_color(rgb(colors.text_bright))
                            })
                            .child("\u{00d7}")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_tab(index, cx);
                            })),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active_tab = index;
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();

        row()
            .id("editor-tabs")
            .h(px(35.))
            .flex_none()
            .overflow_x_scroll()
            .bg(rgb(colors.editor_alt))
            .border_b_1()
            .border_color(rgb(colors.line))
            .children(tabs)
            .child(div().flex_1().min_w(px(24.)))
    }

    fn breadcrumb(&self) -> impl IntoElement {
        let colors = self.colors();
        let path = match self.tabs[self.active_tab] {
            Tab::Diff { file } => self.files[file].path,
            Tab::Source { path } => path,
        };
        row()
            .h(px(28.))
            .flex_none()
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(rgb(colors.line))
            .bg(rgb(colors.editor))
            .text_size(px(12.))
            .text_color(rgb(colors.muted))
            .children(path.split('/').enumerate().flat_map(|(index, part)| {
                let mut elements = Vec::new();
                if index > 0 {
                    elements.push(div().text_color(rgb(colors.dim)).child("›"));
                }
                elements.push(div().child(part.to_string()));
                elements
            }))
            .child(div().text_color(rgb(colors.dim)).child("›"))
            .child(match self.tabs[self.active_tab] {
                Tab::Diff { .. } => "Working Tree",
                Tab::Source { .. } => "Source",
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
            .h(px(EDITOR_LINE_HEIGHT))
            .flex_none()
            .font_family(EDITOR_FONT)
            .text_size(px(EDITOR_FONT_SIZE))
            .line_height(px(EDITOR_LINE_HEIGHT))
            .bg(rgb(match kind {
                "+" => colors.added_bg,
                "-" => colors.removed_bg,
                _ => colors.editor,
            }))
            .child(
                div()
                    .w(px(56.))
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

    fn diff_editor(&self, index: usize) -> impl IntoElement {
        let colors = self.colors();
        let file = &self.files[index];
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
                            .text_size(px(13.))
                            .child(format!("+{}", file.added)),
                    )
                    .child(
                        div()
                            .text_color(rgb(colors.red))
                            .text_size(px(13.))
                            .child(format!("−{}", file.removed)),
                    ),
            )
            .child(
                div()
                    .h(px(EDITOR_LINE_HEIGHT + 6.))
                    .flex_none()
                    .px_4()
                    .py_1()
                    .font_family(EDITOR_FONT)
                    .text_size(px(EDITOR_FONT_SIZE))
                    .line_height(px(EDITOR_LINE_HEIGHT))
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

    fn source_editor(&self, path: &'static str) -> impl IntoElement {
        let colors = self.colors();
        column()
            .id("source-editor")
            .flex_1()
            .min_h_0()
            .overflow_scroll()
            .bg(rgb(colors.editor))
            .py_2()
            .children(
                demo::source(path)
                    .iter()
                    .enumerate()
                    .map(|(offset, code)| self.code_line(offset + 1, " ", code)),
            )
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
            .child(match self.tabs[self.active_tab] {
                Tab::Diff { file } => self.diff_editor(file).into_any_element(),
                Tab::Source { path } => self.source_editor(path).into_any_element(),
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
            .h(px(28.))
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
                    .text_size(px(12.))
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
                    .text_size(px(13.))
                    .text_color(rgb(colors.text))
                    .child(file.path.rsplit('/').next().unwrap_or(file.path)),
            )
            .child(
                div().text_size(px(11.)).text_color(rgb(colors.dim)).child(
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
                    .text_size(px(16.))
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

    fn repository_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .text_size(px(15.))
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
                            .text_size(px(13.))
                            .child(div().w(px(86.)).text_color(rgb(colors.muted)).child("HEAD"))
                            .child(div().text_color(rgb(colors.local)).child("main")),
                    )
                    .child(
                        row()
                            .text_size(px(13.))
                            .child(
                                div()
                                    .w(px(86.))
                                    .text_color(rgb(colors.muted))
                                    .child("Upstream"),
                            )
                            .child(div().text_color(rgb(colors.remote)).child("origin/main")),
                    )
                    .child(
                        row()
                            .text_size(px(13.))
                            .child(
                                div()
                                    .w(px(86.))
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
                            .text_size(px(13.))
                            .child(
                                div()
                                    .w(px(86.))
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
                            .child(section_label(colors, "REFS"))
                            .child(div().flex().flex_wrap().gap_1().children(
                                demo::REFS.iter().enumerate().map(|(index, reference)| {
                                    let color = match reference.kind {
                                        demo::RefKind::Local => colors.local,
                                        demo::RefKind::Remote => colors.remote,
                                        demo::RefKind::Tag => colors.tag,
                                    };
                                    let target = reference.commit;
                                    div()
                                        .id(("ref-badge", index))
                                        .cursor_pointer()
                                        .hover(|this| this.opacity(0.75))
                                        .child(badge(colors, reference.name, color))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.select_commit(target, cx);
                                        }))
                                }),
                            ))
                            .child(div().text_size(px(12.)).text_color(rgb(colors.dim)).child(
                                format!(
                                        "{} local · {} remote · {} tags",
                                        demo::REFS
                                            .iter()
                                            .filter(|r| r.kind == demo::RefKind::Local)
                                            .count(),
                                        demo::REFS
                                            .iter()
                                            .filter(|r| r.kind == demo::RefKind::Remote)
                                            .count(),
                                        demo::REFS
                                            .iter()
                                            .filter(|r| r.kind == demo::RefKind::Tag)
                                            .count(),
                                    ),
                            )),
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
                                    .text_size(px(13.))
                                    .text_color(rgb(colors.text))
                                    .child(commit.subject),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
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
        let open_source = match self.tabs[self.active_tab] {
            Tab::Source { path } => Some(path),
            Tab::Diff { .. } => None,
        };
        let tree_rows = self
            .visible_tree()
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                let collapsed = self.collapsed.contains(entry.path);
                let selected = open_source == Some(entry.path);
                let (glyph, glyph_color) = if entry.directory {
                    (if collapsed { "›" } else { "⌄" }, colors.muted)
                } else if entry.path.ends_with(".md") {
                    ("#", colors.muted)
                } else {
                    ("RS", colors.remote)
                };
                let path = entry.path;
                row()
                    .id(("tree-row", index))
                    .h(px(26.))
                    .pl(px(12. + entry.depth as f32 * 14.))
                    .pr_3()
                    .gap_2()
                    .cursor_pointer()
                    .bg(rgb(if selected {
                        colors.selection
                    } else {
                        colors.panel
                    }))
                    .hover(move |this| this.bg(rgb(colors.hover)))
                    .child(
                        div()
                            .w(px(16.))
                            .flex_none()
                            .font_family(EDITOR_FONT)
                            .text_size(px(11.))
                            .text_color(rgb(glyph_color))
                            .child(glyph),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(rgb(if entry.directory {
                                colors.text
                            } else if selected {
                                colors.text_bright
                            } else {
                                colors.muted
                            }))
                            .child(entry.name),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if entry.directory {
                            this.toggle_folder(path, cx);
                        } else {
                            this.open_source(path, cx);
                        }
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        column()
            .id("repository-sidebar")
            .w(px(RIGHT_SIDEBAR_WIDTH))
            .flex_none()
            .h_full()
            .min_h_0()
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
                            .text_size(px(16.))
                            .text_color(rgb(colors.muted))
                            .child("⋯"),
                    ),
            )
            .child(self.repository_state(cx))
            .child(div().h(px(1.)).flex_none().bg(rgb(colors.line)))
            .child(
                column()
                    .id("repository-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        row()
                            .h(px(28.))
                            .flex_none()
                            .px_3()
                            .gap_2()
                            .child(div().text_color(rgb(colors.muted)).child("⌄"))
                            .child(section_label(colors, format!("CHANGES  {changed_count}")))
                            .child(div().flex_1())
                            .child(
                                div()
                                    .text_size(px(17.))
                                    .text_color(rgb(colors.muted))
                                    .child("＋"),
                            ),
                    )
                    .children(changed_rows)
                    .child(
                        row()
                            .h(px(28.))
                            .flex_none()
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
                                    .text_size(px(17.))
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
                                    .child(section_label(colors, "SOURCE FILE TREE")),
                            )
                            .children(tree_rows),
                    ),
            )
            .child(
                column()
                    .flex_none()
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
                            .text_size(px(13.))
                            .text_color(rgb(colors.dim))
                            .child("Message (Ctrl+Enter to commit)"),
                    )
                    .child(
                        row()
                            .h(px(28.))
                            .justify_center()
                            .rounded(px(4.))
                            .bg(rgb(colors.local))
                            .text_color(rgb(colors.editor))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(13.))
                            .child("Commit  ·  demo"),
                    ),
            )
    }

    /// Shell shared by the column and branch pickers.
    fn picker(
        &self,
        id: &'static str,
        title: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let colors = self.colors();
        column()
            .id(id)
            .absolute()
            .left(px(ACTIVITY_WIDTH + 8.))
            .top(px(78.))
            .w(px(268.))
            .p_3()
            .gap_2()
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
                            .text_size(px(14.))
                            .text_color(rgb(colors.text_bright))
                            .child(title),
                    )
                    .child(
                        div()
                            .id("close-picker")
                            .px_2()
                            .text_color(rgb(colors.muted))
                            .cursor_pointer()
                            .child("\u{00d7}")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.popover = Popover::None;
                                cx.notify();
                            })),
                    ),
            )
            .child(div().h(px(1.)).bg(rgb(colors.line)))
    }

    /// A checkable row inside a picker.
    fn picker_row(
        colors: Palette,
        id: SharedString,
        label: impl Into<String>,
        accent: u32,
        checked: bool,
        enabled: bool,
    ) -> Stateful<Div> {
        row()
            .id(id)
            .h(px(28.))
            .px_2()
            .gap_2()
            .rounded(px(4.))
            .text_size(px(13.))
            .text_color(rgb(if enabled { colors.text } else { colors.dim }))
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(rgb(colors.hover)))
            })
            .child(
                div()
                    .w(px(14.))
                    .flex_none()
                    .text_color(rgb(if checked { accent } else { colors.dim }))
                    .child(if checked { "\u{2713}" } else { "\u{00b7}" }),
            )
            .child(div().flex_1().min_w_0().truncate().child(label.into()))
    }

    fn columns_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        self.picker("columns-panel", "Columns", cx)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(colors.dim))
                    .child("Choose which columns the history shows."),
            )
            .children(COLUMNS.iter().map(|&column| {
                let shown = !self.hidden_columns.contains(&column);
                Self::picker_row(
                    colors,
                    SharedString::from(format!("column-{}", column.label())),
                    column.label(),
                    colors.local,
                    shown,
                    column.hideable(),
                )
                .when(!column.hideable(), |this| {
                    this.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(colors.dim))
                            .child("always"),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_column(column, cx);
                }))
            }))
    }

    fn branches_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let shown = self.visible_branches.len();
        self.picker("branches-panel", "Branches", cx)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(colors.dim))
                    .child(format!(
                        "{shown} of {} shown. The graph draws up to {} lanes.",
                        demo::BRANCHES.len(),
                        graph::LANE_CAPACITY
                    )),
            )
            .children(demo::BRANCHES.iter().enumerate().map(|(index, &branch)| {
                let lane = self.visible_branches.iter().position(|b| *b == branch);
                let full = lane.is_none() && shown >= graph::LANE_CAPACITY;
                Self::picker_row(
                    colors,
                    SharedString::from(format!("branch-{index}")),
                    branch,
                    colors.branch(lane.unwrap_or(0)),
                    lane.is_some(),
                    !full,
                )
                .when(branch == demo::HEAD_BRANCH, |this| {
                    this.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(colors.local))
                            .child("HEAD"),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_branch(branch, cx);
                }))
            }))
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
                            .text_size(px(14.))
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
                                this.popover = Popover::None;
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
                                    .text_size(px(13.))
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
                    .text_size(px(12.))
                    .text_color(rgb(colors.dim))
                    .child("Theme is kept for this preview session."),
            )
    }

    fn statusbar(&self) -> impl IntoElement {
        let colors = self.colors();
        row()
            .h(px(24.))
            .flex_none()
            .px_2()
            .gap_3()
            .bg(rgb(colors.local))
            .text_color(rgb(colors.editor))
            .text_size(px(12.))
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .text_size(px(14.))
            .on_action(cx.listener(|this, _: &NextCommit, _, cx| {
                this.step_commit(1, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousCommit, _, cx| {
                this.step_commit(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowDiff, _, cx| {
                this.open_tab(Tab::Diff { file: this.file }, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowSource, _, cx| {
                match this
                    .tabs
                    .iter()
                    .position(|tab| matches!(tab, Tab::Source { .. }))
                {
                    Some(index) => {
                        this.active_tab = index;
                        cx.notify();
                    }
                    None => this.open_source(DEFAULT_SOURCE, cx),
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleStage, _, cx| {
                this.toggle_stage(this.file, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSettings, _, cx| {
                this.toggle_popover(Popover::Settings, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                this.popover = Popover::None;
                cx.notify();
            }))
            .on_action(|_: &Close, window, _| window.remove_window())
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let Some((origin_x, origin_width)) = this.resize_origin else {
                    return;
                };
                if event.pressed_button != Some(MouseButton::Left) {
                    this.resize_origin = None;
                    cx.notify();
                    return;
                }
                let width = origin_width + f32::from(event.position.x) - origin_x;
                let width = width.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
                if width != this.sidebar_width {
                    this.sidebar_width = width;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseUpEvent, _, cx| {
                    if this.resize_origin.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .child(self.titlebar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.activity_bar(cx))
                    .child(self.graph_sidebar(self.painted_sidebar_width(window), cx))
                    .child(self.sidebar_resizer(cx))
                    .child(self.editor(cx))
                    .child(self.right_sidebar(cx)),
            )
            .child(self.statusbar())
            .when(self.popover == Popover::Settings, |this| {
                this.child(self.settings_panel(cx))
            })
            .when(self.popover == Popover::Columns, |this| {
                this.child(self.columns_panel(cx))
            })
            .when(self.popover == Popover::Branches, |this| {
                this.child(self.branches_panel(cx))
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
                    size(px(1680.), px(960.)),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Width consumed by everything except the history sidebar.
    fn chrome() -> f32 {
        ACTIVITY_WIDTH + RESIZER_WIDTH + RIGHT_SIDEBAR_WIDTH
    }

    fn all_columns_width() -> f32 {
        graph::GRAPH_WIDTH + COLUMNS.iter().map(|c| c.width()).sum::<f32>()
    }

    #[test]
    fn a_roomy_window_keeps_the_dragged_width() {
        assert_eq!(clamp_sidebar_width(SIDEBAR_DEFAULT, 1680.), SIDEBAR_DEFAULT);
        assert_eq!(clamp_sidebar_width(420., 1680.), 420.);
    }

    /// The sidebar yields to the editor rather than pushing the repository
    /// sidebar off the right edge.
    #[test]
    fn a_narrow_window_squeezes_the_sidebar_not_the_editor() {
        for viewport in [1080., 1200., 1440., 1680.] {
            let sidebar = clamp_sidebar_width(SIDEBAR_DEFAULT, viewport);
            let editor = viewport - chrome() - sidebar;
            assert!(
                editor >= EDITOR_MIN - 0.01,
                "at {viewport}px the editor got {editor}px"
            );
            assert!(
                sidebar >= SIDEBAR_MIN - 0.01,
                "sidebar collapsed to {sidebar}"
            );
        }
    }

    #[test]
    fn a_wide_window_never_stretches_past_the_drag_bound() {
        assert_eq!(clamp_sidebar_width(SIDEBAR_MAX, 4000.), SIDEBAR_MAX);
    }

    /// The full table is wider than the sidebar can ever be, which is what the
    /// horizontal scroller exists for.
    #[test]
    fn the_full_history_table_is_wider_than_the_sidebar() {
        assert!(
            all_columns_width() > SIDEBAR_MAX,
            "the message column would never need scrolling"
        );
    }

    #[test]
    fn only_the_message_column_is_permanent() {
        let fixed: Vec<_> = COLUMNS.iter().filter(|c| !c.hideable()).collect();
        assert_eq!(fixed, vec![&Column::Message]);
    }

    /// Every column has a distinct label, so the picker is unambiguous.
    #[test]
    fn column_labels_are_unique() {
        let mut labels: Vec<_> = COLUMNS.iter().map(|c| c.label()).collect();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count);
    }
}
