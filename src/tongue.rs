//! Tongue and groove: a tongue along the edge of one board, the matching groove in the other.

use crate::boards::{contact, cut_block, measurable, FIT};
use crate::joint::{Aabb, Block};

/// What the user may set. `None` means the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Default: a third of the thinner side of the contact.
    pub thickness: Option<f64>,
    /// How far the tongue goes into the second board. Default: half the thickness of the
    /// boards, to the nearest mm, at most 10 mm.
    pub depth: Option<f64>,
    /// Extra depth at the bottom of the groove. Default: 1 mm.
    pub glue_relief: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    /// Added to the first board.
    pub tongue: Block,
    /// Removed from the second board.
    pub groove: Block,
    pub thickness: f64,
    pub depth: f64,
    pub length: f64,
    pub groove_depth: f64,
}

/// Plan a tongue on `first` and a groove in `second`, where the two touch.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let c = contact(&first, &second)
        .ok_or("the two boards do not touch; move one against the other, edge to edge")?;
    let n = c.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    // The tongue runs along the longer side of the contact, centred across it.
    let (l, m) = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { (p, q) } else { (q, p) };
    let short = c.hi[m] - c.lo[m];
    let thickness = o.thickness.unwrap_or(short / 3.0);
    if !(thickness.is_finite() && thickness > FIT) {
        return Err("thickness must be a positive length".into());
    }
    if thickness > short - 2.0 + FIT {
        return Err(format!(
            "a {thickness:.3} mm tongue leaves no cheek beside it: the boards touch over {short:.3} mm only"
        ));
    }
    let depth = o.depth.unwrap_or((0.5 * short).round().clamp(1.0, 10.0));
    if !(depth.is_finite() && depth > FIT) {
        return Err("depth must be a positive length".into());
    }
    let relief = o.glue_relief.unwrap_or(1.0);
    if !(relief.is_finite() && relief >= 0.0) {
        return Err("glue_relief must be zero or a positive length".into());
    }
    let groove_depth = depth + relief;
    if groove_depth >= second.size(n) - 3.0 + FIT {
        return Err(format!(
            "a {groove_depth:.3} mm groove is too deep for the second board; give a depth of at most {:.3} mm",
            (second.size(n) - 3.0 - relief).max(0.0)
        ));
    }
    let (mut lo, mut hi) = (c.lo, c.hi);
    let middle = 0.5 * (c.lo[m] + c.hi[m]);
    (lo[m], hi[m]) = (middle - 0.5 * thickness, middle + 0.5 * thickness);
    let (near, far) = (c.plane(), c.plane() + c.dir * groove_depth);
    (lo[n], hi[n]) = (near.min(far), near.max(far));
    let groove = cut_block(&second, lo, hi);
    // The tongue starts a hair inside the first board, so the two join into one body.
    let back = c.plane() - c.dir * 0.1_f64.min(first.size(n) * 0.05);
    let tip = c.plane() + c.dir * depth;
    (lo[n], hi[n]) = (back.min(tip), back.max(tip));
    let tongue = Block { corner: lo, size: [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]] };
    Ok(Plan { tongue, groove, thickness, depth, length: hi[l] - lo[l], groove_depth })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::bx;

    /// Two 600 x 100 x 18 boards of a panel, edge to edge along Y.
    fn panel() -> (Aabb, Aabb) {
        (bx([0.0, 0.0, 0.0], [600.0, 100.0, 18.0]), bx([0.0, 100.0, 0.0], [600.0, 100.0, 18.0]))
    }

    #[test]
    fn defaults_for_an_18_mm_panel() {
        let (a, b) = panel();
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!((p.thickness, p.depth, p.groove_depth, p.length), (6.0, 9.0, 10.0, 600.0));
        // The tongue: 6 thick, centred, 9 mm into the second board, lapping 0.1 mm into the first.
        assert!((p.tongue.corner[1] - 99.9).abs() < 1e-9 && (p.tongue.size[1] - 9.1).abs() < 1e-9);
        assert_eq!((p.tongue.corner[2], p.tongue.size[2], p.tongue.size[0]), (6.0, 6.0, 600.0));
        // The groove: 10 deep, open on the edge and at both ends.
        assert_eq!(p.groove, Block { corner: [-0.5, 99.5, 6.0], size: [601.0, 10.5, 6.0] });
    }

    #[test]
    fn bad_tongues_are_refused_with_a_reason() {
        let (a, b) = panel();
        let err = |o: Options| plan(a, b, &o).unwrap_err();
        let apart = bx([0.0, 120.0, 0.0], [600.0, 100.0, 18.0]);
        assert!(plan(a, apart, &Options::default()).unwrap_err().contains("do not touch"));
        assert!(err(Options { thickness: Some(17.0), ..Options::default() }).contains("no cheek"));
        assert!(err(Options { depth: Some(99.0), ..Options::default() }).contains("too deep"));
    }
}
