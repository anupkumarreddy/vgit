use crate::{git, theme::Palette};
use gpui::{Bounds, PathBuilder, canvas, point, prelude::*, px, quad, rgb, size};
use std::collections::HashMap;

pub const ROW_HEIGHT: f32 = 28.;

/// How many branches the branch picker will let you choose at once.
pub const LANE_CAPACITY: usize = 5;

/// A useful stress-test size; rendering has no lane cap. The history table
/// scrolls horizontally so unrelated rails never overlap.
#[cfg(test)]
pub const MAX_LANES: usize = 10;

/// Horizontal center of the first lane, and the spacing between lanes.
const LANE_ORIGIN: f32 = 22.;
const LANE_STEP: f32 = 23.;

/// Radius of the rounded corners where a rail turns into a lane connector.
const CORNER: f32 = 6.;
/// Distance from the turning commit to the horizontal connector segment.
const JOG: f32 = 12.;

/// The most segments [`edge_route`] can produce: two rails, two corners, and
/// the horizontal connector between them.
const MAX_SEGMENTS: usize = 5;

/// Width of the gutter needed to draw `lanes` rails plus a node radius.
pub fn gutter_width(lanes: usize) -> f32 {
    LANE_ORIGIN + (lanes.max(1) - 1) as f32 * LANE_STEP + 16.
}

/// One visible row of history: the commit it shows, the lane it was given, and
/// the rows its parents occupy.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub commit: usize,
    pub lane: usize,
    pub parents: Vec<usize>,
    /// The branch this lane belongs to, taken from the ref that opened it.
    /// A commit reachable only from a merged-away tip has no label.
    pub label: Option<String>,
}

/// A laid-out history.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub rows: Vec<Row>,
    /// How many lanes are actually occupied, for sizing the gutter.
    pub lanes: usize,
}

/// Assigns a lane to every commit.
///
/// A commit does not belong to a branch in Git, so lanes are derived from
/// topology rather than read off the commit. Walking newest-first, each active
/// lane remembers the commit it is waiting for. A commit takes the lane that
/// was waiting for it, or a free lane if it is the tip of something new, and
/// then hands its lane to its first parent. Additional parents of a merge open
/// lanes of their own, which is what makes a merge visibly rejoin.
///
/// `commits` must be ordered newest-first, as `git log` returns them.
pub fn assign_lanes(commits: &[git::Commit]) -> Graph {
    let row_of: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(index, commit)| (commit.id.as_str(), index))
        .collect();

    // What each lane is waiting for, and the branch it was opened for.
    let mut waiting: Vec<Option<String>> = Vec::new();
    let mut labels: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    let mut widest = 1;

    let open_lane =
        |waiting: &mut Vec<Option<String>>, labels: &mut Vec<Option<String>>| -> usize {
            match waiting.iter().position(Option::is_none) {
                Some(lane) => lane,
                None => {
                    waiting.push(None);
                    labels.push(None);
                    waiting.len() - 1
                }
            }
        };

    for (index, commit) in commits.iter().enumerate() {
        let expecting: Vec<usize> = waiting
            .iter()
            .enumerate()
            .filter(|(_, want)| want.as_deref() == Some(commit.id.as_str()))
            .map(|(lane, _)| lane)
            .collect();

        let lane = match expecting.first() {
            Some(&lane) => lane,
            None => {
                let lane = open_lane(&mut waiting, &mut labels);
                labels[lane] = branch_label(commit);
                lane
            }
        };

        // Several rails converging on one commit collapse into its lane.
        for &merged in expecting.iter().skip(1) {
            waiting[merged] = None;
            labels[merged] = None;
        }

        // A tip that carries a branch name renames the rail it sits on.
        if let Some(name) = branch_label(commit) {
            labels[lane] = Some(name);
        }

        let mut parents = Vec::new();
        for (position, parent) in commit.parents.iter().enumerate() {
            if let Some(&row) = row_of.get(parent.as_str()) {
                parents.push(row);
            }
            if position == 0 {
                waiting[lane] = Some(parent.clone());
            } else if !waiting
                .iter()
                .any(|want| want.as_deref() == Some(parent.as_str()))
            {
                let opened = open_lane(&mut waiting, &mut labels);
                waiting[opened] = Some(parent.clone());
                labels[opened] = labels[lane].clone();
            }
        }
        if commit.parents.is_empty() {
            waiting[lane] = None;
        }

        widest = widest.max(
            waiting
                .iter()
                .filter(|want| want.is_some())
                .count()
                .max(lane + 1),
        );
        rows.push(Row {
            commit: index,
            lane,
            parents,
            label: labels[lane].clone(),
        });
    }

    Graph {
        rows,
        lanes: widest.max(1),
    }
}

/// The branch a commit is a tip of, preferring a local branch over a remote
/// one. Tags are not branches and never name a rail.
fn branch_label(commit: &git::Commit) -> Option<String> {
    commit
        .references
        .iter()
        .find(|reference| reference.kind == git::RefKind::Local)
        .or_else(|| {
            commit
                .references
                .iter()
                .find(|reference| reference.kind == git::RefKind::Remote)
        })
        .map(|reference| reference.name.clone())
}

/// One piece of a routed connector, in canvas-local pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Line {
        x: f32,
        y: f32,
    },
    /// Quadratic curve to (`x`, `y`) through a single control point.
    Curve {
        x: f32,
        y: f32,
        cx: f32,
        cy: f32,
    },
}

impl Segment {
    #[cfg(test)]
    fn end(self) -> (f32, f32) {
        match self {
            Segment::Line { x, y } | Segment::Curve { x, y, .. } => (x, y),
        }
    }
}

/// A routed connector from a commit to one of its parents.
pub struct EdgeRoute {
    pub start: (f32, f32),
    segments: [Segment; MAX_SEGMENTS],
    len: usize,
}

impl EdgeRoute {
    pub fn segments(&self) -> &[Segment] {
        &self.segments[..self.len]
    }
}

fn lane_x(lane: usize) -> f32 {
    LANE_ORIGIN + lane as f32 * LANE_STEP
}

fn row_y(row: usize) -> f32 {
    row as f32 * ROW_HEIGHT + ROW_HEIGHT / 2.
}

/// Routes the connector between two rows, each given as `(row, lane)`.
///
/// Rows sharing a lane are joined by a straight rail. Across lanes the two
/// rails are joined by a horizontal connector with one small rounded corner at
/// each end, rather than a single long sweeping curve. The connector sits
/// beside the endpoint on the lower lane, so the side branch keeps the long
/// vertical run: just below a branch point, just above a merge target.
pub fn edge_route(child: (usize, usize), parent: (usize, usize)) -> EdgeRoute {
    let (sx, sy) = (lane_x(child.1), row_y(child.0));
    let (ex, ey) = (lane_x(parent.1), row_y(parent.0));
    let mut segments = [Segment::Line { x: ex, y: ey }; MAX_SEGMENTS];
    let mut len = 1;

    if sx != ex {
        let (dx, dy) = (ex - sx, ey - sy);
        let radius = CORNER.min(dx.abs() / 2.).min(dy.abs() / 2.);
        let jog = JOG.clamp(radius, (dy.abs() - radius).max(radius));
        let (hdir, vdir) = (dx.signum(), dy.signum());
        let join = if child.1 < parent.1 {
            sy + jog * vdir
        } else {
            ey - jog * vdir
        };

        segments = [
            Segment::Line {
                x: sx,
                y: join - radius * vdir,
            },
            Segment::Curve {
                x: sx + radius * hdir,
                y: join,
                cx: sx,
                cy: join,
            },
            Segment::Line {
                x: ex - radius * hdir,
                y: join,
            },
            Segment::Curve {
                x: ex,
                y: join + radius * vdir,
                cx: ex,
                cy: join,
            },
            Segment::Line { x: ex, y: ey },
        ];
        len = MAX_SEGMENTS;
    }

    EdgeRoute {
        start: (sx, sy),
        segments,
        len,
    }
}

/// The color of a commit's node. Merges keep the merge amber so they stay
/// distinguishable from the branch rails around them.
fn commit_color(commit: &git::Commit, lane: usize, colors: Palette) -> u32 {
    if commit.is_merge() {
        colors.merge
    } else {
        colors.branch(lane)
    }
}

/// Paints the commit graph for the history sidebar.
///
/// Every lane is a straight vertical rail, joined across lanes by
/// [`edge_route`]. Regular commits use a filled inner dot; merge commits use a
/// hollow inner dot so topology is readable without relying on color alone.
pub fn sidebar_graph(graph: Graph, commits: Vec<git::Commit>, colors: Palette) -> impl IntoElement {
    let height = graph.rows.len() as f32 * ROW_HEIGHT;
    let width = gutter_width(graph.lanes);
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));

            for (index, row) in graph.rows.iter().enumerate() {
                for &parent in &row.parents {
                    let Some(parent_row) = graph.rows.get(parent) else {
                        continue;
                    };
                    let route = edge_route((index, row.lane), (parent, parent_row.lane));
                    let mut path = PathBuilder::stroke(px(2.));
                    path.move_to(at(route.start.0, route.start.1));
                    for segment in route.segments() {
                        match *segment {
                            Segment::Line { x, y } => path.line_to(at(x, y)),
                            Segment::Curve { x, y, cx, cy } => path.curve_to(at(x, y), at(cx, cy)),
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(colors.branch(parent_row.lane)));
                    }
                }
            }

            for (index, row) in graph.rows.iter().enumerate() {
                let Some(commit) = commits.get(row.commit) else {
                    continue;
                };
                let center = at(lane_x(row.lane), row_y(index));
                let color = commit_color(commit, row.lane, colors);

                window.paint_quad(quad(
                    Bounds::new(center - point(px(6.), px(6.)), size(px(12.), px(12.))),
                    px(6.),
                    rgb(colors.sidebar),
                    px(2.),
                    rgb(color),
                    Default::default(),
                ));

                let merge = commit.is_merge();
                window.paint_quad(quad(
                    Bounds::new(center - point(px(2.5), px(2.5)), size(px(5.), px(5.))),
                    px(2.5),
                    rgb(if merge { colors.sidebar } else { color }),
                    px(if merge { 1.5 } else { 0. }),
                    rgb(color),
                    Default::default(),
                ));
            }
        },
    )
    .w(px(width))
    .h(px(height))
    .absolute()
    .top_0()
    .left_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a commit with the given id, parents, and ref decoration.
    fn commit(id: &str, parents: &[&str], refs: &[&str]) -> git::Commit {
        git::Commit {
            id: id.to_string(),
            short_id: id.to_string(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            author: "Test".into(),
            email: "test@example.invalid".into(),
            timestamp: 0,
            relative_time: "now".into(),
            refs: refs.iter().map(|r| r.to_string()).collect(),
            references: refs
                .iter()
                .map(|name| git::Reference {
                    name: name.to_string(),
                    target: id.to_string(),
                    kind: match *name {
                        "origin/main" => git::RefKind::Remote,
                        "v1.2.0" => git::RefKind::Tag,
                        _ => git::RefKind::Local,
                    },
                })
                .collect(),
            subject: format!("Commit {id}"),
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    /// A straight chain stays in one lane.
    #[test]
    fn a_linear_history_uses_a_single_lane() {
        let commits = vec![
            commit("c", &["b"], &["main"]),
            commit("b", &["a"], &[]),
            commit("a", &[], &[]),
        ];
        let graph = assign_lanes(&commits);
        assert_eq!(graph.lanes, 1);
        assert!(graph.rows.iter().all(|row| row.lane == 0));
        assert_eq!(graph.rows[0].parents, vec![1]);
        assert_eq!(graph.rows[2].parents, Vec::<usize>::new());
    }

    /// A merge puts its second parent on a lane of its own, and that lane
    /// closes again where the side branch joins the trunk.
    #[test]
    fn a_merge_opens_a_second_lane_that_closes_at_the_fork() {
        // m -> (t, s); t -> base; s -> base; base
        let commits = vec![
            commit("m", &["t", "s"], &["main"]),
            commit("t", &["base"], &[]),
            commit("s", &["base"], &["feature"]),
            commit("base", &[], &[]),
        ];
        let graph = assign_lanes(&commits);

        assert_eq!(graph.rows[0].lane, 0, "the merge stays on the trunk");
        assert_eq!(graph.rows[1].lane, 0, "the first parent keeps the lane");
        assert_ne!(graph.rows[2].lane, 0, "the second parent gets its own lane");
        assert_eq!(graph.rows[3].lane, 0, "the fork point returns to the trunk");
        assert_eq!(graph.lanes, 2);

        // Both sides of the merge point at the fork.
        assert_eq!(graph.rows[1].parents, vec![3]);
        assert_eq!(graph.rows[2].parents, vec![3]);
    }

    #[test]
    fn a_merge_records_both_parents() {
        let commits = vec![
            commit("m", &["t", "s"], &[]),
            commit("t", &[], &[]),
            commit("s", &[], &[]),
        ];
        let graph = assign_lanes(&commits);
        assert_eq!(graph.rows[0].parents, vec![1, 2]);
    }

    /// Two unrelated tips each get a lane, and neither is dropped.
    #[test]
    fn independent_tips_get_their_own_lanes() {
        let commits = vec![
            commit("a2", &["a1"], &["main"]),
            commit("b2", &["b1"], &["other"]),
            commit("a1", &[], &[]),
            commit("b1", &[], &[]),
        ];
        let graph = assign_lanes(&commits);
        assert_ne!(graph.rows[0].lane, graph.rows[1].lane);
        assert_eq!(graph.rows[0].lane, graph.rows[2].lane);
        assert_eq!(graph.rows[1].lane, graph.rows[3].lane);
    }

    /// A parent outside the loaded window is simply not drawn, rather than
    /// producing an edge to a row that does not exist.
    #[test]
    fn a_parent_beyond_the_loaded_history_is_dropped() {
        let commits = vec![commit("b", &["a"], &["main"])];
        let graph = assign_lanes(&commits);
        assert!(graph.rows[0].parents.is_empty());
    }

    /// Every edge must point at a row that exists, and always downwards.
    #[test]
    fn every_edge_points_at_a_later_row() {
        let commits = vec![
            commit("f", &["e", "c"], &["main"]),
            commit("e", &["d"], &[]),
            commit("d", &["b"], &[]),
            commit("c", &["b"], &["side"]),
            commit("b", &["a"], &[]),
            commit("a", &[], &[]),
        ];
        let graph = assign_lanes(&commits);
        for (index, row) in graph.rows.iter().enumerate() {
            for &parent in &row.parents {
                assert!(parent < graph.rows.len(), "row {index} points off the end");
                assert!(parent > index, "row {index} points back at {parent}");
            }
        }
    }

    #[test]
    fn a_lane_is_named_after_the_branch_that_opened_it() {
        let commits = vec![
            commit("c", &["b"], &["main"]),
            commit("b", &["a"], &[]),
            commit("a", &[], &[]),
        ];
        let graph = assign_lanes(&commits);
        assert_eq!(graph.rows[0].label.as_deref(), Some("main"));
        assert_eq!(graph.rows[1].label.as_deref(), Some("main"));
    }

    /// A tag is not a branch and must not name a rail.
    #[test]
    fn a_tag_does_not_name_a_lane() {
        let commits = vec![commit("a", &[], &["v1.2.0"])];
        assert_eq!(assign_lanes(&commits).rows[0].label, None);
    }

    #[test]
    fn a_local_branch_is_preferred_over_a_remote_one() {
        let commits = vec![commit("a", &[], &["origin/main", "main"])];
        assert_eq!(
            assign_lanes(&commits).rows[0].label.as_deref(),
            Some("main")
        );
    }

    #[test]
    fn an_empty_history_lays_out_without_panicking() {
        let graph = assign_lanes(&[]);
        assert!(graph.rows.is_empty());
        assert_eq!(graph.lanes, 1);
    }

    /// Many simultaneously live branches must retain distinct rails.
    #[test]
    fn busy_history_keeps_all_rails_distinct() {
        let mut commits = Vec::new();
        for index in 0..(MAX_LANES * 3) {
            commits.push(commit(
                &format!("tip{index}"),
                &[&format!("base{index}")],
                &[],
            ));
        }
        let graph = assign_lanes(&commits);
        assert_eq!(graph.lanes, MAX_LANES * 3);
        let lanes: std::collections::HashSet<_> = graph.rows.iter().map(|row| row.lane).collect();
        assert_eq!(lanes.len(), commits.len());
    }

    #[test]
    fn the_gutter_grows_with_the_lanes_it_must_hold() {
        assert!(gutter_width(1) < gutter_width(5));
        assert!(gutter_width(MAX_LANES) < gutter_width(MAX_LANES + 5));
        let widest = lane_x(MAX_LANES - 1) + 6.;
        assert!(widest <= gutter_width(MAX_LANES));
    }

    // ---- Edge routing --------------------------------------------------

    #[test]
    fn every_route_ends_on_its_parent() {
        for (child, parent) in [((0, 0), (3, 1)), ((2, 3), (5, 0)), ((1, 2), (2, 2))] {
            let route = edge_route(child, parent);
            assert!(close(route.start.0, lane_x(child.1)));
            let (x, y) = route.segments().last().expect("segments").end();
            assert!(close(x, lane_x(parent.1)) && close(y, row_y(parent.0)));
        }
    }

    #[test]
    fn same_lane_parents_use_a_single_straight_rail() {
        assert_eq!(edge_route((0, 1), (4, 1)).segments().len(), 1);
    }

    /// A lane change is a rail, a small corner, a horizontal run, a small
    /// corner, and a rail -- never one long sweeping curve.
    #[test]
    fn lane_changes_use_a_horizontal_connector_with_small_corners() {
        for (child, parent) in [((0, 0), (3, 2)), ((1, 4), (2, 0)), ((0, 1), (6, 3))] {
            let route = edge_route(child, parent);
            let segments = route.segments();
            assert_eq!(segments.len(), MAX_SEGMENTS);
            assert!(close(segments[0].end().0, route.start.0));

            let connector_y = segments[2].end().1;
            assert!(close(segments[1].end().1, connector_y), "not horizontal");

            for corner in [segments[1], segments[3]] {
                let Segment::Curve { x, y, cx, cy } = corner else {
                    panic!("corner is not a curve");
                };
                assert!((x - cx).abs() <= CORNER + 0.01 && (y - cy).abs() <= CORNER + 0.01);
            }

            let run = (segments[2].end().0 - segments[1].end().0).abs();
            let span = (lane_x(parent.1) - lane_x(child.1)).abs();
            assert!(run >= span - 2. * CORNER - 0.01);
        }
    }

    /// Branch points turn just below the child; merges turn just above the
    /// parent. Either way the side branch keeps the long vertical rail.
    #[test]
    fn the_side_branch_carries_the_long_rail() {
        let branch_out = edge_route((0, 0), (6, 2));
        assert!((branch_out.segments()[2].end().1 - row_y(0)).abs() <= JOG + 0.01);

        let merge_back = edge_route((0, 2), (6, 0));
        assert!((merge_back.segments()[2].end().1 - row_y(6)).abs() <= JOG + 0.01);
    }

    #[test]
    fn routes_stay_inside_the_graph_gutter() {
        for lane in 0..MAX_LANES {
            for other in 0..MAX_LANES {
                let route = edge_route((0, lane), (3, other));
                for segment in route.segments() {
                    let (x, _) = segment.end();
                    assert!((0. ..=gutter_width(MAX_LANES)).contains(&x), "x={x}");
                }
            }
        }
    }
}
