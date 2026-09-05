mod git;
mod graph;
mod input;
mod theme;

use gpui::{
    AnyElement, App, Application, Bounds, Context, Div, FocusHandle, FontWeight, KeyBinding,
    MouseButton, MouseDownEvent, MouseMoveEvent, SharedString, Stateful, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, div, prelude::*, px, rgb, size,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
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

/// One open editor tab. Tabs are keyed by path rather than by index, because
/// a reload can renumber the change list underneath them.
#[derive(Clone, PartialEq, Eq)]
enum Tab {
    Diff { path: String, staged: bool },
    Source { path: String },
}

impl Tab {
    fn path(&self) -> &str {
        match self {
            Tab::Diff { path, .. } | Tab::Source { path } => path,
        }
    }
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
    Column::When,
    // The message is widest and least aligned, so it ends the row.
    Column::Message,
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

/// One row of the source tree, flattened from the tracked file list.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TreeRow {
    depth: usize,
    name: String,
    path: String,
    directory: bool,
}

/// Flattens sorted repository paths into an indented tree.
fn build_tree(paths: &[String]) -> Vec<TreeRow> {
    let mut rows: Vec<TreeRow> = Vec::new();
    let mut open: Vec<&str> = Vec::new();

    for path in paths {
        let segments: Vec<&str> = path.split('/').collect();
        let (directories, name) = segments.split_at(segments.len() - 1);

        // Close directories that this path has left, then open the new ones.
        while open.len() > directories.len() || open[..] != directories[..open.len()] {
            open.pop();
        }
        for (depth, segment) in directories.iter().enumerate().skip(open.len()) {
            open.push(segment);
            rows.push(TreeRow {
                depth,
                name: (*segment).to_string(),
                path: segments[..=depth].join("/"),
                directory: true,
            });
        }
        rows.push(TreeRow {
            depth: directories.len(),
            name: name[0].to_string(),
            path: path.clone(),
            directory: false,
        });
    }
    rows
}

/// Everything read from the repository in one background pass.
#[derive(Clone, Default)]
struct RepoData {
    root: PathBuf,
    status: git::Status,
    commits: Vec<git::Commit>,
    graph: graph::Graph,
    branches: Vec<String>,
    references: Vec<git::Reference>,
    stashes: Vec<git::Stash>,
    tree: Vec<TreeRow>,
}

/// Which actions make sense for the selected commit right now.
///
/// Git refuses several of these outright in the wrong state, so the buttons
/// are disabled rather than left to fail.
#[derive(Clone, Copy, Default)]
struct CommitActions {
    checkout: bool,
    revert: bool,
    reset: bool,
    amend: bool,
}

/// Where the repository read has got to.
enum RepoState {
    Loading,
    Failed(String),
    Ready(Box<RepoData>),
}

impl RepoState {
    fn data(&self) -> Option<&RepoData> {
        match self {
            RepoState::Ready(data) => Some(data),
            _ => None,
        }
    }
}

/// Which panel, if any, is open over the workspace./// Which panel, if any, is open over the workspace.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Popover {
    None,
    Settings,
    Columns,
    Branches,
    Repository,
}

/// Which single-line field is taking keystrokes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Field {
    CommitMessage,
    BranchName,
    StashMessage,
}

impl Field {
    fn placeholder(self) -> &'static str {
        match self {
            Field::CommitMessage => "Commit message",
            Field::BranchName => "New branch name",
            Field::StashMessage => "Stash message",
        }
    }
}

/// A repository write, deferred until it is either confirmed or run.
type Operation = Box<dyn FnOnce(&git::Repository) -> git::Result<()> + Send>;

/// A destructive action held until the user confirms it.
///
/// Everything that can lose committed or uncommitted work goes through here
/// rather than firing straight from a button.
struct Confirm {
    title: String,
    detail: String,
    /// What the status bar reports once it succeeds.
    outcome: String,
    action: Operation,
}

/// Editor typography. A single monospace measure is shared by the diff and
/// source views so both line up column for column, with the ~1.55 line height
/// a code editor uses rather than the tighter UI leading.
const EDITOR_FONT: &str = "Menlo";
const EDITOR_FONT_SIZE: f32 = 13.;
const EDITOR_LINE_HEIGHT: f32 = 20.;

/// How much history to read. A repository can hold far more than anyone will
/// scroll through, and the graph walk is linear in this number.
const HISTORY_LIMIT: usize = 2000;

struct Workspace {
    focus: FocusHandle,
    history_scroll: gpui::ScrollHandle,
    theme: Theme,
    tabs: Vec<Tab>,
    active_tab: usize,
    /// The selected commit, as an index into the loaded history.
    commit: usize,
    /// The selected path in the change list.
    file: usize,
    repository: Option<git::Repository>,
    repo: RepoState,
    /// Diff text by path and whether the index or the working tree was asked
    /// for. Filled in on demand, off the UI thread.
    diffs: HashMap<(String, bool), String>,
    /// File contents by path, filled in when a source tab opens.
    sources: HashMap<String, String>,
    /// Directories collapsed in the source tree, by path.
    collapsed: HashSet<String>,
    /// Branches drawn in the graph, at most [`graph::LANE_CAPACITY`] of them.
    /// Empty means every ref.
    visible_branches: Vec<String>,
    /// Columns the user has hidden from the history table.
    hidden_columns: HashSet<Column>,
    sidebar_width: f32,
    /// Pointer x and sidebar width captured when a resize drag begins.
    resize_origin: Option<(f32, f32)>,
    popover: Popover,
    /// Text typed into each single-line field, and which one has the keys.
    inputs: HashMap<Field, input::Input>,
    composing: Option<Field>,
    /// A destructive action waiting to be confirmed.
    confirm: Option<Confirm>,
    message: String,
    generation: u64,
    busy: bool,
    outcome: Option<String>,
    open_request: u64,
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

/// The new-side starting line of a unified diff hunk header.
fn hunk_start(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("@@ ")?;
    let plus = rest.split(' ').find(|part| part.starts_with('+'))?;
    plus.trim_start_matches('+').split(',').next()?.parse().ok()
}

/// Reads everything the workspace shows, in one blocking pass.
///
/// Called only from a background thread. `selection` limits the history to
/// those branches; empty means every ref.
fn load(
    repository: &git::Repository,
    selection: &[String],
) -> std::result::Result<RepoData, String> {
    let refs: Vec<&str> = selection.iter().map(String::as_str).collect();
    let status = repository.status().map_err(|error| error.to_string())?;
    let commits = repository
        .log_refs(&refs, HISTORY_LIMIT)
        .map_err(|error| error.to_string())?;
    let graph = graph::assign_lanes(&commits);

    Ok(RepoData {
        root: repository.root().to_path_buf(),
        status,
        graph,
        commits,
        // A repository can be read even when these fail; an empty list is
        // better than refusing to show the history.
        branches: repository.branches().unwrap_or_default(),
        references: repository.references().unwrap_or_default(),
        stashes: repository.stash_list().unwrap_or_default(),
        tree: build_tree(&repository.tracked_files().unwrap_or_default()),
    })
}

/// Clamps a dragged sidebar width to what `viewport` can spare while leaving
/// the editor at least [`EDITOR_MIN`].
fn clamp_sidebar_width(stored: f32, viewport: f32) -> f32 {
    let available = viewport - ACTIVITY_WIDTH - RESIZER_WIDTH - RIGHT_SIDEBAR_WIDTH - EDITOR_MIN;
    stored.min(available.max(SIDEBAR_MIN))
}

/// Which tab becomes active after the tab at `closed` is removed, given the
/// number of tabs `remaining` afterwards.
fn active_after_close(active: usize, closed: usize, remaining: usize) -> usize {
    if closed < active {
        active - 1
    } else if closed == active {
        // Hold the position, or fall back to the new last tab.
        active.min(remaining.saturating_sub(1))
    } else {
        active
    }
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
        let repository = std::env::current_dir()
            .ok()
            .and_then(|cwd| git::Repository::discover(cwd).ok());

        let mut workspace = Self {
            focus,
            history_scroll: gpui::ScrollHandle::new(),
            theme: Theme::Dark,
            tabs: Vec::new(),
            active_tab: 0,
            commit: 0,
            file: 0,
            repository,
            repo: RepoState::Loading,
            diffs: HashMap::new(),
            sources: HashMap::new(),
            collapsed: HashSet::new(),
            visible_branches: Vec::new(),
            hidden_columns: HashSet::new(),
            sidebar_width: SIDEBAR_DEFAULT,
            resize_origin: None,
            popover: Popover::None,
            inputs: [Field::CommitMessage, Field::BranchName, Field::StashMessage]
                .into_iter()
                .map(|f| (f, input::Input::default()))
                .collect(),
            composing: None,
            confirm: None,
            message: "Reading repository…".into(),
            generation: 0,
            busy: false,
            outcome: None,
            open_request: 0,
        };
        workspace.reload(cx);
        workspace
    }

    /// Reads the repository, entirely on a background thread. Git can block
    /// for a long time on a large repository, and the window must stay
    /// responsive while it does.
    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(repository) = self.repository.clone() else {
            self.repo = RepoState::Failed("No Git repository in this directory".into());
            self.message = "No Git repository in this directory".into();
            return;
        };
        let selection = self.visible_branches.clone();
        self.generation += 1;
        let generation = self.generation;
        let selected = self.commits().get(self.commit).map(|c| c.id.clone());
        let selected_file = self.changes().get(self.file).map(|f| f.path.clone());

        let task = cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { load(&repository, &selection) })
                .await;

            this.update(cx, |workspace, cx| {
                if workspace.generation != generation {
                    return;
                }
                match loaded {
                    Ok(data) => {
                        workspace.message = format!(
                            "{} commits · {} changed · {} staged",
                            data.commits.len(),
                            data.status.changed().count(),
                            data.status.staged().count()
                        );
                        workspace.commit = data
                            .commits
                            .iter()
                            .position(|c| Some(&c.id) == selected.as_ref())
                            .unwrap_or(0);
                        workspace.file = data
                            .status
                            .files
                            .iter()
                            .position(|f| Some(&f.path) == selected_file.as_ref())
                            .unwrap_or(0);
                        // Cached text can be stale after a write.
                        workspace.diffs.clear();
                        workspace.sources.clear();
                        workspace.repo = RepoState::Ready(Box::new(data));
                        workspace.ensure_a_tab(cx);
                    }
                    Err(error) => {
                        workspace.message = error.clone();
                        workspace.repo = RepoState::Failed(error);
                    }
                }
                if let Some(outcome) = workspace.outcome.take() {
                    workspace.message = outcome;
                }
                cx.notify();
            })
            .ok();
        });
        task.detach();
    }

    /// Runs an operation that changes the repository, then reloads.
    ///
    /// The call blocks, so it runs off the UI thread like every other Git
    /// call, and its own error message is what the user is shown.
    fn perform(
        &mut self,
        description: String,
        operation: impl FnOnce(&git::Repository) -> git::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            self.message = "A Git operation is running. Wait for it to finish.".into();
            cx.notify();
            return;
        }
        let Some(repository) = self.repository.clone() else {
            return;
        };
        self.message = format!("{description}…");
        self.busy = true;
        self.generation += 1;
        cx.notify();

        let task = cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { operation(&repository) })
                .await;

            this.update(cx, |workspace, cx| {
                workspace.busy = false;
                workspace.outcome = Some(match outcome {
                    Ok(()) => description,
                    Err(error) => error.to_string(),
                });
                workspace.reload(cx);
                cx.notify();
            })
            .ok();
        });
        task.detach();
    }

    /// Loads the diff for a path, unless it is already cached.
    fn request_diff(&mut self, path: String, staged: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let generation = self.generation;
        let key = (path.clone(), staged);
        if self.diffs.contains_key(&key) {
            return;
        }
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let task = cx.spawn(async move |this, cx| {
            let text = cx
                .background_executor()
                .spawn(async move { repository.diff(&path, staged) })
                .await;

            this.update(cx, |workspace, cx| {
                if workspace.generation != generation {
                    return;
                }
                workspace.diffs.insert(
                    key,
                    text.unwrap_or_else(|error| format!("Could not read the diff: {error}")),
                );
                cx.notify();
            })
            .ok();
        });
        task.detach();
    }

    /// Loads a file's contents, unless they are already cached.
    fn request_source(&mut self, path: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let generation = self.generation;
        if self.sources.contains_key(&path) {
            return;
        }
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let task = cx.spawn(async move |this, cx| {
            let key = path.clone();
            let text = cx
                .background_executor()
                .spawn(async move { repository.read_working_file(&path) })
                .await;

            this.update(cx, |workspace, cx| {
                if workspace.generation != generation {
                    return;
                }
                workspace.sources.insert(
                    key,
                    text.unwrap_or_else(|error| format!("Could not read the file: {error}")),
                );
                cx.notify();
            })
            .ok();
        });
        task.detach();
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

    fn data(&self) -> Option<&RepoData> {
        self.repo.data()
    }

    fn commits(&self) -> &[git::Commit] {
        self.data()
            .map(|data| data.commits.as_slice())
            .unwrap_or(&[])
    }

    fn changes(&self) -> Vec<git::FileStatus> {
        self.data()
            .map(|data| data.status.files.clone())
            .unwrap_or_default()
    }

    fn selected_commit(&self) -> Option<&git::Commit> {
        self.commits().get(self.commit)
    }

    /// Whether the selected commit is the one HEAD points at.
    fn selection_is_head(&self) -> bool {
        match (self.selected_commit(), self.data()) {
            (Some(commit), Some(data)) => data.status.head.as_deref() == Some(&commit.id),
            _ => false,
        }
    }

    fn commit_actions(&self) -> CommitActions {
        let (Some(_), Some(data)) = (self.selected_commit(), self.data()) else {
            return CommitActions::default();
        };
        // Nothing that moves HEAD is safe in the middle of a conflict.
        if data.status.has_conflicts() {
            return CommitActions::default();
        }
        CommitActions {
            checkout: true,
            // Git refuses to revert while the index holds staged work.
            revert: data.status.staged().count() == 0,
            reset: true,
            amend: self.selection_is_head(),
        }
    }

    fn select_commit(&mut self, index: usize, cx: &mut Context<Self>) {
        self.commit = index;
        self.history_scroll.scroll_to_item(index);
        if let Some(commit) = self.selected_commit() {
            self.message = format!("{}  ·  {}", commit.short_id, commit.subject);
        }
        cx.notify();
    }

    fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        let staged = self
            .changes()
            .get(index)
            .is_some_and(|f| f.is_staged() && !f.is_modified());
        self.select_file_side(index, staged, cx);
    }

    fn select_file_side(&mut self, index: usize, staged: bool, cx: &mut Context<Self>) {
        let Some(file) = self.changes().get(index).cloned() else {
            return;
        };
        self.file = index;
        self.message = format!(
            "{}  ·  {}",
            file.path,
            if staged { "index" } else { "working tree" }
        );
        self.open_tab(
            Tab::Diff {
                path: file.path.clone(),
                staged,
            },
            cx,
        );
        self.request_diff(file.path, staged, cx);
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
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        self.active_tab = active_after_close(self.active_tab, index, self.tabs.len());
        cx.notify();
    }

    /// Opens the first change as a diff when nothing is open, so the editor is
    /// not blank on a freshly loaded repository.
    fn ensure_a_tab(&mut self, cx: &mut Context<Self>) {
        if !self.tabs.is_empty() {
            for tab in self.tabs.clone() {
                match tab {
                    Tab::Diff { path, staged } => self.request_diff(path, staged, cx),
                    Tab::Source { path } => self.request_source(path, cx),
                }
            }
            return;
        }
        let Some(file) = self.changes().first().cloned() else {
            return;
        };
        let staged = file.is_staged() && !file.is_modified();
        self.open_tab(
            Tab::Diff {
                path: file.path.clone(),
                staged,
            },
            cx,
        );
        self.request_diff(file.path, staged, cx);
    }

    fn open_source(&mut self, path: String, cx: &mut Context<Self>) {
        self.message = format!("{path}  ·  source");
        self.open_tab(Tab::Source { path: path.clone() }, cx);
        self.request_source(path, cx);
    }

    /// Moves the selection `delta` rows through the history, wrapping at each
    /// end.
    fn step_commit(&mut self, delta: isize, cx: &mut Context<Self>) {
        let total = self.commits().len();
        if total == 0 {
            return;
        }
        let next = (self.commit as isize + delta).rem_euclid(total as isize) as usize;
        self.select_commit(next, cx);
    }

    fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.message = format!("Appearance changed to {}", theme.label());
        cx.notify();
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
    /// Total width of a history row: the graph gutter plus every shown column.
    fn row_width(&self, gutter: f32) -> f32 {
        gutter
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
    /// Adds or removes a branch from the history. An empty selection means
    /// every ref; the picker refuses to grow past what the graph can label.
    fn toggle_branch(&mut self, branch: String, cx: &mut Context<Self>) {
        if let Some(index) = self.visible_branches.iter().position(|b| *b == branch) {
            self.visible_branches.remove(index);
        } else if self.visible_branches.len() < graph::LANE_CAPACITY {
            self.visible_branches.push(branch);
        } else {
            self.message = format!(
                "Choose at most {} branches, or none for all",
                graph::LANE_CAPACITY
            );
            cx.notify();
            return;
        }
        self.commit = 0;
        self.reload(cx);
    }

    fn toggle_folder(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&path) {
            self.collapsed.insert(path);
        }
        cx.notify();
    }

    /// Tree rows that are not inside a collapsed directory.
    fn visible_tree(&self) -> Vec<TreeRow> {
        let Some(data) = self.data() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        let mut hidden_below: Option<usize> = None;
        for entry in &data.tree {
            if let Some(depth) = hidden_below {
                if entry.depth > depth {
                    continue;
                }
                hidden_below = None;
            }
            rows.push(entry.clone());
            if entry.directory && self.collapsed.contains(&entry.path) {
                hidden_below = Some(entry.depth);
            }
        }
        rows
    }

    /// Stages or unstages a path for real, then reloads.
    fn toggle_stage(&mut self, index: usize, cx: &mut Context<Self>) {
        let staged = self.changes().get(index).is_some_and(|f| f.is_staged());
        self.stage_side(index, staged, cx);
    }

    fn stage_side(&mut self, index: usize, staged: bool, cx: &mut Context<Self>) {
        let Some(file) = self.changes().get(index).cloned() else {
            return;
        };
        let path = file.path.clone();
        let description = format!("{} {}", if staged { "Unstaged" } else { "Staged" }, path);
        self.perform(
            description,
            move |repository| {
                if staged {
                    repository.unstage(&[&path])
                } else {
                    repository.stage(&[&path])
                }
            },
            cx,
        );
    }

    /// Commits whatever is staged, using the typed message.
    fn commit_staged(&mut self, cx: &mut Context<Self>) {
        let staged = self
            .data()
            .map(|data| data.status.staged().count())
            .unwrap_or(0);
        if staged == 0 {
            self.message = "Nothing is staged".into();
            cx.notify();
            return;
        }
        let message = self.input(Field::CommitMessage).trim().to_string();
        if message.is_empty() {
            self.composing = Some(Field::CommitMessage);
            self.message = "Type a commit message first".into();
            cx.notify();
            return;
        }
        self.composing = None;
        let subject = message.clone();
        self.perform(
            format!("Committed: {subject}"),
            move |repository| repository.commit(&message),
            cx,
        );
    }

    fn input(&self, field: Field) -> &str {
        self.inputs
            .get(&field)
            .map(|v| v.text.as_str())
            .unwrap_or("")
    }

    fn focus_field(&mut self, field: Field, cx: &mut Context<Self>) {
        self.composing = Some(field);
        cx.notify();
    }

    /// Handles a keystroke aimed at whichever field is focused. Returns
    /// whether the key was consumed, so the caller can stop it reaching the
    /// key bindings and staging a file mid-word.
    fn type_into_field(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(field) = self.composing else {
            return false;
        };
        self.inputs.entry(field).or_default().key(event, cx)
    }

    /// Queues a destructive action for confirmation instead of running it.
    fn ask(
        &mut self,
        title: impl Into<String>,
        detail: impl Into<String>,
        outcome: impl Into<String>,
        action: impl FnOnce(&git::Repository) -> git::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.confirm = Some(Confirm {
            title: title.into(),
            detail: detail.into(),
            outcome: outcome.into(),
            action: Box::new(action),
        });
        cx.notify();
    }

    fn run_confirmed(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        self.perform(confirm.outcome, confirm.action, cx);
    }

    fn fetch(&mut self, cx: &mut Context<Self>) {
        self.perform(
            "Fetched remote".into(),
            |repository| {
                let tracking = repository
                    .current_branch()?
                    .map(|branch| repository.tracking(&branch))
                    .transpose()?
                    .flatten();
                let remote = match tracking {
                    Some(tracking) => tracking.remote,
                    None => repository.default_remote()?,
                };
                repository.fetch(&remote, true)
            },
            cx,
        );
    }

    fn pull(&mut self, cx: &mut Context<Self>) {
        self.perform(
            "Pulled configured upstream".into(),
            |repository| repository.pull_tracking(),
            cx,
        );
    }

    fn push(&mut self, cx: &mut Context<Self>) {
        self.perform(
            "Pushed configured destination".into(),
            |repository| repository.push_tracking(),
            cx,
        );
    }

    fn amend(&mut self, cx: &mut Context<Self>) {
        let typed = self.input(Field::CommitMessage).trim().to_string();
        let subject = self
            .commits()
            .iter()
            .find(|commit| {
                self.data()
                    .is_some_and(|data| data.status.head.as_ref() == Some(&commit.id))
            })
            .map(|commit| commit.subject.clone())
            .unwrap_or_default();
        self.composing = None;
        self.ask(
            "Amend the last commit?",
            format!(
                "Replaces {subject:?}. The original stays in the reflog, but if it \
                 was already pushed the branch will have diverged."
            ),
            "Amended the last commit".to_string(),
            move |repository| {
                if typed.is_empty() {
                    repository.amend_keep_message()
                } else {
                    repository.amend(&typed)
                }
            },
            cx,
        );
    }

    fn revert_selected(&mut self, cx: &mut Context<Self>) {
        let Some(commit) = self.selected_commit().cloned() else {
            return;
        };
        let id = commit.id.clone();
        self.perform(
            format!("Reverted {}", commit.short_id),
            move |repository| repository.revert(&id),
            cx,
        );
    }

    fn reset_to_selected(&mut self, mode: git::ResetMode, cx: &mut Context<Self>) {
        let Some(commit) = self.selected_commit().cloned() else {
            return;
        };
        let id = commit.id.clone();
        let label = match mode {
            git::ResetMode::Soft => "soft",
            git::ResetMode::Mixed => "mixed",
            git::ResetMode::Hard => "hard",
        };
        let outcome = format!("Reset {label} to {}", commit.short_id);

        if mode.is_destructive() {
            self.ask(
                format!("Hard reset to {}?", commit.short_id),
                format!(
                    "Moves the branch to {:?} and throws away every uncommitted \
                     change in the index and the working tree. This cannot be undone \
                     through Git.",
                    commit.subject
                ),
                outcome,
                move |repository| repository.reset(&id, mode),
                cx,
            );
        } else {
            self.perform(outcome, move |repository| repository.reset(&id, mode), cx);
        }
    }

    fn checkout_selected(&mut self, cx: &mut Context<Self>) {
        let Some(commit) = self.selected_commit().cloned() else {
            return;
        };
        let id = commit.id.clone();
        self.perform(
            format!("Checked out {} (detached)", commit.short_id),
            move |repository| repository.checkout_commit(&id),
            cx,
        );
    }

    fn switch_to_branch(&mut self, branch: String, cx: &mut Context<Self>) {
        let name = branch.clone();
        self.perform(
            format!("Switched to {branch}"),
            move |repository| repository.switch_branch(&name),
            cx,
        );
    }

    fn create_branch(&mut self, cx: &mut Context<Self>) {
        let name = self.input(Field::BranchName).trim().to_string();
        if name.is_empty() {
            self.focus_field(Field::BranchName, cx);
            self.message = "Type a branch name first".into();
            return;
        }
        self.composing = None;
        let branch = name.clone();
        self.perform(
            format!("Created and switched to {branch}"),
            move |repository| repository.create_and_switch(&name),
            cx,
        );
    }

    fn stash(&mut self, cx: &mut Context<Self>) {
        let typed = self.input(Field::StashMessage).trim().to_string();
        let message = if typed.is_empty() {
            "VGit stash".to_string()
        } else {
            typed
        };
        self.composing = None;
        self.perform(
            format!("Stashed: {message}"),
            move |repository| repository.stash_push(&message, true),
            cx,
        );
    }

    fn apply_stash(&mut self, reference: String, cx: &mut Context<Self>) {
        let name = reference.clone();
        self.perform(
            format!("Applied {reference}"),
            move |repository| repository.stash_apply(&name),
            cx,
        );
    }

    fn drop_stash(&mut self, reference: String, cx: &mut Context<Self>) {
        let name = reference.clone();
        self.ask(
            format!("Drop {reference}?"),
            "Removes this stash entry. The interface cannot find it again.".to_string(),
            format!("Dropped {reference}"),
            move |repository| repository.stash_drop(&name),
            cx,
        );
    }

    fn stash_pop(&mut self, cx: &mut Context<Self>) {
        self.perform(
            "Restored the latest stash".into(),
            |repository| repository.stash_pop(),
            cx,
        );
    }

    /// Throws away one path's working-tree changes. Destructive.
    fn discard_file(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(file) = self.changes().get(index).cloned() else {
            return;
        };
        let path = file.path.clone();
        if file.untracked {
            self.message = format!("{path} is untracked; use Clean to remove it");
            cx.notify();
            return;
        }
        self.ask(
            format!("Discard changes to {path}?"),
            "Throws away every uncommitted change to this file. This cannot be \
             undone through Git."
                .to_string(),
            format!("Discarded {path}"),
            move |repository| repository.discard(&[&path]),
            cx,
        );
    }

    /// Deletes every untracked file. Destructive, and not undoable at all.
    fn clean(&mut self, cx: &mut Context<Self>) {
        let count = self
            .data()
            .map(|data| data.status.files.iter().filter(|f| f.untracked).count())
            .unwrap_or(0);
        if count == 0 {
            self.message = "There are no untracked files".into();
            cx.notify();
            return;
        }
        self.ask(
            format!("Delete {count} untracked file(s)?"),
            "Removes untracked files and directories from disk. Git never had \
             them, so this cannot be undone at all."
                .to_string(),
            format!("Deleted {count} untracked file(s)"),
            |repository| repository.clean(true),
            cx,
        );
    }

    /// Opens a different repository, using the platform folder picker.
    fn open_repository(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            self.message = "Wait for the Git operation before opening another repository".into();
            cx.notify();
            return;
        }
        self.open_request += 1;
        let open_request = self.open_request;
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });

        let task = cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(chosen))) = paths.await else {
                return;
            };
            let Some(path) = chosen.into_iter().next() else {
                return;
            };
            let opened = cx
                .background_executor()
                .spawn(async move { git::Repository::discover(&path).map_err(|e| e.to_string()) })
                .await;

            this.update(cx, |workspace, cx| {
                if workspace.busy || workspace.open_request != open_request {
                    return;
                }
                match opened {
                    Ok(repository) => {
                        workspace.message = format!("Opened {}", repository.root().display());
                        workspace.repository = Some(repository);
                        // Nothing from the previous repository still applies.
                        workspace.repo = RepoState::Loading;
                        workspace.visible_branches.clear();
                        workspace.tabs.clear();
                        workspace.active_tab = 0;
                        workspace.commit = 0;
                        workspace.file = 0;
                        workspace.collapsed.clear();
                        workspace.popover = Popover::None;
                        workspace.confirm = None;
                        for value in workspace.inputs.values_mut() {
                            *value = input::Input::default();
                        }
                        workspace.composing = None;
                        workspace.diffs.clear();
                        workspace.sources.clear();
                        workspace.reload(cx);
                    }
                    Err(error) => workspace.message = error,
                }
                cx.notify();
            })
            .ok();
        });
        task.detach();
    }

    fn stage_all(&mut self, cx: &mut Context<Self>) {
        self.perform(
            "Staged every change".into(),
            |repository| repository.stage_all(),
            cx,
        );
    }

    fn unstage_all(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<String> = self
            .data()
            .map(|data| data.status.staged().map(|f| f.path.clone()).collect())
            .unwrap_or_default();
        if paths.is_empty() {
            return;
        }
        self.perform(
            "Unstaged every change".into(),
            move |repository| {
                let borrowed: Vec<&str> = paths.iter().map(String::as_str).collect();
                repository.unstage(&borrowed)
            },
            cx,
        );
    }

    fn titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let name = self
            .data()
            .map(|data| {
                data.root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| data.root.display().to_string())
            })
            .unwrap_or_else(|| "no repository".into());
        let summary = match &self.repo {
            RepoState::Ready(data) => format!(
                "{}   ·   {}   ·   {}",
                name,
                data.status
                    .head
                    .as_deref()
                    .map(|id| id[..7.min(id.len())].to_string())
                    .unwrap_or_else(|| "unborn".into()),
                data.status
                    .branch
                    .clone()
                    .unwrap_or_else(|| "detached".into())
            ),
            RepoState::Loading => "Reading…".into(),
            RepoState::Failed(reason) => reason.clone(),
        };
        let ahead = self.data().map(|data| data.status.ahead).unwrap_or(0);
        let behind = self.data().map(|data| data.status.behind).unwrap_or(0);

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
                    .gap_2()
                    .text_size(px(14.))
                    .text_color(rgb(colors.text_bright))
                    .child(div().text_color(rgb(colors.local)).child("◇"))
                    .child("vgit"),
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
                    .child(div().min_w_0().truncate().child(summary)),
            )
            .child(div().flex_1())
            .child(
                button(colors, "open-repository", "⌂  Open").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.open_repository(cx);
                    },
                )),
            )
            .child(
                button(colors, "fetch", "↓  Fetch").on_click(cx.listener(|this, _, _, cx| {
                    this.fetch(cx);
                })),
            )
            .child(
                button(
                    colors,
                    "pull",
                    if behind > 0 {
                        format!("⇣  Pull {behind}")
                    } else {
                        "⇣  Pull".to_string()
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pull(cx);
                })),
            )
            .child(
                button(
                    colors,
                    "push",
                    if ahead > 0 {
                        format!("↑  Push {ahead}")
                    } else {
                        "↑  Push".to_string()
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.push(cx);
                })),
            )
            .child(button(colors, "repository-menu", "⋯").on_click(cx.listener(
                |this, _, _, cx| {
                    this.toggle_popover(Popover::Repository, cx);
                },
            )))
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
        let empty = graph::Graph::default();
        let laid_out = self.data().map(|data| &data.graph).unwrap_or(&empty);
        let gutter = graph::gutter_width(laid_out.lanes);
        let row_width = self.row_width(gutter);
        let commits = self.commits().to_vec();
        let head = self
            .data()
            .and_then(|data| data.status.head.clone())
            .unwrap_or_default();
        let selection = if self.visible_branches.is_empty() {
            "all branches".to_string()
        } else {
            format!("{} branches", self.visible_branches.len())
        };

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
                            .child(section_label(colors, "HISTORY"))
                            .child(div().flex_1())
                            .child(button(colors, "reload", "↻").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.reload(cx);
                                },
                            )))
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
                            .child(format!("{} commits · {selection}", self.commits().len())),
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
                                    .pl(px(gutter))
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
                                    .children(laid_out.rows.iter().enumerate().map(
                                        |(index, graph_row)| {
                                            let commit = &commits[graph_row.commit];
                                            let selected = self.commit == graph_row.commit;
                                            let is_head = commit.id == head;
                                            let target = graph_row.commit;
                                            let lane_color = colors.branch(graph_row.lane);
                                            let tags: Vec<&String> = commit
                                                .refs
                                                .iter()
                                                .filter(|name| {
                                                    Some(*name) != graph_row.label.as_ref()
                                                })
                                                .collect();
                                            row()
                                                .id(("commit-row", index))
                                                .h(px(graph::ROW_HEIGHT))
                                                .w(px(row_width))
                                                .flex_none()
                                                .pl(px(gutter))
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
                                                            .font_family(EDITOR_FONT)
                                                            .text_color(rgb(colors.dim))
                                                            .child(commit.short_id.clone()),
                                                        Column::Branch => cell(width)
                                                            .gap_1()
                                                            .when_some(
                                                                graph_row.label.clone(),
                                                                |this, name| {
                                                                    this.child(badge(
                                                                        colors, name, lane_color,
                                                                    ))
                                                                },
                                                            )
                                                            .when(is_head, |this| {
                                                                this.child(badge(
                                                                    colors,
                                                                    "HEAD",
                                                                    colors.text_bright,
                                                                ))
                                                            })
                                                            .children(tags.iter().map(|name| {
                                                                badge(
                                                                    colors,
                                                                    (*name).clone(),
                                                                    colors.tag,
                                                                )
                                                            })),
                                                        Column::Author => cell(width)
                                                            .text_size(px(12.))
                                                            .text_color(rgb(colors.muted))
                                                            .child(
                                                                div()
                                                                    .min_w_0()
                                                                    .truncate()
                                                                    .child(commit.author.clone()),
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
                                                                    .child(commit.subject.clone()),
                                                            ),
                                                        Column::When => cell(width)
                                                            .text_size(px(11.))
                                                            .text_color(rgb(colors.dim))
                                                            .child(
                                                                div().min_w_0().truncate().child(
                                                                    commit
                                                                        .relative_time
                                                                        .replace(" ago", ""),
                                                                ),
                                                            ),
                                                    }
                                                }))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.select_commit(target, cx);
                                                }))
                                        },
                                    ))
                                    .child(graph::sidebar_graph(
                                        laid_out.clone(),
                                        commits.clone(),
                                        colors,
                                    )),
                            ),
                    ),
            )
            .child(self.selected_commit_panel(cx))
            .child(self.stash_panel(cx))
    }

    fn editor_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let tabs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let active = self.active_tab == index;
                let (marker, marker_color) = match tab {
                    Tab::Diff { staged: true, .. } => ("±", colors.local),
                    Tab::Diff { .. } => ("±", colors.merge),
                    Tab::Source { .. } => ("◧", colors.remote),
                };
                let name = tab
                    .path()
                    .rsplit('/')
                    .next()
                    .unwrap_or(tab.path())
                    .to_string();
                let title = match tab {
                    Tab::Diff { staged: true, .. } => format!("{name} (Index)"),
                    Tab::Diff { .. } => format!("{name} (Working Tree)"),
                    Tab::Source { .. } => name,
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
                                // Without this the click also reaches the tab
                                // row behind it, which would then activate the
                                // index that was just removed.
                                cx.stop_propagation();
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
        let path = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.path().to_string())
            .unwrap_or_default();
        let kind = match self.tabs.get(self.active_tab) {
            Some(Tab::Diff { staged: true, .. }) => "Index",
            Some(Tab::Diff { .. }) => "Working Tree",
            Some(Tab::Source { .. }) => "Source",
            None => "",
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
            .when(!kind.is_empty(), |this| {
                this.child(div().text_color(rgb(colors.dim)).child("›"))
                    .child(kind)
            })
    }

    fn code_line(&self, line_number: usize, kind: &'static str, code: String) -> impl IntoElement {
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
                    .child(if line_number == 0 {
                        String::new()
                    } else {
                        line_number.to_string()
                    }),
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

    /// Renders a unified diff produced by `git diff`.
    fn diff_editor(&self, path: &str, staged: bool) -> AnyElement {
        let colors = self.colors();
        let key = (path.to_string(), staged);
        let Some(text) = self.diffs.get(&key) else {
            return self.editor_notice("Reading the diff…").into_any_element();
        };
        if text.trim().is_empty() {
            return self
                .editor_notice("No changes in this file.")
                .into_any_element();
        }

        let (mut added, mut removed) = (0usize, 0usize);
        for line in text.lines() {
            if line.starts_with('+') && !line.starts_with("+++") {
                added += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                removed += 1;
            }
        }

        // The header lines carry no information the breadcrumb lacks.
        let body: Vec<&str> = text
            .lines()
            .skip_while(|line| {
                line.starts_with("diff --git")
                    || line.starts_with("index ")
                    || line.starts_with("--- ")
                    || line.starts_with("+++ ")
                    || line.starts_with("new file")
                    || line.starts_with("deleted file")
                    || line.starts_with("similarity ")
                    || line.starts_with("rename ")
                    || line.starts_with("old mode")
                    || line.starts_with("new mode")
            })
            .collect();

        // Line numbers follow the new side of each hunk.
        let mut number = 0usize;
        let mut rendered = Vec::new();
        for line in body {
            if let Some(start) = hunk_start(line) {
                number = start;
                rendered.push(self.hunk_header(line).into_any_element());
                continue;
            }
            let (kind, code) = match line.chars().next() {
                Some('+') => ("+", &line[1..]),
                Some('-') => ("-", &line[1..]),
                Some(' ') => (" ", &line[1..]),
                _ => (" ", line),
            };
            let shown = if kind == "-" { 0 } else { number };
            if kind != "-" {
                number += 1;
            }
            rendered.push(
                self.code_line(shown, kind, code.to_string())
                    .into_any_element(),
            );
        }

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
                    .child(section_label(
                        colors,
                        if staged {
                            "HEAD ↔ INDEX"
                        } else {
                            "INDEX ↔ WORKING TREE"
                        },
                    ))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_color(rgb(colors.local))
                            .text_size(px(13.))
                            .child(format!("+{added}")),
                    )
                    .child(
                        div()
                            .text_color(rgb(colors.red))
                            .text_size(px(13.))
                            .child(format!("−{removed}")),
                    ),
            )
            .children(rendered)
            .into_any_element()
    }

    fn hunk_header(&self, line: &str) -> impl IntoElement {
        let colors = self.colors();
        div()
            .h(px(EDITOR_LINE_HEIGHT + 6.))
            .flex_none()
            .px_4()
            .py_1()
            .font_family(EDITOR_FONT)
            .text_size(px(EDITOR_FONT_SIZE))
            .line_height(px(EDITOR_LINE_HEIGHT))
            .text_color(rgb(colors.remote))
            .child(line.to_string())
    }

    fn editor_notice(&self, text: &'static str) -> impl IntoElement {
        let colors = self.colors();
        column()
            .id("editor-notice")
            .flex_1()
            .min_h_0()
            .p_4()
            .bg(rgb(colors.editor))
            .text_size(px(13.))
            .text_color(rgb(colors.dim))
            .child(text)
    }

    fn source_editor(&self, path: &str) -> AnyElement {
        let colors = self.colors();
        let Some(text) = self.sources.get(path) else {
            return self.editor_notice("Reading the file…").into_any_element();
        };
        column()
            .id("source-editor")
            .flex_1()
            .min_h_0()
            .overflow_scroll()
            .bg(rgb(colors.editor))
            .py_2()
            .children(
                text.lines()
                    .take(4000)
                    .enumerate()
                    .map(|(offset, code)| self.code_line(offset + 1, " ", code.to_string())),
            )
            .into_any_element()
    }

    /// One column of the bottom dock: a titled, scrolling list of changes.
    fn change_column(
        &self,
        id: &'static str,
        title: String,
        rows: Vec<AnyElement>,
        empty: &'static str,
        control: impl IntoElement,
    ) -> impl IntoElement {
        let colors = self.colors();
        column()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                row()
                    .h(px(26.))
                    .flex_none()
                    .px_3()
                    .gap_2()
                    .bg(rgb(colors.editor_alt))
                    .border_t_1()
                    .border_b_1()
                    .border_color(rgb(colors.line))
                    .child(section_label(colors, title))
                    .child(div().flex_1())
                    .child(control),
            )
            .child(
                column()
                    .id(id)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(rows.is_empty(), |this| {
                        this.child(
                            div()
                                .px_3()
                                .py_2()
                                .text_size(px(12.))
                                .text_color(rgb(colors.dim))
                                .child(empty),
                        )
                    })
                    .children(rows),
            )
    }

    /// The dock under the editor: what has changed, what is staged, and the
    /// message that will commit it.
    fn changes_dock(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let files = self.changes();
        let staged_count = files.iter().filter(|file| file.is_staged()).count();
        let changed_count = files.iter().filter(|file| file.is_modified()).count();
        let changed_rows = files
            .iter()
            .enumerate()
            .filter(|(_, file)| file.is_modified())
            .map(|(index, file)| self.file_row(index, file, false, cx).into_any_element())
            .collect::<Vec<_>>();
        let staged_rows = files
            .iter()
            .enumerate()
            .filter(|(_, file)| file.is_staged())
            .map(|(index, file)| self.file_row(index, file, true, cx).into_any_element())
            .collect::<Vec<_>>();

        let bulk = |id: &'static str, glyph: &'static str| {
            div()
                .id(id)
                .px_1()
                .text_size(px(15.))
                .text_color(rgb(colors.muted))
                .cursor_pointer()
                .hover(move |this| this.text_color(rgb(colors.text_bright)))
                .child(glyph)
        };

        column()
            // Three stacked sections; the lists flex and the commit box does
            // not, so it keeps its full height as the dock is squeezed.
            .h(px(360.))
            .flex_none()
            .border_t_1()
            .border_color(rgb(colors.line))
            .bg(rgb(colors.panel))
            .child(self.change_column(
                "dock-changes",
                format!("CHANGES  {changed_count}"),
                changed_rows,
                "The working tree is clean.",
                bulk("stage-all", "＋").on_click(cx.listener(|this, _, _, cx| {
                    this.stage_all(cx);
                })),
            ))
            .child(self.change_column(
                "dock-staged",
                format!("STAGED  {staged_count}"),
                staged_rows,
                "Nothing is staged.",
                bulk("unstage-all", "−").on_click(cx.listener(|this, _, _, cx| {
                    this.unstage_all(cx);
                })),
            ))
            .child(
                column()
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .child(
                        row()
                            .h(px(26.))
                            .flex_none()
                            .px_3()
                            .bg(rgb(colors.editor_alt))
                            .border_b_1()
                            .border_color(rgb(colors.line))
                            .child(section_label(colors, "COMMIT MESSAGE")),
                    )
                    .child(
                        row()
                            .flex_none()
                            .p_2()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(self.text_field(Field::CommitMessage, cx)),
                            )
                            .child(
                                row()
                                    .id("commit-button")
                                    .h(px(28.))
                                    .px_3()
                                    .flex_none()
                                    .gap_1()
                                    .justify_center()
                                    .rounded(px(4.))
                                    .bg(rgb(if staged_count == 0 {
                                        colors.line_strong
                                    } else {
                                        colors.local
                                    }))
                                    .text_color(rgb(colors.editor))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(13.))
                                    .when(staged_count > 0, |this| {
                                        this.cursor_pointer().on_click(cx.listener(
                                            |this, _, _, cx| {
                                                this.commit_staged(cx);
                                            },
                                        ))
                                    })
                                    .child("✓")
                                    .child(if staged_count == 0 {
                                        "Commit".to_string()
                                    } else {
                                        format!("Commit {staged_count}")
                                    }),
                            ),
                    ),
            )
    }

    fn editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let body = match self.tabs.get(self.active_tab) {
            Some(Tab::Diff { path, staged }) => self.diff_editor(path, *staged),
            Some(Tab::Source { path }) => self.source_editor(path),
            None => self
                .editor_notice("Select a change or a file to open it here.")
                .into_any_element(),
        };
        column()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(rgb(colors.editor))
            .child(self.editor_tabs(cx))
            .child(self.breadcrumb())
            .child(body)
            .child(self.changes_dock(cx))
    }

    fn file_row(
        &self,
        index: usize,
        file: &git::FileStatus,
        staged: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = self.colors();
        // The letter Git itself uses, taken from whichever side changed.
        let letter = if file.untracked {
            "?"
        } else if file.unmerged {
            "!"
        } else if staged {
            match file.index {
                'A' => "A",
                'D' => "D",
                'R' => "R",
                _ => "M",
            }
        } else {
            match file.worktree {
                'D' => "D",
                'A' => "A",
                _ => "M",
            }
        };
        let color = match letter {
            "A" => colors.local,
            "D" => colors.red,
            "?" => colors.dim,
            "!" => colors.merge,
            _ => colors.remote,
        };
        let name = file
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&file.path)
            .to_string();
        let directory = file
            .path
            .rsplit_once('/')
            .map(|(head, _)| head.to_string())
            .unwrap_or_default();

        row()
            .id((if staged { "staged-row" } else { "working-row" }, index))
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
                    .text_color(rgb(color))
                    .child(letter),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .text_color(rgb(colors.text))
                    .child(name),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(colors.dim))
                    .child(directory),
            )
            .child(
                div()
                    .id(("discard-file", index))
                    .w(px(18.))
                    .text_center()
                    .text_size(px(13.))
                    .text_color(rgb(colors.dim))
                    .cursor_pointer()
                    .hover(move |this| this.text_color(rgb(colors.red)))
                    .child("⟲")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.discard_file(index, cx);
                    })),
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
                        // Staging a file should not also open it in the editor.
                        cx.stop_propagation();
                        this.stage_side(index, staged, cx);
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_file_side(index, staged, cx);
            }))
    }

    /// Real repository state: the branch, its upstream, and the refs Git
    /// reports, all read from the repository rather than from a fixture.
    fn repository_state(&self) -> impl IntoElement {
        let colors = self.colors();
        let field = |label: &'static str, value: String, accent: u32| {
            row()
                .text_size(px(13.))
                .child(
                    div()
                        .w(px(86.))
                        .flex_none()
                        .text_color(rgb(colors.muted))
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(accent))
                        .child(value),
                )
        };

        let mut body: Vec<gpui::AnyElement> = Vec::new();
        match &self.repo {
            RepoState::Loading => {
                body.push(field("Status", "Reading…".into(), colors.muted).into_any_element())
            }
            RepoState::Failed(reason) => {
                body.push(field("Status", "Unavailable".into(), colors.red).into_any_element());
                body.push(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(colors.dim))
                        .child(reason.clone())
                        .into_any_element(),
                );
            }
            RepoState::Ready(data) => {
                let name = data
                    .root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| data.root.display().to_string());
                body.push(field("Repository", name, colors.text).into_any_element());
                body.push(
                    field(
                        "HEAD",
                        match (&data.status.branch, data.status.detached) {
                            (Some(branch), _) => branch.clone(),
                            (None, true) => "detached".into(),
                            (None, false) => "unborn".into(),
                        },
                        colors.local,
                    )
                    .into_any_element(),
                );
                body.push(
                    field(
                        "Upstream",
                        data.status
                            .upstream
                            .clone()
                            .unwrap_or_else(|| "none".into()),
                        colors.remote,
                    )
                    .into_any_element(),
                );
                body.push(
                    field(
                        "Status",
                        format!(
                            "{} ahead · {} behind",
                            data.status.ahead, data.status.behind
                        ),
                        colors.merge,
                    )
                    .into_any_element(),
                );
                if let Some(commit) = self.selected_commit() {
                    body.push(
                        field("Commit", commit.short_id.clone(), colors.text).into_any_element(),
                    );
                }
            }
        }

        column()
            .flex_none()
            .child(
                row()
                    .h(px(26.))
                    .px_3()
                    .child(section_label(colors, "REPOSITORY")),
            )
            .child(column().px_3().pb_3().gap_1().children(body))
    }

    /// Every ref Git reports. Selecting one jumps the history to its commit.
    fn refs_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let references = self
            .data()
            .map(|data| data.references.clone())
            .unwrap_or_default();
        let count = |kind: git::RefKind| references.iter().filter(|r| r.kind == kind).count();
        let summary = format!(
            "{} local · {} remote · {} tags",
            count(git::RefKind::Local),
            count(git::RefKind::Remote),
            count(git::RefKind::Tag)
        );

        column()
            .flex_none()
            .max_h(px(190.))
            .child(
                row()
                    .h(px(28.))
                    .flex_none()
                    .px_3()
                    .gap_2()
                    .child(section_label(colors, "REFS"))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(colors.dim))
                            .child(summary),
                    ),
            )
            .child(
                div()
                    .id("refs-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_3()
                    .pb_2()
                    .child(
                        div().flex().flex_wrap().gap_1().children(
                            references
                                .into_iter()
                                .enumerate()
                                .map(|(index, reference)| {
                                    let color = match reference.kind {
                                        git::RefKind::Local => colors.local,
                                        git::RefKind::Remote => colors.remote,
                                        git::RefKind::Tag => colors.tag,
                                    };
                                    let target = reference.target.clone();
                                    div()
                                        .id(("ref-badge", index))
                                        .cursor_pointer()
                                        .hover(|this| this.opacity(0.75))
                                        .child(badge(colors, reference.name.clone(), color))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            match this.commits().iter().position(|c| c.id == target)
                                            {
                                                Some(row) => this.select_commit(row, cx),
                                                None => {
                                                    this.message =
                                                        "That ref is outside the loaded history"
                                                            .into();
                                                    cx.notify();
                                                }
                                            }
                                        }))
                                }),
                        ),
                    ),
            )
    }

    fn right_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let open_source = match self.tabs.get(self.active_tab) {
            Some(Tab::Source { path }) => Some(path.clone()),
            _ => None,
        };
        let tree_rows = self
            .visible_tree()
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                let collapsed = self.collapsed.contains(&entry.path);
                let selected = open_source.as_deref() == Some(entry.path.as_str());
                let (glyph, glyph_color) = if entry.directory {
                    (if collapsed { "›" } else { "⌄" }, colors.muted)
                } else {
                    ("·", colors.dim)
                };
                let path = entry.path.clone();
                let directory = entry.directory;
                row()
                    .id(("tree-row", index))
                    .h(px(26.))
                    .flex_none()
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
                            .child(entry.name.clone()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if directory {
                            this.toggle_folder(path.clone(), cx);
                        } else {
                            this.open_source(path.clone(), cx);
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
                    .border_b_1()
                    .border_color(rgb(colors.line))
                    .child(section_label(colors, "SOURCE CONTROL")),
            )
            .child(self.repository_state())
            .child(div().h(px(1.)).flex_none().bg(rgb(colors.line)))
            .child(self.refs_panel(cx))
            .child(
                column()
                    .flex_1()
                    .min_h_0()
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .child(
                        row()
                            .h(px(28.))
                            .flex_none()
                            .px_3()
                            .child(section_label(colors, "SOURCE FILE TREE")),
                    )
                    .child(
                        column()
                            .id("file-tree")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .children(tree_rows),
                    ),
            )
    }

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
        let branches: Vec<String> = self
            .data()
            .map(|data| data.branches.clone())
            .unwrap_or_default();
        let head = self
            .data()
            .and_then(|data| data.status.branch.clone())
            .unwrap_or_default();
        let chosen = self.visible_branches.len();

        self.picker("branches-panel", "Branches", cx)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(colors.dim))
                    .child(if chosen == 0 {
                        format!(
                            "Showing every ref. Choose up to {} to narrow the history.",
                            graph::LANE_CAPACITY
                        )
                    } else {
                        format!("{chosen} of {} branches shown.", branches.len())
                    }),
            )
            .children(branches.iter().take(60).enumerate().map(|(index, branch)| {
                let position = self.visible_branches.iter().position(|b| b == branch);
                let full = position.is_none() && chosen >= graph::LANE_CAPACITY;
                let name = branch.clone();
                Self::picker_row(
                    colors,
                    SharedString::from(format!("branch-{index}")),
                    branch.clone(),
                    colors.branch(position.unwrap_or(0)),
                    position.is_some(),
                    !full,
                )
                .when(*branch == head, |this| {
                    this.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(colors.local))
                            .child("HEAD"),
                    )
                })
                .child(
                    div()
                        .id(("switch-branch", index))
                        .px_1()
                        .rounded(px(3.))
                        .text_size(px(11.))
                        .text_color(rgb(colors.remote))
                        .cursor_pointer()
                        .hover(move |this| this.bg(rgb(colors.line_strong)))
                        .child("switch")
                        .on_click(cx.listener({
                            let branch = branch.clone();
                            move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.popover = Popover::None;
                                this.switch_to_branch(branch.clone(), cx);
                            }
                        })),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_branch(name.clone(), cx);
                }))
            }))
            .child(
                row()
                    .id("show-all-branches")
                    .h(px(28.))
                    .px_2()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(rgb(colors.remote))
                    .hover(move |this| this.bg(rgb(colors.hover)))
                    .child("Show every ref")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.visible_branches.clear();
                        this.commit = 0;
                        this.reload(cx);
                    })),
            )
    }
    /// A button that is visibly unavailable when the action would fail.
    fn action_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: impl Into<String>,
        enabled: bool,
        danger: bool,
    ) -> Stateful<Div> {
        let colors = self.colors();
        let border = if !enabled {
            colors.line
        } else if danger {
            colors.red
        } else {
            colors.line_strong
        };
        let text = if !enabled {
            colors.dim
        } else if danger {
            colors.red
        } else {
            colors.text
        };
        row()
            .id(id)
            .h(px(26.))
            .px_2()
            .gap_1()
            .justify_center()
            .flex_none()
            .rounded(px(4.))
            .border_1()
            .border_color(rgb(border))
            .text_size(px(12.))
            .text_color(rgb(text))
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(rgb(colors.hover)))
            })
            .child(div().text_size(px(13.)).child(icon))
            .child(label.into())
    }

    /// The selected commit and everything that can be done to it. Buttons
    /// enable only when Git would actually accept the operation.
    fn selected_commit_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let actions = self.commit_actions();
        let selected = self.selected_commit().cloned();

        let detail = match &selected {
            Some(commit) => column()
                .gap_1()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.))
                        .text_color(rgb(colors.text_bright))
                        .child(commit.subject.clone()),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(colors.dim))
                        .child(format!(
                            "{} · {} · {}",
                            commit.short_id, commit.author, commit.relative_time
                        )),
                )
                .into_any_element(),
            None => div()
                .text_size(px(12.))
                .text_color(rgb(colors.dim))
                .child("Select a commit in the history above.")
                .into_any_element(),
        };

        column()
            .flex_none()
            .px_3()
            .py_2()
            .gap_2()
            .border_t_1()
            .border_color(rgb(colors.line))
            .child(
                row()
                    .child(section_label(colors, "SELECTED COMMIT"))
                    .child(div().flex_1())
                    .when(self.selection_is_head(), |this| {
                        this.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(colors.local))
                                .child("HEAD"),
                        )
                    }),
            )
            .child(detail)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        self.action_button(
                            "act-checkout",
                            "◎",
                            "Check out",
                            actions.checkout,
                            false,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.checkout_selected(cx);
                        })),
                    )
                    .child(
                        self.action_button("act-revert", "⟲", "Revert", actions.revert, false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.revert_selected(cx);
                            })),
                    )
                    .child(
                        self.action_button("act-soft", "←", "Reset soft", actions.reset, false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reset_to_selected(git::ResetMode::Soft, cx);
                            })),
                    )
                    .child(
                        self.action_button("act-mixed", "⇐", "Reset mixed", actions.reset, false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reset_to_selected(git::ResetMode::Mixed, cx);
                            })),
                    )
                    .child(
                        self.action_button("act-hard", "⇤", "Reset hard", actions.reset, true)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reset_to_selected(git::ResetMode::Hard, cx);
                            })),
                    )
                    .child(
                        self.action_button("act-amend", "✎", "Amend", actions.amend, true)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.amend(cx);
                            })),
                    ),
            )
            .when(selected.is_some() && !actions.revert, |this| {
                this.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(colors.dim))
                        .child("Revert needs an empty index."),
                )
            })
    }

    /// The stash list and the controls that act on it.
    fn stash_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let stashes = self
            .data()
            .map(|data| data.stashes.clone())
            .unwrap_or_default();
        let dirty = self
            .data()
            .map(|data| data.status.changed().count() > 0)
            .unwrap_or(false);

        column()
            .flex_none()
            .max_h(px(164.))
            .border_t_1()
            .border_color(rgb(colors.line))
            .child(
                row()
                    .h(px(28.))
                    .flex_none()
                    .px_3()
                    .gap_2()
                    .child(section_label(colors, format!("STASH  {}", stashes.len())))
                    .child(div().flex_1())
                    .child(
                        self.action_button("stash-push", "↓", "Stash", dirty, false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stash(cx);
                            })),
                    ),
            )
            .child(
                column()
                    .id("stash-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(stashes.is_empty(), |this| {
                        this.child(
                            div()
                                .px_3()
                                .pb_2()
                                .text_size(px(12.))
                                .text_color(rgb(colors.dim))
                                .child("Nothing stashed."),
                        )
                    })
                    .children(stashes.into_iter().enumerate().map(|(index, stash)| {
                        let apply = stash.reference.clone();
                        let discard = stash.reference.clone();
                        row()
                            .h(px(26.))
                            .flex_none()
                            .px_3()
                            .gap_2()
                            .hover(move |this| this.bg(rgb(colors.hover)))
                            .child(
                                div()
                                    .w(px(34.))
                                    .flex_none()
                                    .font_family(EDITOR_FONT)
                                    .text_size(px(11.))
                                    .text_color(rgb(colors.dim))
                                    .child(format!("{{{index}}}")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(rgb(colors.text))
                                    .child(stash.message.clone()),
                            )
                            .child(
                                div()
                                    .id(("stash-apply", index))
                                    .px_1()
                                    .rounded(px(3.))
                                    .text_size(px(11.))
                                    .text_color(rgb(colors.remote))
                                    .cursor_pointer()
                                    .hover(move |this| this.bg(rgb(colors.line_strong)))
                                    .child("↑")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.apply_stash(apply.clone(), cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id(("stash-drop", index))
                                    .px_1()
                                    .rounded(px(3.))
                                    .text_size(px(11.))
                                    .text_color(rgb(colors.dim))
                                    .cursor_pointer()
                                    .hover(move |this| this.text_color(rgb(colors.red)))
                                    .child("×")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.drop_stash(discard.clone(), cx);
                                    })),
                            )
                    })),
            )
    }

    /// A single-line field backed by the platform text input handler.
    fn text_field(&self, field: Field, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let text = self.input(field).to_string();
        let focused = self.composing == Some(field);
        row()
            .id(SharedString::from(format!("field-{}", field.placeholder())))
            .h(px(28.))
            .px_2()
            .rounded(px(4.))
            .border_1()
            .border_color(rgb(if focused {
                colors.local
            } else {
                colors.line_strong
            }))
            .bg(rgb(colors.editor))
            .cursor_text()
            .text_size(px(13.))
            .text_color(rgb(if text.is_empty() {
                colors.dim
            } else {
                colors.text
            }))
            .child(input::field(field, cx))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.focus_field(field, cx);
            }))
    }

    /// Repository-wide actions: branches, stashing, and cleaning.
    fn repository_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let stashes = self
            .data()
            .map(|data| data.status.files.iter().filter(|f| f.untracked).count())
            .unwrap_or(0);

        self.picker("repository-panel", "Repository", cx)
            .child(section_label(colors, "NEW BRANCH"))
            .child(self.text_field(Field::BranchName, cx))
            .child(
                button(colors, "create-branch", "Create and switch").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.popover = Popover::None;
                        this.create_branch(cx);
                    },
                )),
            )
            .child(section_label(colors, "STASH"))
            .child(self.text_field(Field::StashMessage, cx))
            .child(
                row()
                    .gap_2()
                    .child(
                        button(colors, "stash-push", "Stash changes").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.popover = Popover::None;
                                this.stash(cx);
                            },
                        )),
                    )
                    .child(
                        button(colors, "stash-pop", "Restore stash").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.popover = Popover::None;
                                this.stash_pop(cx);
                            },
                        )),
                    ),
            )
            .child(section_label(colors, "DESTROYS WORK"))
            .child(
                row()
                    .id("clean-untracked")
                    .h(px(28.))
                    .px_2()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(rgb(colors.red))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(rgb(colors.red))
                    .hover(move |this| this.bg(rgb(colors.hover)))
                    .child(format!("Delete {stashes} untracked file(s)"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.popover = Popover::None;
                        this.clean(cx);
                    })),
            )
    }

    /// The gate every destructive action passes through.
    fn confirm_dialog(&self, confirm: &Confirm, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        column()
            .id("confirm-dialog")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000099))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                column()
                    .w(px(420.))
                    .p_4()
                    .gap_3()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(colors.red))
                    .bg(rgb(colors.elevated))
                    .shadow_lg()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(15.))
                            .text_color(rgb(colors.text_bright))
                            .child(confirm.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(colors.muted))
                            .child(confirm.detail.clone()),
                    )
                    .child(
                        row()
                            .gap_2()
                            .justify_end()
                            .child(button(colors, "confirm-cancel", "Cancel").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.confirm = None;
                                    this.message = "Cancelled".into();
                                    cx.notify();
                                }),
                            ))
                            .child(
                                row()
                                    .id("confirm-go")
                                    .h(px(28.))
                                    .px_3()
                                    .justify_center()
                                    .rounded(px(4.))
                                    .bg(rgb(colors.red))
                                    .text_color(rgb(colors.editor))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(13.))
                                    .cursor_pointer()
                                    .child("Yes, do it")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.run_confirmed(cx);
                                    })),
                            ),
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

    /// The status bar reports the real repository VGit was launched from, so
    /// the counts here are computed by Git rather than read from the fixture.
    /// Reports the real repository: its branch, how far it has diverged from
    /// its upstream, and how many paths are changed and staged.
    fn statusbar(&self) -> impl IntoElement {
        let colors = self.colors();
        let (branch, divergence, counts) = match &self.repo {
            RepoState::Loading => ("⑂ …".to_string(), String::new(), String::new()),
            RepoState::Failed(_) => ("⑂ no repository".to_string(), String::new(), String::new()),
            RepoState::Ready(data) => {
                let status = &data.status;
                let branch = match (&status.branch, status.detached) {
                    (Some(name), _) => format!("⑂ {name}"),
                    (None, true) => "⑂ detached".to_string(),
                    (None, false) => "⑂ unborn".to_string(),
                };
                let divergence = if status.upstream.is_none() {
                    "no upstream".to_string()
                } else if status.ahead == 0 && status.behind == 0 {
                    "up to date".to_string()
                } else {
                    format!("↑{} ↓{}", status.ahead, status.behind)
                };
                (
                    branch,
                    divergence,
                    format!(
                        "● {}  ✓ {}",
                        status.changed().count(),
                        status.staged().count()
                    ),
                )
            }
        };

        row()
            .h(px(24.))
            .flex_none()
            .px_2()
            .gap_3()
            .bg(rgb(colors.local))
            .text_color(rgb(colors.editor))
            .text_size(px(12.))
            .child(branch)
            .when(!divergence.is_empty(), |this| this.child(divergence))
            .when(!counts.is_empty(), |this| this.child(counts))
            .child(div().flex_1().child(self.message.clone()))
            .child("UTF-8")
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
                this.select_file(this.file, cx);
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
                    None => {
                        if let Some(entry) =
                            this.visible_tree().into_iter().find(|row| !row.directory)
                        {
                            this.open_source(entry.path, cx);
                        }
                    }
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
                this.confirm = None;
                this.composing = None;
                cx.notify();
            }))
            .on_action(|_: &Close, window, _| window.remove_window())
            // Captured before the key bindings run, so typing a message
            // cannot trigger Space to stage or Up to move the selection.
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if this.type_into_field(event, cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
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
            .when(self.popover == Popover::Repository, |this| {
                this.child(self.repository_panel(cx))
            })
            .when_some(self.confirm.as_ref(), |this, confirm| {
                this.child(self.confirm_dialog(confirm, cx))
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
        graph::gutter_width(graph::LANE_CAPACITY) + COLUMNS.iter().map(|c| c.width()).sum::<f32>()
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

    /// Closing a tab must always leave a valid active index. This is the
    /// arithmetic behind the crash where closing a tab left `active_tab`
    /// pointing one past the end of the list.
    #[test]
    fn closing_a_tab_always_leaves_a_valid_active_index() {
        for total in 2..6usize {
            for active in 0..total {
                for closed in 0..total {
                    let remaining = total - 1;
                    let next = active_after_close(active, closed, remaining);
                    assert!(
                        next < remaining,
                        "closing {closed} of {total} with {active} active gave {next}"
                    );
                }
            }
        }
    }

    #[test]
    fn closing_an_earlier_tab_keeps_the_same_tab_active() {
        // Tabs [a, b, c] with c active; closing a leaves c active at index 1.
        assert_eq!(active_after_close(2, 0, 2), 1);
        // Closing a later tab does not move the selection.
        assert_eq!(active_after_close(0, 1, 2), 0);
    }

    #[test]
    fn closing_the_active_tab_falls_back_within_the_list() {
        // Closing the last tab while it is active steps back one.
        assert_eq!(active_after_close(2, 2, 2), 1);
        // Closing an active tab in the middle holds the position.
        assert_eq!(active_after_close(1, 1, 3), 1);
        assert_eq!(active_after_close(1, 1, 2), 1);
    }

    // ---- The source tree ------------------------------------------------

    fn tree_of(paths: &[&str]) -> Vec<TreeRow> {
        build_tree(&paths.iter().map(|p| p.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn a_flat_list_of_paths_becomes_an_indented_tree() {
        let rows = tree_of(&["src/git.rs", "src/main.rs"]);
        let shape: Vec<(usize, &str, bool)> = rows
            .iter()
            .map(|row| (row.depth, row.name.as_str(), row.directory))
            .collect();
        assert_eq!(
            shape,
            vec![
                (0, "src", true),
                (1, "git.rs", false),
                (1, "main.rs", false),
            ]
        );
    }

    /// A directory is emitted once, no matter how many files it holds.
    #[test]
    fn a_shared_directory_is_not_repeated() {
        let rows = tree_of(&["a/b/one.rs", "a/b/two.rs", "a/three.rs"]);
        let directories: Vec<&str> = rows
            .iter()
            .filter(|row| row.directory)
            .map(|row| row.path.as_str())
            .collect();
        assert_eq!(directories, vec!["a", "a/b"]);
    }

    /// Leaving a directory and entering a sibling must not nest the sibling
    /// inside the one just closed.
    #[test]
    fn leaving_a_directory_returns_to_the_right_depth() {
        let rows = tree_of(&["a/deep/one.rs", "b/two.rs"]);
        let b = rows.iter().find(|row| row.path == "b").expect("b");
        assert_eq!(b.depth, 0);
        let two = rows.iter().find(|row| row.path == "b/two.rs").expect("two");
        assert_eq!(two.depth, 1);
    }

    #[test]
    fn every_tree_row_keeps_its_full_path() {
        for row in tree_of(&["a/b/c.rs"]) {
            assert!(row.path.ends_with(&row.name), "{row:?}");
        }
    }

    #[test]
    fn a_file_at_the_root_has_no_directory_above_it() {
        let rows = tree_of(&["README.md"]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].depth, 0);
        assert!(!rows[0].directory);
    }

    #[test]
    fn an_empty_repository_has_an_empty_tree() {
        assert!(tree_of(&[]).is_empty());
    }

    // ---- Diff hunk headers ----------------------------------------------

    /// Line numbers in the diff follow the new side of the hunk.
    #[test]
    fn a_hunk_header_yields_its_new_side_start_line() {
        assert_eq!(hunk_start("@@ -1,4 +1,6 @@"), Some(1));
        assert_eq!(hunk_start("@@ -18,10 +24,12 @@ fn render()"), Some(24));
        assert_eq!(hunk_start("@@ -0,0 +1 @@"), Some(1));
    }

    #[test]
    fn an_ordinary_line_is_not_read_as_a_hunk_header() {
        assert_eq!(hunk_start("+ added line"), None);
        assert_eq!(hunk_start("   context"), None);
        assert_eq!(hunk_start(""), None);
        assert_eq!(hunk_start("@@ malformed"), None);
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
