//! Loose tenons across two boards that touch: matching mortises in both, and the tenons.

use crate::boards::{contact, cut_block, measurable, share, FIT, too_many};
use crate::joint::{Aabb, Block};

/// The most tenons one call makes: see `boards::too_many`.
pub const MAX: u32 = 40;

/// What the user may set. `None` means the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Default: the largest of 5, 6, 8 and 10 mm that is at most a third of the thinner side.
    pub thickness: Option<f64>,
    /// Across the joint. Default: its share of the joint, less a shoulder on each side.
    pub width: Option<f64>,
    /// Length of one tenon. Default: five thicknesses, rounded up to 10 mm.
    pub length: Option<f64>,
    /// Default: one, then one every 120 mm or so.
    pub count: Option<u32>,
    /// Extra depth at the bottom of each mortise. Default: 1 mm.
    pub glue_relief: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub mortises_first: Vec<Block>,
    pub mortises_second: Vec<Block>,
    pub tenons: Vec<Block>,
    pub thickness: f64,
    pub width: f64,
    pub length: f64,
    /// How much of the tenon is in each board.
    pub in_first: f64,
    pub in_second: f64,
}

/// Plan loose tenons across the faces where `first` and `second` touch.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let c = contact(&first, &second)
        .ok_or("the two boards do not touch; move one against the other, where the tenons go")?;
    let n = c.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    // The tenons stand in a row along the longer side of the contact, centred across it.
    let (l, m) = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { (p, q) } else { (q, p) };
    let (long, short) = (c.hi[l] - c.lo[l], c.hi[m] - c.lo[m]);

    let thickness = match o.thickness {
        Some(t) => t,
        None => [10.0, 8.0, 6.0, 5.0].into_iter().find(|t| *t <= short / 3.0 + FIT).unwrap_or(short / 3.0),
    };
    if !(thickness.is_finite() && thickness > FIT) {
        return Err("thickness must be a positive length".into());
    }
    // 3 mm of wood at least on each side of the mortise.
    if thickness > short - 6.0 + FIT {
        return Err(format!(
            "a {thickness:.3} mm tenon leaves no cheek beside it: the boards touch over {short:.3} mm only"
        ));
    }
    let count = match o.count {
        Some(0) => return Err("count must be at least 1".into()),
        Some(k) if k > MAX => return Err(too_many(k, MAX, "tenons")),
        Some(k) => k,
        None => ((long / 120.0).ceil().max(1.0) as u32).min(MAX),
    };
    let pitch = long / f64::from(count);
    let width = match o.width {
        Some(w) => w,
        None => pitch - 2.0 * (0.5 * (short - thickness)).min(0.2 * pitch),
    };
    if !(width.is_finite() && width > FIT) {
        return Err("width must be a positive length".into());
    }
    if width > pitch - 2.0 + FIT {
        return Err(format!(
            "{count} tenons {width:.3} mm wide do not fit in {long:.3} mm; give fewer or narrower ones"
        ));
    }
    let length = o.length.unwrap_or((5.0 * thickness / 10.0).ceil() * 10.0);
    if !(length.is_finite() && length > FIT) {
        return Err("length must be a positive length".into());
    }
    let relief = o.glue_relief.unwrap_or(1.0);
    if !(relief.is_finite() && relief >= 0.0) {
        return Err("glue_relief must be zero or a positive length".into());
    }
    let (in_first, in_second) = share(length, relief, first.size(n), second.size(n)).map_err(|most| {
        format!("a {length:.3} mm tenon is too long for these boards; give a length of at most {most:.3} mm")
    })?;

    let mut plan = Plan {
        mortises_first: Vec::new(),
        mortises_second: Vec::new(),
        tenons: Vec::new(),
        thickness,
        width,
        length,
        in_first,
        in_second,
    };
    let plane = c.plane();
    for i in 0..count {
        let mut lo = [0.0; 3];
        let mut hi = [0.0; 3];
        let centre = c.lo[l] + pitch * (f64::from(i) + 0.5);
        (lo[l], hi[l]) = (centre - 0.5 * width, centre + 0.5 * width);
        let middle = 0.5 * (c.lo[m] + c.hi[m]);
        (lo[m], hi[m]) = (middle - 0.5 * thickness, middle + 0.5 * thickness);
        // Along the joint, from `from` to `to`, counted from the contact towards the second board.
        let span = |from: f64, to: f64| {
            let (a, b) = (plane + c.dir * from, plane + c.dir * to);
            let (mut lo, mut hi) = (lo, hi);
            (lo[n], hi[n]) = (a.min(b), a.max(b));
            (lo, hi)
        };
        let (a_lo, a_hi) = span(-(in_first + relief), 0.0);
        plan.mortises_first.push(cut_block(&first, a_lo, a_hi));
        let (b_lo, b_hi) = span(0.0, in_second + relief);
        plan.mortises_second.push(cut_block(&second, b_lo, b_hi));
        let (t_lo, t_hi) = span(-in_first, in_second);
        plan.tenons.push(Block { corner: t_lo, size: [t_hi[0] - t_lo[0], t_hi[1] - t_lo[1], t_hi[2] - t_lo[2]] });
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::bx;

    /// A 360 x 18 x 45 rail against the X = 35 face of a 35 x 35 x 830 stile.
    fn cabinet() -> (Aabb, Aabb) {
        (bx([35.0, 0.0, 400.0], [360.0, 18.0, 45.0]), bx([0.0, 0.0, 0.0], [35.0, 35.0, 830.0]))
    }

    #[test]
    fn defaults_for_a_rail_end() {
        let (rail, stile) = cabinet();
        let p = plan(rail, stile, &Options::default()).unwrap();
        assert_eq!((p.thickness, p.width, p.length, p.in_first, p.in_second), (6.0, 33.0, 30.0, 15.0, 15.0));
        // In the rail: 16 mm deep from X = 35, open on the contact face.
        assert_eq!(p.mortises_first, vec![Block { corner: [34.5, 6.0, 406.0], size: [16.5, 6.0, 33.0] }]);
        // In the stile: 16 mm deep the other way.
        assert_eq!(p.mortises_second, vec![Block { corner: [19.0, 6.0, 406.0], size: [16.5, 6.0, 33.0] }]);
        // The tenon: 30 long, half in each board.
        assert_eq!(p.tenons, vec![Block { corner: [20.0, 6.0, 406.0], size: [30.0, 6.0, 33.0] }]);
    }

    #[test]
    fn a_long_joint_gets_several_tenons() {
        // A 360 x 300 x 18 shelf against a side panel 18 mm thick.
        let shelf = bx([18.0, 0.0, 0.0], [360.0, 300.0, 18.0]);
        let side = bx([0.0, 0.0, -100.0], [18.0, 300.0, 400.0]);
        let p = plan(shelf, side, &Options { width: Some(20.0), ..Options::default() }).unwrap();
        assert_eq!((p.thickness, p.tenons.len()), (6.0, 3));
        // The side is 18 mm thick: 14 mm in it at most, so 16 mm in the shelf.
        assert_eq!((p.in_first, p.in_second), (16.0, 14.0));
        assert_eq!(p.tenons[0].corner, [4.0, 40.0, 6.0]);
        assert_eq!(p.tenons[2].size, [30.0, 20.0, 6.0]);
    }

    #[test]
    fn bad_loose_tenons_are_refused_with_a_reason() {
        let (rail, stile) = cabinet();
        let err = |o: Options| plan(rail, stile, &o).unwrap_err();
        let apart = bx([45.0, 0.0, 400.0], [360.0, 18.0, 45.0]);
        assert!(plan(apart, stile, &Options::default()).unwrap_err().contains("do not touch"));
        assert!(err(Options { thickness: Some(17.0), ..Options::default() }).contains("no cheek"));
        assert!(err(Options { width: Some(45.0), ..Options::default() }).contains("do not fit"));
        assert!(err(Options { length: Some(400.0), ..Options::default() }).contains("too long"));
        assert!(err(Options { count: Some(0), ..Options::default() }).contains("at least 1"));
    }

    #[test]
    fn a_6_m_joint_gets_40_tenons_at_most() {
        let (a, b) = (bx([0.0, 0.0, 0.0], [6000.0, 100.0, 18.0]), bx([0.0, 100.0, 0.0], [6000.0, 100.0, 18.0]));
        // Worked out from the length: cut down to MAX.
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!(p.tenons.len() as u32, MAX);
        // Given above MAX: refused before anything is cut.
        assert!(plan(a, b, &Options { count: Some(MAX + 1), ..Options::default() }).unwrap_err().contains("or fewer"));
    }
}
