//! A scarf joint: two boards end to end, overlapping, each cut along one long slope.

use crate::boards::{cut_block, measurable, FIT};
use crate::joint::{Aabb, Block};
use crate::slant::{beyond, Tool};

/// What the user may set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// The slope runs from the high face of the lower board to the low face of the other one.
    /// `true` turns it the other way.
    pub flip: bool,
    /// A tabled scarf: two hooked steps in place of the slope, held tight by a key between them.
    pub key: bool,
    /// Width of the key along the boards. Default: half their thickness.
    pub key_width: Option<f64>,
}

/// A tabled scarf: what is cut from each board, and the key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tabled {
    pub cuts_first: [Block; 2],
    pub cuts_second: [Block; 2],
    pub key: Block,
    /// Volume cut from each board, in mm3.
    pub cut: f64,
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
    /// Set for a tabled scarf: the boards are then cut by blocks, not by the two tools.
    pub tabled: Option<Tabled>,
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

/// How far to move `first` so that it lies end to end against `second`, level across their width
/// and thickness. `slide` then overlaps them by the slope the user asks for.
pub fn align(first: &Aabb, second: &Aabb) -> Result<[f64; 3], String> {
    measurable(first)?;
    measurable(second)?;
    let (along, thick, width) = axes(first, second)?;
    let mut shift = [0.0; 3];
    shift[along] = if first.min[along] + first.max[along] <= second.min[along] + second.max[along] {
        second.min[along] - first.max[along]
    } else {
        second.max[along] - first.min[along]
    };
    shift[thick] = second.min[thick] - first.min[thick];
    shift[width] = 0.5 * (second.min[width] + second.max[width] - first.min[width] - first.max[width]);
    Ok(shift)
}

/// Boards end to end that touch, or lie a little apart, are slid over each other before they are
/// cut: this is how far to move `first`, so that they overlap by `slope` thicknesses (empty: 8).
/// `None` when they already overlap, or are not level across their width and thickness (then
/// Align brings them together).
pub fn slide(first: &Aabb, second: &Aabb, slope: Option<f64>, unlimited: bool) -> Result<Option<[f64; 3]>, String> {
    measurable(first)?;
    measurable(second)?;
    let Ok((along, thick, _)) = axes(first, second) else { return Ok(None) };
    let apart: Vec<usize> = (0..3).filter(|&i| first.min[i].max(second.min[i]) >= first.max[i].min(second.max[i]) - FIT).collect();
    if apart != [along] {
        return Ok(None);
    }
    let gap = (second.min[along] - first.max[along]).max(first.min[along] - second.max[along]);
    if gap > crate::boards::FARTHEST + FIT && !unlimited {
        return Err(crate::lap::far(gap));
    }
    let slope = slope.unwrap_or(8.0);
    if !(slope.is_finite() && slope >= 1.0) {
        return Err("slope must be 1 or more: 8 means a slope of 1:8".into());
    }
    let length = slope * first.size(thick);
    if length >= first.size(along) - FIT || length >= second.size(along) - FIT {
        return Err(format!("a slope of 1:{slope} is {length:.3} mm long, as long as a board; give a smaller slope"));
    }
    let mut shift = [0.0; 3];
    shift[along] = if first.max[along] <= second.min[along] + FIT {
        second.min[along] + length - first.max[along]
    } else {
        second.max[along] - length - first.min[along]
    };
    Ok(Some(shift))
}

/// Plan the two slopes where `first` and `second` overlap.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let (along, thick, width) = axes(&first, &second)?;
    if (first.size(thick) - second.size(thick)).abs() > 0.1 {
        return Err(format!(
            "a scarf joins two boards of one thickness, its slope running from face to face: these are {:.3} and {:.3} mm",
            first.size(thick),
            second.size(thick)
        ));
    }
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
    let tabled = if o.key {
        if length < 3.0 * height - FIT {
            return Err(format!(
                "a tabled scarf needs an overlap of three thicknesses or more: {:.3} mm here, not {length:.3}",
                3.0 * height
            ));
        }
        let key = o.key_width.unwrap_or(0.5 * height);
        if !(key.is_finite() && key > FIT) || key > length / 3.0 + FIT {
            return Err(format!("key_width must be more than zero and at most {:.3} mm", length / 3.0));
        }
        // The board that starts at the low end keeps a hook: 3/8 of the thickness up to the key,
        // 5/8 past it. The other board keeps the rest, and the key fills the step between them.
        let (neck, hook) = (0.375 * height, 0.625 * height);
        let (start, end) = (lo[along], hi[along]);
        let middle = 0.5 * (start + end);
        let (k0, k1) = (middle - 0.5 * key, middle + 0.5 * key);
        // A region of the overlap: `from..to` along the boards, `low..high` up from the hook's face.
        let region = |from: f64, to: f64, low: f64, high: f64| {
            let (mut a, mut b) = (lo, hi);
            (a[along], b[along]) = (from, to);
            let (x, y) = if o.flip { (hi[thick] - high, hi[thick] - low) } else { (lo[thick] + low, lo[thick] + high) };
            (a[thick], b[thick]) = (x, y);
            (a, b)
        };
        let first_is_low = first.min[along] + first.max[along] <= second.min[along] + second.max[along];
        let (low_board, high_board) = if first_is_low { (&first, &second) } else { (&second, &first) };
        let cut = |board: &Aabb, r: ([f64; 3], [f64; 3])| cut_block(board, r.0, r.1);
        let low_cuts = [cut(low_board, region(start, k1, neck, height)), cut(low_board, region(k1, end, hook, height))];
        let high_cuts = [cut(high_board, region(start, k0, 0.0, neck)), cut(high_board, region(k0, end, 0.0, hook))];
        let (a, b) = region(k0, k1, neck, hook);
        let across = hi[width] - lo[width];
        Some(Tabled {
            cuts_first: if first_is_low { low_cuts } else { high_cuts },
            cuts_second: if first_is_low { high_cuts } else { low_cuts },
            key: Block { corner: a, size: [b[0] - a[0], b[1] - a[1], b[2] - a[2]] },
            cut: ((k1 - start) * (height - neck) + (end - k1) * (height - hook)) * across,
        })
    } else {
        None
    };
    Ok(Plan {
        tabled,
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
    fn boards_of_two_thicknesses_are_refused() {
        let a = bx([0.0, 0.0, 0.0], [400.0, 45.0, 18.0]);
        let b = bx([256.0, 0.0, 0.0], [260.0, 45.0, 40.0]);
        assert!(plan(a, b, &Options { flip: true, ..Options::default() }).unwrap_err().contains("18.000 and 40.000"));
    }

    #[test]
    fn align_lays_the_boards_end_to_end_and_the_slope_sets_the_overlap() {
        let a = bx([0.0, 10.0, 5.0], [400.0, 45.0, 18.0]);
        let b = bx([420.0, 0.0, 0.0], [400.0, 45.0, 18.0]);
        assert_eq!(align(&a, &b).unwrap(), [20.0, -10.0, -5.0]);
        let mut moved = a;
        for i in 0..3 {
            moved.min[i] += [20.0, -10.0, -5.0][i];
            moved.max[i] += [20.0, -10.0, -5.0][i];
        }
        // At 1:6, 108 mm over 18 mm boards.
        assert_eq!(slide(&moved, &b, Some(6.0), false).unwrap(), Some([108.0, 0.0, 0.0]));
        assert!(slide(&a, &bx([700.0, 10.0, 5.0], [400.0, 45.0, 18.0]), None, false).unwrap_err().contains("300 mm apart"));
    }

    #[test]
    fn boards_end_to_end_slide_by_the_slope() {
        // Two 400 x 45 x 18 boards that touch end to end: the first slides 8 x 18 = 144 mm.
        let a = bx([0.0, 0.0, 0.0], [400.0, 45.0, 18.0]);
        let b = bx([400.0, 0.0, 0.0], [400.0, 45.0, 18.0]);
        assert_eq!(slide(&a, &b, None, false).unwrap(), Some([144.0, 0.0, 0.0]));
        // 20 mm apart, on the far side, at 1:6: 20 + 108 the other way.
        let c = bx([-420.0, 0.0, 0.0], [400.0, 45.0, 18.0]);
        assert_eq!(slide(&a, &c, Some(6.0), false).unwrap(), Some([-128.0, 0.0, 0.0]));
        // Already overlapping, or not level: not slid.
        assert_eq!(slide(&a, &bx([256.0, 0.0, 0.0], [400.0, 45.0, 18.0]), None, false).unwrap(), None);
        assert_eq!(slide(&a, &bx([400.0, 0.0, 30.0], [400.0, 45.0, 18.0]), None, false).unwrap(), None);
        assert!(slide(&a, &b, Some(30.0), false).unwrap_err().contains("give a smaller slope"));
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
    fn align_lays_the_boards_end_to_end_level() {
        let (_, b) = boards();
        // The first board lies 50 mm short of the second, 10 mm aside and 5 mm too high: align
        // closes the 50 mm and levels it; the slide then overlaps it by 144 (1:8), 194 in all.
        let loose = bx([-194.0, 10.0, 5.0], [400.0, 45.0, 18.0]);
        assert_eq!(align(&loose, &b).unwrap(), [50.0, -10.0, -5.0]);
        let across = bx([0.0, 0.0, 0.0], [45.0, 400.0, 18.0]);
        assert!(align(&across, &b).unwrap_err().contains("same way"));
    }

    #[test]
    fn a_tabled_scarf_hooks_the_two_boards_around_a_key() {
        let (a, b) = boards();
        let p = plan(a, b, &Options { key: true, ..Options::default() }).unwrap();
        let t = p.tabled.unwrap();
        // The key: 9 long in the middle of the overlap (X = 328), between 3/8 and 5/8 of the thickness.
        assert_eq!(t.key, Block { corner: [323.5, 0.0, 6.75], size: [9.0, 45.0, 4.5] });
        // Each board loses the same: 76.5 x 11.25 and 67.5 x 6.75, over 45 mm.
        assert_eq!(t.cut, (76.5 * 11.25 + 67.5 * 6.75) * 45.0);
        // The first board is cut from above: over the key and to its own end.
        assert_eq!(t.cuts_first[0], Block { corner: [256.0, -0.5, 6.75], size: [76.5, 46.0, 11.75] });
        assert_eq!(t.cuts_first[1], Block { corner: [332.5, -0.5, 11.25], size: [68.0, 46.0, 7.25] });
        // The second from below, from its end to the key and beyond.
        assert_eq!(t.cuts_second[0], Block { corner: [255.5, -0.5, -0.5], size: [68.0, 46.0, 7.25] });
        let short = bx([370.0, 0.0, 0.0], [400.0, 45.0, 18.0]);
        assert!(plan(a, short, &Options { key: true, ..Options::default() }).unwrap_err().contains("three thicknesses"));
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
