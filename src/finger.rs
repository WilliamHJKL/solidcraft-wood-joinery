//! A box joint: the end of one board meets the face of another in a row of square fingers.

use crate::boards::{contact, cut_block, measurable, FIT};
use crate::joint::{Aabb, Block};

/// What the user may set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Fingers and gaps across the width. Default: the odd number that makes them nearest to
    /// square, at least 3.
    pub count: Option<u32>,
    /// The first board gets the second, fourth... fingers. `true` gives it the first, third...
    pub swap: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Added to the first board.
    pub fingers: Vec<Block>,
    /// Removed from the second board.
    pub sockets: Vec<Block>,
    pub count: u32,
    pub finger_width: f64,
    pub finger_length: f64,
}

/// How far to move `first` so that its end lies on the face of `second`, at the nearest end of
/// `second`, with the two boards level across their width.
///
/// A board is read by its thickness, its smallest side, not by its longest one: a cabinet side
/// may be taller than it is long.
pub fn align(first: &Aabb, second: &Aabb) -> Result<[f64; 3], String> {
    measurable(first)?;
    measurable(second)?;
    let thin = |b: &Aabb| (0..3).min_by(|&x, &y| b.size(x).total_cmp(&b.size(y))).unwrap_or(0);
    // `n` is square to the face of the second board, `m` runs along that board towards its
    // end, and `l` is the width the two boards share.
    let (m, n) = (thin(first), thin(second));
    if m == n {
        return Err("the two boards lie flat on each other; turn one so they make a corner".into());
    }
    let l = 3 - n - m;
    let centre = |b: &Aabb, a: usize| 0.5 * (b.min[a] + b.max[a]);
    let mut shift = [0.0; 3];
    shift[n] = if centre(first, n) >= centre(second, n) { second.max[n] - first.min[n] } else { second.min[n] - first.max[n] };
    shift[m] = if centre(first, m) <= centre(second, m) { second.min[m] - first.min[m] } else { second.max[m] - first.max[m] };
    shift[l] = centre(second, l) - centre(first, l);
    Ok(shift)
}

/// Plan a box joint where the end of `first` lies on a face of `second`.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let c = contact(&first, &second).ok_or(
        "the end of the first board does not lie on a face of the second; move it against the second board first",
    )?;
    let n = c.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    // The fingers are counted along the width of the joint, the longer side of the contact.
    let (l, m) = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { (p, q) } else { (q, p) };
    let (width, thickness) = (c.hi[l] - c.lo[l], c.hi[m] - c.lo[m]);

    let count = match o.count {
        Some(k) if k < 2 => return Err("count must be at least 2".into()),
        Some(k) => k,
        None => (2.0 * ((width / thickness - 1.0) * 0.5).round() + 1.0).max(3.0) as u32,
    };
    let finger_width = width / f64::from(count);
    if finger_width < 2.0 - FIT {
        return Err(format!("{count} fingers across {width:.3} mm are under 2 mm wide; give a smaller count"));
    }
    // The fingers go through the whole second board.
    let finger_length = second.size(n);
    let far = c.plane() + c.dir * finger_length;
    // Each finger starts a hair inside the first board, so the two join into one body.
    let back = c.plane() - c.dir * 0.1_f64.min(first.size(n) * 0.05);

    let mut fingers = Vec::new();
    let mut sockets = Vec::new();
    for i in (if o.swap { 0 } else { 1 }..count).step_by(2) {
        let mut lo = c.lo;
        let mut hi = c.hi;
        lo[l] = c.lo[l] + finger_width * f64::from(i);
        hi[l] = c.lo[l] + finger_width * f64::from(i + 1);
        lo[n] = c.plane().min(far);
        hi[n] = c.plane().max(far);
        sockets.push(cut_block(&second, lo, hi));
        lo[n] = back.min(far);
        hi[n] = back.max(far);
        fingers.push(Block { corner: lo, size: [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]] });
    }
    Ok(Plan { fingers, sockets, count, finger_width, finger_length })
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
    fn default_fingers_are_near_square() {
        let (side, front) = corner();
        let p = plan(side, front, &Options::default()).unwrap();
        assert_eq!((p.count, p.finger_width, p.finger_length), (5, 20.0, 18.0));
        // The side gets fingers 2 and 4, through the front, lapping 0.1 mm into the side.
        assert_eq!(p.fingers.len(), 2);
        assert_eq!(p.fingers[0], Block { corner: [0.0, 0.0, 20.0], size: [18.1, 18.0, 20.0] });
        // The sockets are open on both faces of the front and at its end.
        assert_eq!(p.sockets[1], Block { corner: [-0.5, -0.5, 60.0], size: [19.0, 18.5, 20.0] });
    }

    #[test]
    fn swap_and_count() {
        let (side, front) = corner();
        let p = plan(side, front, &Options { count: Some(4), swap: true }).unwrap();
        assert_eq!((p.fingers.len(), p.finger_width), (2, 25.0));
        // The first finger is at the lower edge, so its socket is open there too.
        assert_eq!(p.sockets[0], Block { corner: [-0.5, -0.5, -0.5], size: [19.0, 18.5, 25.5] });
    }

    #[test]
    fn align_brings_the_first_board_to_the_nearest_corner() {
        let (side, front) = corner();
        assert_eq!(align(&side, &front).unwrap(), [0.0, 0.0, 0.0]);
        // 40 mm away, 25 mm in from the end of the front and 7 mm too high.
        let loose = bx([58.0, 25.0, 7.0], [300.0, 18.0, 100.0]);
        assert_eq!(align(&loose, &front).unwrap(), [-40.0, -25.0, -7.0]);
        // On the other side of the front, near its far end, and run into it by 5 mm.
        let other = bx([-295.0, 270.0, 0.0], [300.0, 18.0, 100.0]);
        assert_eq!(align(&other, &front).unwrap(), [-5.0, 12.0, 0.0]);
        let parallel = bx([40.0, 0.0, 0.0], [18.0, 300.0, 100.0]);
        assert!(align(&parallel, &front).unwrap_err().contains("flat on each other"));
    }

    #[test]
    fn align_reads_a_board_by_its_thickness_not_its_longest_side() {
        // Two cabinet panels, both taller (400) than they are long: a side turned to run along
        // Y, and a back along X. The side sits 50 mm away and 30 mm too high.
        let side = bx([-18.0, 68.0, 30.0], [18.0, 300.0, 400.0]);
        let back = bx([-18.0, 0.0, 0.0], [350.0, 18.0, 400.0]);
        // The side comes down its length (Y) onto the back, not up onto its top edge.
        assert_eq!(align(&side, &back).unwrap(), [0.0, -50.0, -30.0]);
    }

    #[test]
    fn bad_box_joints_are_refused_with_a_reason() {
        let (side, front) = corner();
        let apart = bx([30.0, 0.0, 0.0], [300.0, 18.0, 100.0]);
        assert!(plan(apart, front, &Options::default()).unwrap_err().contains("does not lie on a face"));
        assert!(plan(side, front, &Options { count: Some(80), swap: false }).unwrap_err().contains("under 2 mm"));
        assert!(plan(side, front, &Options { count: Some(1), swap: false }).unwrap_err().contains("at least 2"));
    }
}
