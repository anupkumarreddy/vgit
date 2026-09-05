use crate::{demo::COMMITS, theme::Palette};
use gpui::{Bounds, PathBuilder, canvas, point, prelude::*, px, quad, rgb, size};

pub const ROW_HEIGHT: f32 = 28.;

/// Width of the graph gutter. Sized to hold [`LANE_CAPACITY`] rails plus the
/// radius of a node dot, so branches never paint into the commit columns.
pub const GRAPH_WIDTH: f32 = LANE_ORIGIN + (LANE_CAPACITY - 1) as f32 * LANE_STEP + 16.;

/// How many branches the gutter can show at once. A repository may hold more,
/// in which case the sidebar offers a selection of which to draw.
pub const LANE_CAPACITY: usize = 5;

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

/// One visible row of history: the commit it shows, the lane its branch was
/// given, and the rows its parents resolved to.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub commit: usize,
    pub lane: usize,
    pub parents: Vec<usize>,
}

/// Builds the visible rows for a branch selection.
///
/// A commit is shown when its branch is selected, and takes the lane of that
/// branch's position in `selected`. An edge whose parent sits on a hidden
/// branch is redirected to the nearest visible ancestor, so hiding a branch
/// never breaks the history into disconnected pieces.
pub fn rows(selected: &[&str]) -> Vec<Row> {
    let lane: Vec<Option<usize>> = COMMITS
        .iter()
        .map(|commit| selected.iter().position(|name| *name == commit.branch))
        .collect();

    let mut row_of = vec![usize::MAX; COMMITS.len()];
    let mut order = Vec::new();
    for (commit, lane) in lane.iter().enumerate() {
        if lane.is_some() {
            row_of[commit] = order.len();
            order.push(commit);
        }
    }

    order
        .iter()
        .map(|&commit| {
            let mut visible_parents = Vec::new();
            let mut seen = Vec::new();
            for &parent in COMMITS[commit].parents {
                nearest_visible(parent, &lane, &mut visible_parents, &mut seen);
            }
            Row {
                commit,
                lane: lane[commit].expect("commit is visible"),
                parents: visible_parents
                    .into_iter()
                    .map(|commit| row_of[commit])
                    .collect(),
            }
        })
        .collect()
}

/// Walks up from `commit` until it reaches visible ancestors, collecting each
/// one exactly once.
fn nearest_visible(
    commit: usize,
    lane: &[Option<usize>],
    found: &mut Vec<usize>,
    seen: &mut Vec<usize>,
) {
    if lane[commit].is_some() {
        if !found.contains(&commit) {
            found.push(commit);
        }
        return;
    }
    if seen.contains(&commit) {
        return;
    }
    seen.push(commit);
    for &parent in COMMITS[commit].parents {
        nearest_visible(parent, lane, found, seen);
    }
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

fn commit_color(index: usize, lane: usize, colors: Palette) -> u32 {
    let commit = &COMMITS[index];
    if commit.parents.len() > 1 {
        colors.merge
    } else if commit.reference.contains("origin/") {
        colors.remote
    } else if commit.reference.starts_with('v') {
        colors.tag
    } else {
        colors.branch(lane)
    }
}

/// Paints a compact, native commit graph for the history sidebar.
///
/// Every visible branch has a straight vertical rail, joined across lanes by
/// [`edge_route`]. Regular commits use a filled inner dot; merge commits use a
/// hollow inner dot so topology is readable without relying on color alone.
pub fn sidebar_graph(rows: Vec<Row>, colors: Palette) -> impl IntoElement {
    let height = rows.len() as f32 * ROW_HEIGHT;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));

            for (index, row) in rows.iter().enumerate() {
                for &parent in &row.parents {
                    let route = edge_route((index, row.lane), (parent, rows[parent].lane));
                    let mut path = PathBuilder::stroke(px(2.));
                    path.move_to(at(route.start.0, route.start.1));
                    for segment in route.segments() {
                        match *segment {
                            Segment::Line { x, y } => path.line_to(at(x, y)),
                            Segment::Curve { x, y, cx, cy } => path.curve_to(at(x, y), at(cx, cy)),
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(colors.branch(rows[parent].lane)));
                    }
                }
            }

            for (index, row) in rows.iter().enumerate() {
                let center = at(lane_x(row.lane), row_y(index));
                let color = commit_color(row.commit, row.lane, colors);

                window.paint_quad(quad(
                    Bounds::new(center - point(px(6.), px(6.)), size(px(12.), px(12.))),
                    px(6.),
                    rgb(colors.sidebar),
                    px(2.),
                    rgb(color),
                    Default::default(),
                ));

                let merge = COMMITS[row.commit].parents.len() > 1;
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
    .w(px(GRAPH_WIDTH))
    .h(px(height))
    .absolute()
    .top_0()
    .left_0()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::BRANCHES;

    fn default_selection() -> Vec<&'static str> {
        BRANCHES.iter().take(LANE_CAPACITY).copied().collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn the_gutter_holds_its_advertised_lane_capacity() {
        let last = lane_x(LANE_CAPACITY - 1);
        assert!(last + 6. <= GRAPH_WIDTH, "lane {LANE_CAPACITY} overflows");
    }

    #[test]
    fn the_fixture_has_more_branches_than_the_gutter_can_show() {
        assert!(
            BRANCHES.len() > LANE_CAPACITY,
            "branch selection would be pointless with {} branches",
            BRANCHES.len()
        );
    }

    /// Every commit belongs to a branch the branch list knows about.
    #[test]
    fn every_commit_sits_on_a_known_branch() {
        for commit in COMMITS {
            assert!(
                BRANCHES.contains(&commit.branch),
                "unknown branch {}",
                commit.branch
            );
        }
    }

    /// Parents must always be older, or the graph would route backwards.
    #[test]
    fn parents_are_always_older_than_their_children() {
        for (index, commit) in COMMITS.iter().enumerate() {
            for &parent in commit.parents {
                assert!(parent > index, "commit {index} has a parent at {parent}");
            }
        }
    }

    #[test]
    fn a_selection_only_shows_its_own_branches() {
        let selected = vec!["main", "feature/graph"];
        for row in rows(&selected) {
            assert!(selected.contains(&COMMITS[row.commit].branch));
        }
    }

    #[test]
    fn lanes_follow_the_order_of_the_selection() {
        let selected = vec!["feature/themes", "main"];
        for row in rows(&selected) {
            let expected = if COMMITS[row.commit].branch == "feature/themes" {
                0
            } else {
                1
            };
            assert_eq!(row.lane, expected);
        }
    }

    #[test]
    fn every_selection_stays_inside_the_lane_capacity() {
        let selected = default_selection();
        assert!(rows(&selected).iter().all(|row| row.lane < LANE_CAPACITY));
    }

    /// Hiding a branch must not orphan the commits below it: an edge into a
    /// hidden parent is redirected to the nearest visible ancestor.
    #[test]
    fn hiding_a_branch_keeps_the_history_connected() {
        let selected = vec!["main"];
        let rows = rows(&selected);
        assert!(rows.len() > 1);
        for (index, row) in rows.iter().enumerate() {
            for &parent in &row.parents {
                assert!(parent > index, "row {index} points back at {parent}");
                assert!(parent < rows.len(), "row {index} points outside the list");
            }
        }
        // Only the root may be without a parent.
        let childless = rows.iter().filter(|row| row.parents.is_empty()).count();
        assert_eq!(childless, 1, "history split into disconnected pieces");
    }

    #[test]
    fn a_merge_keeps_both_parents_when_both_are_visible() {
        let selected = default_selection();
        let rows = rows(&selected);
        let merge = rows
            .iter()
            .find(|row| {
                COMMITS[row.commit].parents.len() > 1 && COMMITS[row.commit].branch == "main"
            })
            .expect("the fixture has a merge on main");
        assert_eq!(merge.parents.len(), 2);
    }

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
        for lane in 0..LANE_CAPACITY {
            for other in 0..LANE_CAPACITY {
                let route = edge_route((0, lane), (3, other));
                for segment in route.segments() {
                    let (x, _) = segment.end();
                    assert!((0. ..=GRAPH_WIDTH).contains(&x), "x={x}");
                }
            }
        }
    }
}
