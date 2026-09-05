use crate::{demo::COMMITS, theme::Palette};
use gpui::{Bounds, PathBuilder, canvas, point, prelude::*, px, quad, rgb, size};

pub const ROW_HEIGHT: f32 = 28.;

/// Width of the graph gutter. Sized to hold [`LANE_CAPACITY`] rails plus the
/// radius of a node dot, so branches never paint into the commit columns.
pub const GRAPH_WIDTH: f32 = LANE_ORIGIN + (LANE_CAPACITY - 1) as f32 * LANE_STEP + 16.;

/// How many concurrent branches the gutter reserves room for.
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

fn lane_x(index: usize) -> f32 {
    LANE_ORIGIN + COMMITS[index].lane as f32 * LANE_STEP
}

fn row_y(index: usize) -> f32 {
    index as f32 * ROW_HEIGHT + ROW_HEIGHT / 2.
}

/// Routes the connector from `child` to one of its parents.
///
/// Commits sharing a lane are joined by a straight rail. Across lanes the two
/// rails are joined by a horizontal connector with one small rounded corner at
/// each end, rather than a single long sweeping curve. The connector sits
/// beside the endpoint on the lower lane, so the side branch keeps the long
/// vertical run: just below a branch point, just above a merge target.
pub fn edge_route(child: usize, parent: usize) -> EdgeRoute {
    let (sx, sy) = (lane_x(child), row_y(child));
    let (ex, ey) = (lane_x(parent), row_y(parent));
    let mut segments = [Segment::Line { x: ex, y: ey }; MAX_SEGMENTS];
    let mut len = 1;

    if sx != ex {
        let (dx, dy) = (ex - sx, ey - sy);
        let radius = CORNER.min(dx.abs() / 2.).min(dy.abs() / 2.);
        let jog = JOG.clamp(radius, (dy.abs() - radius).max(radius));
        let (hdir, vdir) = (dx.signum(), dy.signum());
        let join = if COMMITS[child].lane < COMMITS[parent].lane {
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

fn commit_color(index: usize, colors: Palette) -> u32 {
    let commit = &COMMITS[index];
    if commit.parents.len() > 1 {
        colors.merge
    } else if commit.reference.contains("origin/") {
        colors.remote
    } else if commit.reference.starts_with('v') {
        colors.tag
    } else {
        colors.branch(commit.lane)
    }
}

/// Paints a compact, native commit graph for the history sidebar.
///
/// Every active branch has a straight vertical rail, joined across lanes by
/// [`edge_route`]. Regular commits use a filled inner dot; merge commits use a
/// hollow inner dot so topology is readable without relying on color alone.
pub fn sidebar_graph(colors: Palette) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));

            for (index, commit) in COMMITS.iter().enumerate() {
                for &parent in commit.parents {
                    let route = edge_route(index, parent);
                    let mut path = PathBuilder::stroke(px(2.));
                    path.move_to(at(route.start.0, route.start.1));
                    for segment in route.segments() {
                        match *segment {
                            Segment::Line { x, y } => path.line_to(at(x, y)),
                            Segment::Curve { x, y, cx, cy } => path.curve_to(at(x, y), at(cx, cy)),
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(colors.branch(COMMITS[parent].lane)));
                    }
                }
            }

            for (index, commit) in COMMITS.iter().enumerate() {
                let center = at(lane_x(index), row_y(index));
                let color = commit_color(index, colors);

                window.paint_quad(quad(
                    Bounds::new(center - point(px(6.), px(6.)), size(px(12.), px(12.))),
                    px(6.),
                    rgb(colors.sidebar),
                    px(2.),
                    rgb(color),
                    Default::default(),
                ));

                let merge = commit.parents.len() > 1;
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
    .h(px(COMMITS.len() as f32 * ROW_HEIGHT))
    .absolute()
    .top_0()
    .left_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every (child, parent) pair in the fixture topology.
    fn edges() -> impl Iterator<Item = (usize, usize)> {
        COMMITS
            .iter()
            .enumerate()
            .flat_map(|(child, commit)| commit.parents.iter().map(move |&p| (child, p)))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn every_route_ends_on_its_parent() {
        for (child, parent) in edges() {
            let route = edge_route(child, parent);
            assert!(close(route.start.0, lane_x(child)) && close(route.start.1, row_y(child)));
            let (x, y) = route.segments().last().expect("route has segments").end();
            assert!(
                close(x, lane_x(parent)) && close(y, row_y(parent)),
                "{child}->{parent} ended at ({x}, {y})"
            );
        }
    }

    #[test]
    fn same_lane_parents_use_a_single_straight_rail() {
        for (child, parent) in edges() {
            if COMMITS[child].lane == COMMITS[parent].lane {
                let route = edge_route(child, parent);
                assert_eq!(
                    route.segments().len(),
                    1,
                    "{child}->{parent} is not straight"
                );
            }
        }
    }

    /// A lane change is a rail, a small corner, a horizontal run, a small
    /// corner, and a rail -- never one long sweeping curve.
    #[test]
    fn lane_changes_use_a_horizontal_connector_with_small_corners() {
        for (child, parent) in edges() {
            if COMMITS[child].lane == COMMITS[parent].lane {
                continue;
            }
            let route = edge_route(child, parent);
            let segments = route.segments();
            assert_eq!(segments.len(), MAX_SEGMENTS, "{child}->{parent}");

            // The first rail leaves the child straight down its own lane.
            assert!(
                close(segments[0].end().0, route.start.0),
                "{child}->{parent}"
            );

            // The connector between the two corners is exactly horizontal.
            let connector_y = segments[2].end().1;
            assert!(
                close(segments[1].end().1, connector_y),
                "{child}->{parent} connector is not horizontal"
            );

            // Curvature is confined to the two corners, each within CORNER.
            for corner in [segments[1], segments[3]] {
                let Segment::Curve { x, y, cx, cy } = corner else {
                    panic!("{child}->{parent} corner is not a curve");
                };
                assert!(
                    (x - cx).abs() <= CORNER + 0.01 && (y - cy).abs() <= CORNER + 0.01,
                    "{child}->{parent} corner is too wide"
                );
            }

            // The straight horizontal run is the bulk of the lane change.
            let run = (segments[2].end().0 - segments[1].end().0).abs();
            let span = (lane_x(parent) - lane_x(child)).abs();
            assert!(run >= span - 2. * CORNER - 0.01, "{child}->{parent}");
        }
    }

    /// Branch points turn just below the child; merges turn just above the
    /// parent. Either way the side branch keeps the long vertical rail.
    #[test]
    fn the_side_branch_carries_the_long_rail() {
        for (child, parent) in edges() {
            if COMMITS[child].lane == COMMITS[parent].lane {
                continue;
            }
            let route = edge_route(child, parent);
            let connector_y = route.segments()[2].end().1;
            let (from_child, from_parent) = (
                (connector_y - row_y(child)).abs(),
                (connector_y - row_y(parent)).abs(),
            );
            if COMMITS[child].lane < COMMITS[parent].lane {
                assert!(
                    from_child <= JOG + 0.01,
                    "{child}->{parent} turned too late"
                );
            } else {
                assert!(
                    from_parent <= JOG + 0.01,
                    "{child}->{parent} turned too early"
                );
            }
        }
    }

    /// The gutter must fit every lane it advertises, dot radius included.
    #[test]
    fn the_gutter_holds_its_advertised_lane_capacity() {
        let last = LANE_ORIGIN + (LANE_CAPACITY - 1) as f32 * LANE_STEP;
        assert!(last + 6. <= GRAPH_WIDTH, "lane {LANE_CAPACITY} overflows");
        assert!(
            COMMITS.iter().all(|commit| commit.lane < LANE_CAPACITY),
            "a fixture commit sits outside the lane capacity"
        );
    }

    #[test]
    fn routes_stay_inside_the_graph_gutter() {
        for (child, parent) in edges() {
            let route = edge_route(child, parent);
            for segment in route.segments() {
                let (x, _) = segment.end();
                assert!((0. ..=GRAPH_WIDTH).contains(&x), "{child}->{parent} x={x}");
            }
        }
    }
}
