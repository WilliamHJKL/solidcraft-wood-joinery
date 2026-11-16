//! A miter: two boards that meet at a corner, each cut along the plane from the outer corner to
//! the inner one.
//!
//! The cut is at an angle: each board is cut by a tool box turned about the outer corner.

use crate::boards::{measurable, FIT, TOUCH};
use crate::joint::{Aabb, Block};
use crate::slant::{beyond, cross, dot, Tool};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    /// Wood added first to a board that stops at the other one, so that it reaches the corner.
    pub extend_first: Option<Block>,
    pub extend_second: Option<Block>,
    pub tool_first: Tool,
    pub tool_second: Tool,
    /// The angle of the cut across the first board, in degrees: 45 when both are equally thick.
    pub angle: f64,
    /// Volume the cut takes from each board, in mm3.
    pub cut_first: f64,
    pub cut_second: f64,
    /// A loose spline across the cut: its body, built along the axes and then turned like a
    /// tool, and the volume of the slot it leaves in each board.
    pub spline: Option<(Tool, f64)>,
}

fn thin(b: &Aabb) -> usize {
    (0..3).min_by(|&x, &y| b.size(x).total_cmp(&b.size(y))).unwrap_or(0)
}

/// Plan the miter between `first` and `second`.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    // In the plane of the corner, `a` is the side of the first board that the cut crosses, `b`
    // that of the second; the corner runs along `l`. Two boards standing on edge are read by
    // their thickness. Two boards lying flat, as in a picture frame, are equally thin: they are
    // read by their length, and the cut crosses their width.
    let long = |x: &Aabb| (0..3).max_by(|&i, &j| x.size(i).total_cmp(&x.size(j))).unwrap_or(0);
    let (a, b) = match (thin(&first), thin(&second), long(&first), long(&second)) {
        (ta, tb, _, _) if ta != tb => (ta, tb),
        (_, _, la, lb) if la != lb => (lb, la),
        _ => return Err("the two boards run the same way; turn one so they make a corner".into()),
    };
    let l = 3 - a - b;
    let meets = |x: usize| first.min[x].max(second.min[x]) <= first.max[x].min(second.max[x]) + TOUCH;
    if !(meets(a) && meets(b)) || first.min[l].max(second.min[l]) >= first.max[l].min(second.max[l]) - FIT {
        return Err("the two boards do not meet at a corner; move one against the end of the other".into());
    }
    let centre = |x: &Aabb, i: usize| 0.5 * (x.min[i] + x.max[i]);
    // The corner square: the thickness of each board. The inner corner is on the side of the
    // other board.
    let mut outer = [0.0; 3];
    let mut inner = [0.0; 3];
    (outer[a], inner[a]) = if centre(&second, a) > centre(&first, a) { (first.min[a], first.max[a]) } else { (first.max[a], first.min[a]) };
    (outer[b], inner[b]) = if centre(&first, b) > centre(&second, b) { (second.min[b], second.max[b]) } else { (second.max[b], second.min[b]) };
    let (ta, tb) = (first.size(a), second.size(b));
    let diagonal = (ta * ta + tb * tb).sqrt();
    let w = [(inner[0] - outer[0]) / diagonal, (inner[1] - outer[1]) / diagonal, (inner[2] - outer[2]) / diagonal];

    let reach = ta + tb + 1.0;
    // The tool lies on the side of the cut plane away from the board.
    let tool = |board: &Aabb| {
        let keep = [centre(board, 0), centre(board, 1), centre(board, 2)];
        beyond(keep, outer, w, l, a, (board.min[l], board.max[l]), reach)
    };
    // A board that stops at the other one is first made to reach the outer corner.
    let extend = |board: &Aabb, across: usize, along: usize| {
        let (lo, hi) = (outer[along].min(inner[along]), outer[along].max(inner[along]));
        if board.min[along] <= lo + FIT && board.max[along] >= hi - FIT {
            return None;
        }
        let lap = 0.1_f64.min(board.size(along) * 0.05);
        let mut corner = [0.0; 3];
        let mut size = [0.0; 3];
        corner[across] = board.min[across];
        size[across] = board.size(across);
        let (from, to) = if board.min[along] > lo + FIT { (lo, board.min[along] + lap) } else { (board.max[along] - lap, hi) };
        corner[along] = from;
        size[along] = to - from;
        corner[l] = board.min[l];
        size[l] = board.size(l);
        Some(Block { corner, size })
    };
    let thickness = o.spline.unwrap_or(0.0);
    if !(thickness.is_finite() && thickness >= 0.0) {
        return Err("spline must be zero or a positive thickness".into());
    }
    let spline = if thickness > 0.0 {
        let depth = o.spline_depth.unwrap_or(ta.min(tb) / 3.0);
        // Half way along the cut, the wood on each side is half the diagonal deep at most.
        let most = 0.5 * diagonal * (ta / tb).min(tb / ta) - 2.0;
        if !(depth.is_finite() && depth > FIT) || depth > most + FIT {
            return Err(format!("spline_depth must be more than zero and at most {:.3} mm for these boards", most.max(0.0)));
        }
        if thickness > 0.5 * diagonal {
            return Err(format!("a {thickness:.3} mm spline is too thick for a cut {diagonal:.3} mm long"));
        }
        let tool = tool(&first);
        // The spline lies square to the cut: `depth` each way across it, `thickness` along it,
        // around the middle of the cut. It is built along the axes, then turned as the tool is.
        let mut u = [0.0; 3];
        u[l] = 1.0;
        let mut e = [0.0; 3];
        e[a] = 1.0;
        let turn = tool.angle.to_radians();
        let across = cross(u, e);
        // Where `e` and `across` point after the turn about `u`.
        let d = [e[0] * turn.cos() + across[0] * turn.sin(), e[1] * turn.cos() + across[1] * turn.sin(), e[2] * turn.cos() + across[2] * turn.sin()];
        let along = if dot(cross(u, d), w) > 0.0 { 1.0 } else { -1.0 } * across[b];
        let (from, to) = (along * (0.5 * diagonal - 0.5 * thickness), along * (0.5 * diagonal + 0.5 * thickness));
        let (l_lo, l_hi) = (first.min[l].max(second.min[l]), first.max[l].min(second.max[l]));
        let mut corner = outer;
        let mut size = [0.0; 3];
        corner[a] = outer[a] - depth;
        size[a] = 2.0 * depth;
        corner[b] = outer[b] + from.min(to);
        size[b] = thickness;
        corner[l] = l_lo;
        size[l] = l_hi - l_lo;
        Some((Tool { block: Block { corner, size }, ..tool }, thickness * depth * (l_hi - l_lo)))
    } else {
        None
    };
    Ok(Plan {
        spline,
        extend_first: extend(&first, a, b),
        extend_second: extend(&second, b, a),
        tool_first: tool(&first),
        tool_second: tool(&second),
        angle: tb.atan2(ta).to_degrees(),
        cut_first: 0.5 * ta * tb * first.size(l),
        cut_second: 0.5 * ta * tb * second.size(l),
    })
}

/// What the user may set. The cut itself always runs from the outer corner to the inner one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Thickness of a loose spline across the cut. Default: 0, no spline.
    pub spline: Option<f64>,
    /// How far the spline goes into each board. Default: a third of the thinner board.
    pub spline_depth: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::bx;

    /// Two 18 x 100 boards drawn through each other at a corner: one along X, one along Y.
    fn corner() -> (Aabb, Aabb) {
        (bx([0.0, 0.0, 0.0], [300.0, 18.0, 100.0]), bx([0.0, 0.0, 0.0], [18.0, 300.0, 100.0]))
    }

    #[test]
    fn equal_boards_are_cut_at_45_degrees() {
        let (x, y) = corner();
        let p = plan(x, y, &Options::default()).unwrap();
        assert!((p.angle - 45.0).abs() < 1e-9);
        assert_eq!((p.extend_first, p.extend_second), (None, None));
        assert_eq!((p.cut_first, p.cut_second), (16200.0, 16200.0));
        // Both tools turn about the outer corner, the vertical line through the origin.
        assert_eq!((p.tool_first.origin, p.tool_first.axis), ([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
        // The board along X loses what lies beyond the plane x = y on the Y side.
        assert!((p.tool_first.angle - 45.0).abs() < 1e-9);
        assert!((p.tool_second.angle + 135.0).abs() < 1e-9);
        assert_eq!(p.tool_first.block, Block { corner: [-37.0, 0.0, -0.5], size: [74.0, 37.0, 101.0] });
    }

    #[test]
    fn a_board_that_stops_at_the_other_is_extended_first() {
        // The board along X stops at the inner face of the one along Y.
        let x = bx([18.0, 0.0, 0.0], [282.0, 18.0, 100.0]);
        let (_, y) = corner();
        let p = plan(x, y, &Options::default()).unwrap();
        let ext = p.extend_first.unwrap();
        assert!((ext.corner[0] - 0.0).abs() < 1e-9 && (ext.size[0] - 18.1).abs() < 1e-9);
        assert_eq!((ext.corner[1], ext.size[1], ext.size[2]), (0.0, 18.0, 100.0));
        assert_eq!(p.extend_second, None);
    }

    #[test]
    fn unequal_boards_and_other_corners() {
        // A 12 mm board along X on the far end of a 20 mm board along Y.
        let x = bx([-300.0, 282.0, 0.0], [300.0, 12.0, 100.0]);
        let y = bx([-20.0, -6.0, 0.0], [20.0, 300.0, 100.0]);
        let p = plan(x, y, &Options::default()).unwrap();
        assert!((p.angle - (20.0_f64).atan2(12.0).to_degrees()).abs() < 1e-9);
        // The outer corner is at x = 0, y = 294.
        assert_eq!(p.tool_first.origin, [0.0, 294.0, 0.0]);
        assert_eq!(p.cut_first, 0.5 * 12.0 * 20.0 * 100.0);
    }

    #[test]
    fn a_flat_frame_is_cut_across_its_width() {
        // Two 45 x 18 boards lying flat, as two sides of a picture frame.
        let x = bx([0.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        let y = bx([0.0, 0.0, 0.0], [45.0, 300.0, 18.0]);
        let p = plan(x, y, &Options::default()).unwrap();
        assert!((p.angle - 45.0).abs() < 1e-9);
        assert_eq!((p.tool_first.axis, p.cut_first), ([0.0, 0.0, 1.0], 0.5 * 45.0 * 45.0 * 18.0));
    }

    #[test]
    fn a_spline_lies_square_to_the_cut_half_way_along_it() {
        let (x, y) = corner();
        let p = plan(x, y, &Options { spline: Some(4.0), ..Options::default() }).unwrap();
        let (body, slot) = p.spline.unwrap();
        // 6 mm into each board, 4 thick, over the 100 mm the boards share.
        assert_eq!(slot, 4.0 * 6.0 * 100.0);
        assert_eq!((body.block.size[1], body.block.size[2]), (12.0, 100.0));
        assert!((body.block.size[0] - 4.0).abs() < 1e-9);
        // It turns with the first tool, about the outer corner.
        assert_eq!((body.origin, body.angle), (p.tool_first.origin, p.tool_first.angle));
        // The middle of the cut is 12.73 mm from the outer corner.
        let middle = body.block.corner[0].abs() + 2.0;
        assert!((middle - 0.5 * (2.0_f64 * 18.0 * 18.0).sqrt()).abs() < 1e-9);
        assert!(plan(x, y, &Options { spline: Some(4.0), spline_depth: Some(12.0) }).unwrap_err().contains("at most"));
    }

    #[test]
    fn bad_miters_are_refused_with_a_reason() {
        let (x, y) = corner();
        let beside = bx([0.0, 40.0, 0.0], [300.0, 18.0, 100.0]);
        assert!(plan(x, beside, &Options::default()).unwrap_err().contains("same way"));
        let apart = bx([60.0, 60.0, 0.0], [18.0, 300.0, 100.0]);
        assert!(plan(x, apart, &Options::default()).unwrap_err().contains("do not meet"));
        let _ = y;
    }
}
