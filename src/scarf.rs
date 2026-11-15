//! A scarf joint: two boards end to end, overlapping, each cut along one long slope.

use crate::boards::{measurable, FIT};
use crate::joint::Aabb;
use crate::slant::{beyond, Tool};

/// What the user may set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// The slope runs from the high face of the lower board to the low face of the other one.
    /// `true` turns it the other way.
    pub flip: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub tool_first: Tool,
    pub tool_second: Tool,
    /// Length of the slope along the boards: what they overlap.
    pub length: f64,
    /// The slope as 1:N, the length over the thickness.
    pub slope: f64,
    /// Volume the cut takes from each board, in mm3.
    pub cut: f64,
}

fn axes(first: &Aabb, second: &Aabb) -> Result<(usize, usize, usize), String> {
    let pick = |x: &Aabb, most: bool| {
        let by = |i: &usize, j: &usize| x.size(*i).total_cmp(&x.size(*j));
        if most { (0..3).max_by(by) } else { (0..3).min_by(by) }.unwrap_or(0)
    };
    let along = pick(first, true);
    if pick(second, true) != along {
        return Err("the two boards do not run the same way; a scarf joins two boards end to end".into());
    }
    let thick = pick(first, false);
    Ok((along, thick, 3 - along - thick))
}

/// How far to move `first` so that the two boards overlap by eight thicknesses, end over end,
/// and lie level across their width and thickness.
pub fn align(first: &Aabb, second: &Aabb) -> Result<[f64; 3], String> {
    measurable(first)?;
    measurable(second)?;
    let (along, thick, width) = axes(first, second)?;
    let length = 8.0 * first.size(thick);
    if length >= first.size(along) - FIT || length >= second.size(along) - FIT {
        return Err("a board is shorter than eight thicknesses; overlap the boards by hand instead".into());
    }
    let mut shift = [0.0; 3];
    shift[along] = if first.min[along] + first.max[along] <= second.min[along] + second.max[along] {
        second.min[along] + length - first.max[along]
    } else {
        second.max[along] - length - first.min[along]
    };
    shift[thick] = second.min[thick] - first.min[thick];
    shift[width] = 0.5 * (second.min[width] + second.max[width] - first.min[width] - first.max[width]);
    Ok(shift)
}

/// Plan the two slopes where `first` and `second` overlap.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let (along, thick, width) = axes(&first, &second)?;
    let mut lo = [0.0; 3];
    let mut hi = [0.0; 3];
    for i in 0..3 {
        lo[i] = first.min[i].max(second.min[i]);
        hi[i] = first.max[i].min(second.max[i]);
        if hi[i] - lo[i] <= FIT {
            return Err("the two boards do not overlap; slide one over the end of the other, by the length of the slope".into());
        }
    }
    let length = hi[along] - lo[along];
    if length >= first.size(along) - FIT || length >= second.size(along) - FIT {
        return Err("one board lies wholly inside the other; a scarf joins two boards end to end".into());
    }
    let height = hi[thick] - lo[thick];
    // The slope starts on one face at the low end of the overlap and ends on the other face.
    let (mut from, mut to) = (lo, lo);
    to[along] = hi[along];
    if o.flip {
        to[thick] = hi[thick];
    } else {
        from[thick] = hi[thick];
    }
    let run = (length * length + height * height).sqrt();
    let w = [(to[0] - from[0]) / run, (to[1] - from[1]) / run, (to[2] - from[2]) / run];
    let centre = |x: &Aabb| [0.5 * (x.min[0] + x.max[0]), 0.5 * (x.min[1] + x.max[1]), 0.5 * (x.min[2] + x.max[2])];
    let reach = length + height + 1.0;
    let tool = |board: &Aabb| beyond(centre(board), from, w, width, thick, (board.min[width], board.max[width]), reach);
    Ok(Plan {
        tool_first: tool(&first),
        tool_second: tool(&second),
        length,
        slope: length / height,
        cut: 0.5 * length * height * (hi[width] - lo[width]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::bx;

    /// Two 400 x 45 x 18 boards end to end along X, overlapping by 144 mm (eight thicknesses).
    fn boards() -> (Aabb, Aabb) {
        (bx([0.0, 0.0, 0.0], [400.0, 45.0, 18.0]), bx([256.0, 0.0, 0.0], [400.0, 45.0, 18.0]))
    }

    #[test]
    fn the_slope_runs_over_the_overlap() {
        let (a, b) = boards();
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!((p.length, p.slope, p.cut), (144.0, 8.0, 0.5 * 144.0 * 18.0 * 45.0));
        // Both tools turn about the line across the boards at the start of the slope, on top.
        assert_eq!((p.tool_first.origin, p.tool_first.axis), ([256.0, 0.0, 18.0], [0.0, 1.0, 0.0]));
        // The two tools look opposite ways.
        assert!(((p.tool_first.angle - p.tool_second.angle).abs() - 180.0).abs() < 1e-9);
    }

    #[test]
    fn align_overlaps_the_boards_by_eight_thicknesses() {
        let (_, b) = boards();
        // The first board lies 50 mm short of the second, 10 mm aside and 5 mm too high.
        let loose = bx([-194.0, 10.0, 5.0], [400.0, 45.0, 18.0]);
        assert_eq!(align(&loose, &b).unwrap(), [194.0, -10.0, -5.0]);
        let across = bx([0.0, 0.0, 0.0], [45.0, 400.0, 18.0]);
        assert!(align(&across, &b).unwrap_err().contains("same way"));
    }

    #[test]
    fn bad_scarfs_are_refused_with_a_reason() {
        let (a, _) = boards();
        let apart = bx([420.0, 0.0, 0.0], [400.0, 45.0, 18.0]);
        assert!(plan(a, apart, &Options::default()).unwrap_err().contains("do not overlap"));
        let inside = bx([100.0, 0.0, 0.0], [100.0, 45.0, 18.0]);
        assert!(plan(a, inside, &Options::default()).is_err());
    }
}
