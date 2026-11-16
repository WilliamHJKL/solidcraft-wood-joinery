//! A housing: the end of a shelf let into the face of a side panel.

use crate::boards::{contact, cut_block, measurable, FIT};
use crate::joint::{Aabb, Block};
use crate::slant::{beyond, Tool};

/// What the user may set. `None` means the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// How far the shelf goes into the side. Default: a third of the side, to the nearest mm.
    pub depth: Option<f64>,
    /// Wood left uncut in the side at the end of the joint with the lower coordinate. Default: none.
    pub stop_start: Option<f64>,
    /// The same at the other end.
    pub stop_end: Option<f64>,
    /// A sliding dovetail: the tongue widens into the side at 1:N. Default: 0, a square housing.
    pub slope: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    /// Added to the shelf: its end, made longer by `depth`.
    pub tongue: Block,
    /// Removed from the side.
    pub socket: Block,
    pub depth: f64,
    /// Thickness of the shelf, which is the width of the housing.
    pub width: f64,
    pub length: f64,
    /// For a sliding dovetail: the two tools that slope the sides of the tongue, and the width
    /// of the tongue where it leaves the shelf. The tongue is then made as a body of its own.
    pub sides: Option<([Tool; 2], f64)>,
}

fn not_negative(name: &str, v: f64) -> Result<f64, String> {
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(format!("{name} must be zero or a positive length"))
    }
}

/// Plan a housing where the end of `shelf` lies on a face of `side`.
pub fn plan(shelf: Aabb, side: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&shelf)?;
    measurable(&side)?;
    let c = contact(&shelf, &side).ok_or(
        "the end of the shelf does not lie on a face of the side; move it against the side first",
    )?;
    let n = c.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    // `l` runs along the housing, `m` is the thickness of the shelf.
    let (l, m) = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { (p, q) } else { (q, p) };
    let thick = side.size(n);
    let depth = o.depth.unwrap_or((thick / 3.0).round().max(1.0));
    if !(depth.is_finite() && depth > FIT) {
        return Err("depth must be a positive length".into());
    }
    if depth >= thick - FIT {
        return Err(format!("depth ({depth:.3} mm) would cut through the {thick:.3} mm side"));
    }
    let stop_start = not_negative("stop_start", o.stop_start.unwrap_or(0.0))?;
    let stop_end = not_negative("stop_end", o.stop_end.unwrap_or(0.0))?;
    let (mut lo, mut hi) = (c.lo, c.hi);
    lo[l] += stop_start;
    hi[l] -= stop_end;
    if hi[l] - lo[l] <= FIT {
        return Err("the two stops leave no housing".into());
    }
    let (near, far) = (c.plane(), c.plane() + c.dir * depth);
    (lo[n], hi[n]) = (near.min(far), near.max(far));
    let socket = cut_block(&side, lo, hi);
    // The tongue starts a hair inside the shelf, so the two join into one body.
    let back = c.plane() - c.dir * 0.1_f64.min(shelf.size(n) * 0.05);
    (lo[n], hi[n]) = (back.min(far), back.max(far));
    let tongue = Block { corner: lo, size: [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]] };
    let width = hi[m] - lo[m];
    let slope = o.slope.unwrap_or(0.0);
    if !(slope.is_finite() && (slope == 0.0 || slope >= 1.0)) {
        return Err("slope must be 0 for a square housing, or 1 or more: 6 means a slope of 1:6".into());
    }
    let sides = if slope > 0.0 {
        let lean = depth / slope;
        let neck = width - 2.0 * lean;
        if neck < width / 3.0 - FIT {
            return Err(format!("a slope of 1:{slope} leaves a neck of {neck:.3} mm; give a higher slope or a shallower housing"));
        }
        let run = (depth * depth + lean * lean).sqrt();
        let middle = 0.5 * (lo[m] + hi[m]);
        let mut keep = lo;
        keep[m] = middle;
        keep[n] = 0.5 * (near + far);
        let side = |sign: f64| {
            // From the neck, on the face of the side, out to the full thickness at the bottom.
            let mut from = lo;
            from[m] = middle + sign * 0.5 * neck;
            from[n] = near;
            let mut w = [0.0; 3];
            w[m] = sign * lean / run;
            w[n] = c.dir * depth / run;
            beyond(keep, from, w, l, m, (lo[l], hi[l]), depth + width)
        };
        Some(([side(-1.0), side(1.0)], neck))
    } else {
        None
    };
    Ok(Plan { tongue, socket, depth, width, length: hi[l] - lo[l], sides })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::bx;

    /// The same block, give or take float noise.
    fn near(a: Block, b: Block) -> bool {
        (0..3).all(|i| (a.corner[i] - b.corner[i]).abs() < 1e-9 && (a.size[i] - b.size[i]).abs() < 1e-9)
    }

    /// A 300 x 250 x 18 shelf against the X = 18 face of an 18 x 250 x 400 side, at Z = 200.
    fn shelf_and_side() -> (Aabb, Aabb) {
        (bx([18.0, 0.0, 200.0], [300.0, 250.0, 18.0]), bx([0.0, 0.0, 0.0], [18.0, 250.0, 400.0]))
    }

    #[test]
    fn a_through_housing_is_a_third_of_the_side_deep() {
        let (shelf, side) = shelf_and_side();
        let p = plan(shelf, side, &Options::default()).unwrap();
        assert_eq!((p.depth, p.width, p.length), (6.0, 18.0, 250.0));
        // The shelf grows 6 mm into the side, lapping 0.1 mm into itself.
        assert!(near(p.tongue, Block { corner: [12.0, 0.0, 200.0], size: [6.1, 250.0, 18.0] }));
        // The housing is open on the face of the side and at both of its edges.
        assert_eq!(p.socket, Block { corner: [12.0, -0.5, 200.0], size: [6.5, 251.0, 18.0] });
    }

    #[test]
    fn a_stopped_housing_leaves_the_front_edge_whole() {
        let (shelf, side) = shelf_and_side();
        let o = Options { depth: Some(8.0), stop_end: Some(20.0), ..Options::default() };
        let p = plan(shelf, side, &o).unwrap();
        assert_eq!((p.depth, p.length), (8.0, 230.0));
        assert!(near(p.tongue, Block { corner: [10.0, 0.0, 200.0], size: [8.1, 230.0, 18.0] }));
        assert_eq!(p.socket, Block { corner: [10.0, -0.5, 200.0], size: [8.5, 230.5, 18.0] });
    }

    #[test]
    fn a_slope_makes_a_sliding_dovetail() {
        let (shelf, side) = shelf_and_side();
        let p = plan(shelf, side, &Options { slope: Some(6.0), ..Options::default() }).unwrap();
        // 6 mm deep at 1:6: each side leans 1 mm, so the neck is 16 mm.
        let (tools, neck) = p.sides.unwrap();
        assert!((neck - 16.0).abs() < 1e-9);
        // The tools turn about lines along the housing, on the face of the side, 16 mm apart.
        assert_eq!((tools[0].origin, tools[1].origin), ([18.0, 0.0, 201.0], [18.0, 0.0, 217.0]));
        assert_eq!(tools[0].axis, [0.0, 1.0, 0.0]);
        let steep = plan(shelf, side, &Options { slope: Some(1.0), depth: Some(8.0), ..Options::default() });
        assert!(steep.unwrap_err().contains("neck"));
    }

    #[test]
    fn bad_housings_are_refused_with_a_reason() {
        let (shelf, side) = shelf_and_side();
        let err = |o: Options| plan(shelf, side, &o).unwrap_err();
        let apart = bx([30.0, 0.0, 200.0], [300.0, 250.0, 18.0]);
        assert!(plan(apart, side, &Options::default()).unwrap_err().contains("does not lie on a face"));
        assert!(err(Options { depth: Some(18.0), ..Options::default() }).contains("cut through"));
        assert!(err(Options { stop_start: Some(200.0), stop_end: Some(60.0), ..Options::default() }).contains("no housing"));
        assert!(err(Options { depth: Some(-1.0), ..Options::default() }).contains("positive"));
    }
}
