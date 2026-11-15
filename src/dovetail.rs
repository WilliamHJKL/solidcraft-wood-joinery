//! Through dovetails: tails on the end of one board, through the other.
//!
//! Each tail is first made as a body of its own: a block whose two sides are cut at the slope.
//! That body cuts its socket in the pin board, then joins the tail board.

use crate::boards::{contact, measurable, FIT};
use crate::joint::{Aabb, Block};
use crate::slant::{beyond, Tool};

/// What the user may set. `None` means the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Number of tails. Default: one every two and a half thicknesses or so.
    pub count: Option<u32>,
    /// The slope as 1:N. Default: 8, the usual hardwood slope; 6 for softwood.
    pub slope: Option<f64>,
    /// Width of a pin at its narrow end. Default: half the thickness of the tail board.
    pub pin: Option<f64>,
    /// Wood left in front of the tails, for half-blind dovetails. Default: 0, through dovetails.
    pub lap: Option<f64>,
}

/// One tail: the block it is cut from, and the two tools that slope its sides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tail {
    pub blank: Block,
    pub sides: [Tool; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub tails: Vec<Tail>,
    pub slope: f64,
    pub pin: f64,
    /// Width of a tail at its end, where it is widest, and at its base.
    pub tail_wide: f64,
    pub tail_narrow: f64,
    pub tail_length: f64,
    /// Volume of one tail outside the tail board, in mm3.
    pub tail_volume: f64,
}

/// Plan dovetails where the end of `first`, the tail board, lies on a face of `second`.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let c = contact(&first, &second).ok_or(
        "the end of the first board does not lie on a face of the second; move it against the second board first",
    )?;
    let n = c.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    // The tails are counted along the width of the joint; `m` is the thickness of the tail board.
    let (l, m) = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { (p, q) } else { (q, p) };
    let (width, thickness) = (c.hi[l] - c.lo[l], c.hi[m] - c.lo[m]);
    let lap = o.lap.unwrap_or(0.0);
    if !(lap.is_finite() && lap >= 0.0) {
        return Err("lap must be zero or a positive length".into());
    }
    let tail_length = second.size(n) - lap;
    if tail_length < 3.0 - FIT {
        return Err(format!("a lap of {lap:.3} mm leaves no room for the tails in the {:.3} mm board", second.size(n)));
    }

    let slope = o.slope.unwrap_or(8.0);
    if !(slope.is_finite() && slope >= 1.0) {
        return Err("slope must be 1 or more: 8 means a slope of 1:8".into());
    }
    let count = match o.count {
        Some(0) => return Err("count must be at least 1".into()),
        Some(k) => k,
        None => (width / (2.5 * thickness)).round().max(1.0) as u32,
    };
    let pin = o.pin.unwrap_or(0.5 * thickness);
    if !(pin.is_finite() && pin >= 2.0 - FIT) {
        return Err("pin must be at least 2 mm".into());
    }
    let pitch = width / f64::from(count);
    let lean = tail_length / slope;
    let tail_wide = pitch - pin;
    let tail_narrow = tail_wide - 2.0 * lean;
    if tail_narrow < 3.0 - FIT {
        return Err(format!(
            "{count} tails across {width:.3} mm leave tails under 3 mm wide at their base; give fewer tails or narrower pins"
        ));
    }
    let (base, far) = (c.plane(), c.plane() + c.dir * tail_length);
    // Each blank starts a hair inside the tail board, so the tail joins it into one body.
    let back = base - c.dir * 0.1_f64.min(first.size(n) * 0.05);
    let run = (tail_length * tail_length + lean * lean).sqrt();
    let mut tails = Vec::new();
    for i in 0..count {
        let middle = c.lo[l] + pitch * (f64::from(i) + 0.5);
        let mut corner = c.lo;
        let mut size = [0.0; 3];
        corner[l] = middle - 0.5 * tail_wide;
        size[l] = tail_wide;
        corner[n] = back.min(far);
        size[n] = (far - back).abs();
        size[m] = thickness;
        let mut keep = c.lo;
        keep[l] = middle;
        keep[n] = 0.5 * (base + far);
        let side = |sign: f64| {
            // The side runs from the base of the tail, where it is narrow, to its wide end.
            let mut from = c.lo;
            from[l] = middle + sign * 0.5 * tail_narrow;
            from[n] = base;
            let mut w = [0.0; 3];
            w[l] = sign * lean / run;
            w[n] = c.dir * tail_length / run;
            beyond(keep, from, w, m, l, (c.lo[m], c.hi[m]), tail_length + tail_wide)
        };
        tails.push(Tail { blank: Block { corner, size }, sides: [side(-1.0), side(1.0)] });
    }
    Ok(Plan {
        tails,
        slope,
        pin,
        tail_wide,
        tail_narrow,
        tail_length,
        tail_volume: 0.5 * (tail_wide + tail_narrow) * tail_length * thickness,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::bx;

    /// A drawer corner: the end of an 18 x 100 side on the face of an 18 x 100 front, at its end.
    fn corner() -> (Aabb, Aabb) {
        (bx([18.0, 0.0, 0.0], [300.0, 18.0, 100.0]), bx([0.0, 0.0, 0.0], [18.0, 300.0, 100.0]))
    }

    #[test]
    fn defaults_for_a_drawer_side() {
        let (side, front) = corner();
        let p = plan(side, front, &Options::default()).unwrap();
        // Two tails, pins 9 mm at their narrow end, slope 1:8 over 18 mm: 2.25 mm each side.
        assert_eq!((p.tails.len(), p.slope, p.pin, p.tail_wide, p.tail_narrow, p.tail_length), (2, 8.0, 9.0, 41.0, 36.5, 18.0));
        assert_eq!(p.tail_volume, 0.5 * (41.0 + 36.5) * 18.0 * 18.0);
        // The first blank: 41 wide around Z = 25, through the front, lapping 0.1 mm into the side.
        assert_eq!(p.tails[0].blank, Block { corner: [0.0, 0.0, 4.5], size: [18.1, 18.0, 41.0] });
        // Its sides turn about lines along Y at the base of the tail, 36.5 mm apart.
        assert_eq!((p.tails[0].sides[0].origin, p.tails[0].sides[1].origin), ([18.0, 0.0, 6.75], [18.0, 0.0, 43.25]));
        assert_eq!(p.tails[0].sides[0].axis, [0.0, 1.0, 0.0]);
        // The two sides lean the same amount, opposite ways.
        let lean = (1.0_f64 / 8.0).atan().to_degrees();
        assert!((p.tails[0].sides[0].angle.abs() - lean).abs() < 1e-9 || (p.tails[0].sides[0].angle.abs() - (180.0 - lean)).abs() < 1e-9);
    }

    #[test]
    fn bad_dovetails_are_refused_with_a_reason() {
        let (side, front) = corner();
        let err = |o: Options| plan(side, front, &o).unwrap_err();
        let apart = bx([30.0, 0.0, 0.0], [300.0, 18.0, 100.0]);
        assert!(plan(apart, front, &Options::default()).unwrap_err().contains("does not lie on a face"));
        assert!(err(Options { count: Some(9), ..Options::default() }).contains("under 3 mm"));
        assert!(err(Options { slope: Some(0.5), ..Options::default() }).contains("1 or more"));
        assert!(err(Options { pin: Some(1.0), ..Options::default() }).contains("at least 2"));
        assert!(err(Options { count: Some(0), ..Options::default() }).contains("at least 1"));
        assert!(err(Options { lap: Some(16.0), ..Options::default() }).contains("no room"));
    }

    #[test]
    fn a_lap_makes_half_blind_dovetails() {
        let (side, front) = corner();
        let p = plan(side, front, &Options { lap: Some(5.0), ..Options::default() }).unwrap();
        // The tails stop 5 mm short of the face of the front: 13 mm long, leaning 1.625 mm.
        assert_eq!((p.tail_length, p.tail_wide, p.tail_narrow), (13.0, 41.0, 37.75));
        let blank = p.tails[0].blank;
        assert_eq!((blank.corner, blank.size[1], blank.size[2]), ([5.0, 0.0, 4.5], 18.0, 41.0));
        assert!((blank.size[0] - 13.1).abs() < 1e-9);
    }
}
