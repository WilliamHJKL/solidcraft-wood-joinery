//! A half lap: two boards that cross, each notched so they sit in one plane.

use crate::boards::{cut_block, long_axis, measurable, FIT};
use crate::joint::{Aabb, Block};

/// What the user may set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Depth of the notch in the first board. Default: half the thickness where they cross.
    pub depth: Option<f64>,
    /// The first board is notched on its high side. `true` notches its low side instead.
    pub flip: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub cut_first: Block,
    pub cut_second: Block,
    /// Thickness of wood where the boards cross.
    pub thickness: f64,
    pub depth_first: f64,
    pub depth_second: f64,
}

/// The axis the notches go down. Boards that cross (their lengths along two axes) are notched
/// along the third: through the thickness when they lie flat, through the height when they stand
/// on edge, as in an egg-crate divider. Boards along one axis (an end lap, or edge to edge) are
/// notched through the thickness of the first.
pub fn lap_axis(first: &Aabb, second: &Aabb) -> usize {
    let thinnest = |b: &Aabb, but: Option<usize>| (0..3).filter(|a| Some(*a) != but).min_by(|&x, &y| b.size(x).total_cmp(&b.size(y))).unwrap_or(2);
    match (long_axis(first), long_axis(second)) {
        (Some(x), Some(y)) if x != y => 3 - x - y,
        (la, _) => thinnest(first, la),
    }
}

/// How far to move `first` along the lap axis so that it lies level with `second` there: on the
/// nearer of the two faces when the boards are not equally thick.
pub fn align(first: &Aabb, second: &Aabb) -> Result<[f64; 3], String> {
    measurable(first)?;
    measurable(second)?;
    let n = lap_axis(first, second);
    let apart: Vec<usize> = (0..3).filter(|&a| a != n && first.min[a].max(second.min[a]) >= first.max[a].min(second.max[a]) - FIT).collect();
    // Boards along one line that only meet are slid over each other afterwards (`slide`).
    let parallel = long_axis(first).is_some() && long_axis(first) == long_axis(second);
    if !apart.is_empty() && !(parallel && apart.len() == 1) {
        return Err("the two boards do not cross; move one over the other, where the joint goes".into());
    }
    let (low, high) = (second.min[n] - first.min[n], second.max[n] - first.max[n]);
    let mut shift = [0.0; 3];
    shift[n] = if low.abs() <= high.abs() { low } else { high };
    Ok(shift)
}

/// Boards along one line that meet edge to edge or end to end, or lie a little apart, are slid
/// over each other before they are notched: this is how far to move `first`, so that the two
/// overlap by `overlap` (empty: half the thickness edge to edge, the width of the first board end
/// to end). `None` when the boards already overlap, or cross, or are not side by side.
pub fn slide(first: &Aabb, second: &Aabb, overlap: Option<f64>) -> Result<Option<[f64; 3]>, String> {
    measurable(first)?;
    measurable(second)?;
    let length = long_axis(first);
    if length.is_none() || length != long_axis(second) {
        return Ok(None);
    }
    let n = lap_axis(first, second);
    let apart: Vec<usize> = (0..3).filter(|&a| first.min[a].max(second.min[a]) >= first.max[a].min(second.max[a]) - FIT).collect();
    let [j] = apart.as_slice() else { return Ok(None) };
    let j = *j;
    if j == n {
        return Ok(None); // one on the other: Align brings them level
    }
    let by = match overlap {
        Some(v) => v,
        None if Some(j) == length => first.size(3 - n - j),
        None => 0.5 * first.size(n),
    };
    let most = first.size(j).min(second.size(j));
    if !(by.is_finite() && by > FIT && by < most - FIT) {
        return Err(format!("overlap must be more than zero and less than the {most:.3} mm of the boards along the joint"));
    }
    let mut shift = [0.0; 3];
    shift[j] = if first.max[j] <= second.min[j] + FIT { second.min[j] + by - first.max[j] } else { second.max[j] - by - first.min[j] };
    Ok(Some(shift))
}

/// Plan the two notches where `first` and `second` overlap.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let n = lap_axis(&first, &second);
    let mut lo = [0.0; 3];
    let mut hi = [0.0; 3];
    for a in 0..3 {
        lo[a] = first.min[a].max(second.min[a]);
        hi[a] = first.max[a].min(second.max[a]);
    }
    let apart: Vec<usize> = (0..3).filter(|&a| hi[a] - lo[a] <= FIT).collect();
    match apart.as_slice() {
        [] => {}
        [a] if *a == n => {
            return Err("the two boards lie one on the other: tick \"Align the boards first\", or move one into the other, where the joint goes".into())
        }
        [a] if Some(*a) == long_axis(&first) && long_axis(&first) == long_axis(&second) => {
            return Err("the two boards meet end to end without overlapping: a half lap needs them to overlap by the length of the lap, so move one along the other by that length (often the width of the boards)".into())
        }
        _ => return Err("the two boards do not overlap; move one into the other, where the joint goes".into()),
    }
    let thickness = hi[n] - lo[n];
    let depth = o.depth.unwrap_or(0.5 * thickness);
    if !(depth.is_finite() && depth > FIT && depth < thickness - FIT) {
        return Err(format!("depth must be more than zero and less than the {thickness:.3} mm the boards share"));
    }
    // The first board loses `depth` on one side, the second loses the rest on the other side.
    let split = if o.flip { lo[n] + depth } else { hi[n] - depth };
    let (mut first_lo, mut first_hi, mut second_lo, mut second_hi) = (lo, hi, lo, hi);
    if o.flip {
        first_hi[n] = split;
        second_lo[n] = split;
    } else {
        first_lo[n] = split;
        second_hi[n] = split;
    }
    Ok(Plan {
        cut_first: cut_block(&first, first_lo, first_hi),
        cut_second: cut_block(&second, second_lo, second_hi),
        thickness,
        depth_first: depth,
        depth_second: thickness - depth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::bx;

    /// Two 45 x 18 boards crossing flat, one along X and one along Y.
    fn cross() -> (Aabb, Aabb) {
        (bx([0.0, 0.0, 0.0], [300.0, 45.0, 18.0]), bx([100.0, -100.0, 0.0], [45.0, 300.0, 18.0]))
    }

    #[test]
    fn a_cross_lap_takes_half_of_each_board() {
        let (a, b) = cross();
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!((p.thickness, p.depth_first, p.depth_second), (18.0, 9.0, 9.0));
        // The first board: the top half, open on both edges and on top.
        assert_eq!(p.cut_first, Block { corner: [100.0, -0.5, 9.0], size: [45.0, 46.0, 9.5] });
        // The second board: the bottom half, open on both edges and underneath.
        assert_eq!(p.cut_second, Block { corner: [99.5, 0.0, -0.5], size: [46.0, 45.0, 9.5] });
    }

    #[test]
    fn depth_and_flip() {
        let (a, b) = cross();
        let p = plan(a, b, &Options { depth: Some(6.0), flip: true }).unwrap();
        assert_eq!((p.depth_first, p.depth_second), (6.0, 12.0));
        assert_eq!(p.cut_first, Block { corner: [100.0, -0.5, -0.5], size: [45.0, 46.0, 6.5] });
        assert_eq!(p.cut_second, Block { corner: [99.5, 0.0, 6.0], size: [46.0, 45.0, 12.5] });
    }

    #[test]
    fn a_corner_lap_is_open_at_the_ends() {
        let a = bx([0.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        let b = bx([0.0, 0.0, 0.0], [45.0, 300.0, 18.0]);
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!(p.cut_first, Block { corner: [-0.5, -0.5, 9.0], size: [45.5, 46.0, 9.5] });
        assert_eq!(p.cut_second, Block { corner: [-0.5, -0.5, -0.5], size: [46.0, 45.5, 9.5] });
    }

    #[test]
    fn align_brings_the_first_board_into_the_plane_of_the_second() {
        let (a, b) = cross();
        assert_eq!(align(&a, &b).unwrap(), [0.0; 3]);
        // Lying 30 mm above the second board.
        let above = bx([0.0, 0.0, 30.0], [300.0, 45.0, 18.0]);
        assert_eq!(align(&above, &b).unwrap(), [0.0, 0.0, -30.0]);
        // Beside it: they do not cross, so there is nowhere to put the joint.
        let beside = bx([0.0, 300.0, 30.0], [300.0, 45.0, 18.0]);
        assert!(align(&beside, &b).unwrap_err().contains("do not cross"));
    }

    #[test]
    fn boards_on_edge_are_notched_through_their_height() {
        // An egg-crate divider: two 300 x 18 x 100 boards standing on edge, crossing.
        let a = bx([0.0, 141.0, 0.0], [300.0, 18.0, 100.0]);
        let b = bx([141.0, 0.0, 0.0], [18.0, 300.0, 100.0]);
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!((p.thickness, p.depth_first, p.depth_second), (100.0, 50.0, 50.0));
        // The first board: a slot from the top, the full 18 mm of the second's thickness wide.
        assert_eq!(p.cut_first, Block { corner: [141.0, 140.5, 50.0], size: [18.0, 19.0, 50.5] });
        assert_eq!(p.cut_second, Block { corner: [140.5, 141.0, -0.5], size: [19.0, 18.0, 50.5] });
    }

    #[test]
    fn an_upright_crosses_a_rail_through_their_thickness() {
        // A post along Z and a rail along X, both 45 x 18 across Y: the third axis is Y.
        let post = bx([100.0, 0.0, 0.0], [45.0, 18.0, 500.0]);
        let rail = bx([0.0, 0.0, 200.0], [300.0, 18.0, 45.0]);
        assert_eq!(lap_axis(&post, &rail), 1);
        let p = plan(post, rail, &Options::default()).unwrap();
        assert_eq!(p.thickness, 18.0);
    }

    #[test]
    fn boards_along_one_axis_are_notched_through_the_thickness() {
        // An end lap: two flat boards overlapping by 45 at their ends.
        let a = bx([0.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        let b = bx([255.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        assert_eq!(lap_axis(&a, &b), 2);
        // Edge to edge, overlapping by 10 across their width: a shiplap, still through the thickness.
        let c = bx([0.0, 35.0, 0.0], [300.0, 45.0, 18.0]);
        let p = plan(a, c, &Options::default()).unwrap();
        assert_eq!(p.thickness, 18.0);
    }

    #[test]
    fn boards_edge_to_edge_slide_over_each_other() {
        // Two 220 x 22 planks lying flat, edge to edge: the first slides 11 mm over the second.
        let a = bx([0.0, 0.0, 0.0], [220.0, 1360.0, 22.0]);
        let b = bx([220.0, 0.0, 0.0], [220.0, 1360.0, 22.0]);
        assert_eq!(slide(&a, &b, None).unwrap(), Some([11.0, 0.0, 0.0]));
        assert_eq!(slide(&a, &b, Some(10.0)).unwrap(), Some([10.0, 0.0, 0.0]));
        // 10 mm apart, from the other side: it slides 20 the other way.
        let c = bx([-230.0, 0.0, 0.0], [220.0, 1360.0, 22.0]);
        assert_eq!(slide(&c, &a, Some(10.0)).unwrap(), Some([20.0, 0.0, 0.0]));
        // Slid, they make a 10 x 11 lap along the whole length.
        let mut moved = a;
        moved.min[0] += 10.0;
        moved.max[0] += 10.0;
        let p = plan(moved, b, &Options::default()).unwrap();
        assert_eq!((p.thickness, p.depth_first), (22.0, 11.0));
        assert!(slide(&a, &b, Some(220.0)).unwrap_err().contains("less than the 220"));
    }

    #[test]
    fn boards_end_to_end_slide_by_their_width() {
        let a = bx([0.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        let b = bx([300.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        assert_eq!(slide(&b, &a, None).unwrap(), Some([-45.0, 0.0, 0.0]));
        // Boards that already overlap, or cross, or lie on each other are not slid.
        let over = bx([255.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        assert_eq!(slide(&a, &over, None).unwrap(), None);
        let cross = bx([100.0, -100.0, 0.0], [45.0, 300.0, 18.0]);
        assert_eq!(slide(&a, &cross, None).unwrap(), None);
        let on_top = bx([0.0, 0.0, 18.0], [300.0, 45.0, 18.0]);
        assert_eq!(slide(&a, &on_top, None).unwrap(), None);
    }

    #[test]
    fn boards_that_only_touch_are_told_what_to_do() {
        let a = bx([0.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        let on_top = bx([100.0, -100.0, 18.0], [45.0, 300.0, 18.0]);
        assert!(plan(a, on_top, &Options::default()).unwrap_err().contains("Align the boards first"));
        let end_to_end = bx([300.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        assert!(plan(a, end_to_end, &Options::default()).unwrap_err().contains("overlap by the length of the lap"));
        // Stacked boards on edge are aligned along their height.
        let low = bx([0.0, 141.0, 0.0], [300.0, 18.0, 100.0]);
        let high = bx([141.0, 0.0, 100.0], [18.0, 300.0, 100.0]);
        assert_eq!(align(&low, &high).unwrap(), [0.0, 0.0, 100.0]);
    }

    #[test]
    fn bad_laps_are_refused_with_a_reason() {
        let (a, b) = cross();
        let apart = bx([100.0, 45.0, 0.0], [45.0, 300.0, 18.0]);
        assert!(plan(a, apart, &Options::default()).unwrap_err().contains("do not overlap"));
        assert!(plan(a, b, &Options { depth: Some(18.0), flip: false }).unwrap_err().contains("less than"));
    }
}
