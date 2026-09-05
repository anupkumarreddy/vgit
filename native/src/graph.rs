use crate::{demo::COMMITS, theme::Palette};
use gpui::{Bounds, PathBuilder, canvas, point, prelude::*, px, quad, rgb, size};

pub const ROW_HEIGHT: f32 = 54.;
pub const GRAPH_WIDTH: f32 = 94.;

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
/// Every active branch has a straight vertical rail. A cubic transition is
/// used only where a parent changes lanes. Regular commits use a filled inner
/// dot; merge commits use a hollow inner dot so topology is readable without
/// relying on color alone.
pub fn sidebar_graph(colors: Palette) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let position = |index: usize| {
                bounds.origin
                    + point(
                        px(22. + COMMITS[index].lane as f32 * 23.),
                        px(index as f32 * ROW_HEIGHT + ROW_HEIGHT / 2.),
                    )
            };

            for (index, commit) in COMMITS.iter().enumerate() {
                for &parent in commit.parents {
                    let start = position(index);
                    let end = position(parent);
                    let line_color = colors.branch(COMMITS[parent].lane);
                    let mut path = PathBuilder::stroke(px(2.));
                    path.move_to(start);
                    if start.x == end.x {
                        path.line_to(end);
                    } else {
                        let turn = (end.y - start.y).abs().min(px(34.));
                        if start.y < end.y {
                            path.cubic_bezier_to(
                                end,
                                point(start.x, start.y + turn),
                                point(end.x, end.y - turn),
                            );
                        } else {
                            path.cubic_bezier_to(
                                end,
                                point(start.x, start.y - turn),
                                point(end.x, end.y + turn),
                            );
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(line_color));
                    }
                }
            }

            for (index, commit) in COMMITS.iter().enumerate() {
                let center = position(index);
                let color = commit_color(index, colors);

                window.paint_quad(quad(
                    Bounds::new(center - point(px(7.), px(7.)), size(px(14.), px(14.))),
                    px(7.),
                    rgb(colors.sidebar),
                    px(2.),
                    rgb(color),
                    Default::default(),
                ));

                let merge = commit.parents.len() > 1;
                window.paint_quad(quad(
                    Bounds::new(center - point(px(3.), px(3.)), size(px(6.), px(6.))),
                    px(3.),
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
