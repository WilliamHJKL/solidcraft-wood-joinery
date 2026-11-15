//! A half lap: two boards that cross, each notched so they sit in one plane.

use crate::boards::{cut_block, measurable, FIT};
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

/// How far to move `first` across its thickness so that it lies in the plane of `second`: on the
/// nearer of the two faces when the boards are not equally thick.
pub fn align(first: &Aabb, second: &Aabb) -> Result<[f64; 3], String> {
    measurable(first)?;
    measurable(second)?;
    let n = (0..3).min_by(|&a, &b| first.size(a).total_cmp(&first.size(b))).unwrap_or(2);
    for a in (0..3).filter(|a| *a != n) {
        if first.min[a].max(second.min[a]) >= first.max[a].min(second.max[a]) - FIT {
            return Err("the two boards do not cross; move one over the other, where the joint goes".into());
        }
    }
    let (low, high) = (second.min[n] - first.min[n], second.max[n] - first.max[n]);
    let mut shift = [0.0; 3];
    shift[n] = if low.abs() <= high.abs() { low } else { high };
    Ok(shift)
}

/// Plan the two notches where `first` and `second` overlap.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let mut lo = [0.0; 3];
    let mut hi = [0.0; 3];
    for a in 0..3 {
        lo[a] = first.min[a].max(second.min[a]);
        hi[a] = first.max[a].min(second.max[a]);
        if hi[a] - lo[a] <= FIT {
            return Err("the two boards do not overlap; move one into the other, where the joint goes".into());
        }
    }
    // The boards are thinnest across the joint.
    let n = (0..3).min_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b]))).unwrap_or(2);
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
    fn bad_laps_are_refused_with_a_reason() {
        let (a, b) = cross();
        let apart = bx([100.0, 45.0, 0.0], [45.0, 300.0, 18.0]);
        assert!(plan(a, apart, &Options::default()).unwrap_err().contains("do not overlap"));
        assert!(plan(a, b, &Options { depth: Some(18.0), flip: false }).unwrap_err().contains("less than"));
    }
}
