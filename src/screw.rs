//! Screws through one board into another: a clearance hole with a countersink in the first
//! board, a pilot hole in the second.

use crate::boards::{contact, measurable, FIT, OVERCUT, too_many};
use crate::dowel::Bore;
use crate::joint::Aabb;

/// The most screws one call makes: see `boards::too_many`.
pub const MAX: u32 = 15;

/// What the user may set. `None` means the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Diameter of the screw. Default: 4 mm.
    pub diameter: Option<f64>,
    /// Length of the screw. Default: two and a half times the first board, to the nearest 5 mm.
    pub length: Option<f64>,
    /// Default: one, two from 60 mm, then one every 150 mm or so.
    pub count: Option<u32>,
    /// A countersink for the head. Default: true.
    pub countersink: Option<bool>,
}

/// A countersink: a cone that narrows from `diameter` to `top_diameter` over `height`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sink {
    pub base: [f64; 3],
    pub axis: usize,
    pub dir: f64,
    pub diameter: f64,
    pub top_diameter: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Through the first board.
    pub clearance: Vec<Bore>,
    pub sinks: Vec<Sink>,
    /// In the second board.
    pub pilots: Vec<Bore>,
    pub diameter: f64,
    pub length: f64,
    pub clearance_diameter: f64,
    pub pilot_diameter: f64,
    pub pilot_depth: f64,
    /// Depth of the countersink, 0 without one.
    pub sink_depth: f64,
}

/// Two boards with their faces the same way (both lying flat, say) that overlap seen from above,
/// drawn in one plane or apart, are laid one on the other before they are screwed: this is how
/// far to move `first`, the board under the heads, onto the face of `second`. `None` when they
/// already lie face on face, or do not have their faces the same way.
pub fn stack(first: &Aabb, second: &Aabb) -> Option<[f64; 3]> {
    let thin = |b: &Aabb| (0..3).min_by(|&x, &y| b.size(x).total_cmp(&b.size(y))).unwrap_or(2);
    let n = thin(first);
    if thin(second) != n {
        return None;
    }
    let shares = |a: usize| first.min[a].max(second.min[a]) < first.max[a].min(second.max[a]) - FIT;
    if !(0..3).filter(|&a| a != n).all(shares) {
        return None;
    }
    let face_on_face = (first.min[n] - second.max[n]).abs() <= FIT || (first.max[n] - second.min[n]).abs() <= FIT;
    if face_on_face {
        return None;
    }
    // Onto the side it is on; drawn in the same plane, on top.
    let mut shift = [0.0; 3];
    shift[n] = if first.min[n] + first.max[n] >= second.min[n] + second.max[n] {
        second.max[n] - first.min[n]
    } else {
        second.min[n] - first.max[n]
    };
    Some(shift)
}

/// Plan screws through `first` into `second`, where the two touch.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let c = contact(&first, &second)
        .ok_or("the two boards do not touch; move one against the other, where the screws go")?;
    let n = c.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    let (l, m) = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { (p, q) } else { (q, p) };
    let (long, short) = (c.hi[l] - c.lo[l], c.hi[m] - c.lo[m]);
    let through = first.size(n);
    let thinnest = (0..3).map(|a| first.size(a)).fold(f64::INFINITY, f64::min);
    if through > 2.5 * thinnest + FIT {
        return Err(format!(
            "the screws would run {through:.3} mm through the first board, the long way; lay it on the other \
             board face on face, where the screws go, or take the thinner board as the first one"
        ));
    }

    let diameter = o.diameter.unwrap_or(4.0);
    if !(diameter.is_finite() && diameter > FIT) {
        return Err("diameter must be a positive length".into());
    }
    let clearance_diameter = diameter + 0.5;
    let head = 2.0 * diameter;
    let countersink = o.countersink.unwrap_or(true);
    if (if countersink { head } else { clearance_diameter }) > short - 2.0 + FIT {
        return Err(format!("a {diameter:.3} mm screw leaves no wood beside it: the boards touch over {short:.3} mm only"));
    }
    // Empty: 2.5 times the first board, to 5 mm, but never through the second one.
    let longest = through + second.size(n) - 3.0;
    let length = o.length.unwrap_or(((2.5 * through / 5.0).round() * 5.0).min((longest / 5.0).floor() * 5.0));
    if !(length.is_finite() && length > FIT) {
        return Err("length must be a positive length".into());
    }
    let pilot_depth = length - through;
    if pilot_depth < 5.0 - FIT {
        return Err(format!(
            "a {length:.3} mm screw does not reach through the {through:.3} mm first board; \
             give a longer screw, or take the thinner board as the first one"
        ));
    }
    if pilot_depth > second.size(n) - 3.0 + FIT {
        return Err(format!(
            "a {length:.3} mm screw would come through the second board; give a length of at most {:.3} mm",
            through + second.size(n) - 3.0
        ));
    }
    let count = match o.count {
        Some(0) => return Err("count must be at least 1".into()),
        Some(k) if k > MAX => return Err(too_many(k, MAX, "screws")),
        Some(k) => k,
        None if long < 60.0 => 1,
        None => ((long / 150.0).ceil().max(2.0) as u32).min(MAX),
    };
    let pitch = long / f64::from(count);
    if 0.5 * pitch < head - FIT {
        return Err(format!("{count} screws of {diameter:.3} mm do not fit in {long:.3} mm; give fewer"));
    }
    // 0.7 of the screw, to the nearest half millimetre.
    let pilot_diameter = ((0.7 * diameter * 2.0).round() / 2.0).max(0.5);
    // A 90 degree countersink, from the head's diameter down to the clearance hole.
    let sink_depth = if countersink { 0.5 * (head - clearance_diameter) } else { 0.0 };
    if sink_depth >= through - FIT {
        return Err("the first board is too thin for the head of this screw".into());
    }

    let mut plan = Plan {
        clearance: Vec::new(),
        sinks: Vec::new(),
        pilots: Vec::new(),
        diameter,
        length,
        clearance_diameter,
        pilot_diameter,
        pilot_depth,
        sink_depth,
    };
    let plane = c.plane();
    // The head is on the face of the first board away from the second one.
    let outside = plane - c.dir * through;
    for i in 0..count {
        let mut at = [0.0; 3];
        at[l] = c.lo[l] + pitch * (f64::from(i) + 0.5);
        at[m] = 0.5 * (c.lo[m] + c.hi[m]);
        let mut from_outside = at;
        from_outside[n] = outside - c.dir * OVERCUT;
        plan.clearance.push(Bore { base: from_outside, axis: n, dir: c.dir, diameter: clearance_diameter, height: through + 2.0 * OVERCUT });
        if countersink {
            // Started just outside the face and that much wider, so the slope stays at 45 degrees.
            plan.sinks.push(Sink {
                base: from_outside,
                axis: n,
                dir: c.dir,
                diameter: head + 2.0 * OVERCUT,
                top_diameter: clearance_diameter,
                height: sink_depth + OVERCUT,
            });
        }
        let mut from_contact = at;
        from_contact[n] = plane - c.dir * OVERCUT;
        plan.pilots.push(Bore { base: from_contact, axis: n, dir: c.dir, diameter: pilot_diameter, height: pilot_depth + OVERCUT });
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::bx;

    /// An 18 x 250 x 400 side screwed onto the end of a 300 x 250 x 18 shelf.
    fn side_and_shelf() -> (Aabb, Aabb) {
        (bx([0.0, 0.0, 0.0], [18.0, 250.0, 400.0]), bx([18.0, 0.0, 200.0], [300.0, 250.0, 18.0]))
    }

    #[test]
    fn planks_overlapping_in_one_plane_are_laid_one_on_the_other() {
        // Two 220 x 1360 x 22 planks lying flat, drawn in one plane, overlapping by 205 mm.
        let bottom = bx([843.0, 0.0, 0.0], [220.0, 1360.0, 22.0]);
        let top = bx([858.0, 0.0, 0.0], [220.0, 1360.0, 22.0]);
        assert_eq!(stack(&top, &bottom), Some([0.0, 0.0, 22.0]));
        // 68 mm apart, the top one above: it comes down onto the other.
        let low = bx([843.0, 0.0, -90.0], [220.0, 1360.0, 22.0]);
        assert_eq!(stack(&top, &low), Some([0.0, 0.0, -68.0]));
        // Already face on face, or a side on a shelf: left as they are.
        let on = bx([858.0, 0.0, 22.0], [220.0, 1360.0, 22.0]);
        assert_eq!(stack(&on, &bottom), None);
        let (side, shelf) = side_and_shelf();
        assert_eq!(stack(&side, &shelf), None);
        // Laid on it, 22 + 22 mm: the default screw is 40 mm, not 55, so it stays in the wood.
        let mut moved = top;
        moved.min[2] += 22.0;
        moved.max[2] += 22.0;
        assert_eq!(plan(moved, bottom, &Options::default()).unwrap().length, 40.0);
    }

    #[test]
    fn screws_never_go_the_long_way_through_a_board() {
        let bottom = bx([843.0, 0.0, 0.0], [220.0, 1360.0, 22.0]);
        let beside = bx([1063.0, 0.0, 0.0], [220.0, 1360.0, 22.0]);
        let err = plan(beside, bottom, &Options { length: Some(20.0), ..Options::default() }).unwrap_err();
        assert!(err.contains("the long way"), "{err}");
    }

    #[test]
    fn defaults_for_a_side_on_a_shelf_end() {
        let (side, shelf) = side_and_shelf();
        let p = plan(side, shelf, &Options::default()).unwrap();
        assert_eq!((p.diameter, p.length, p.clearance_diameter, p.pilot_diameter, p.pilot_depth, p.sink_depth), (4.0, 45.0, 4.5, 3.0, 27.0, 1.75));
        assert_eq!(p.clearance.len(), 2);
        // Through the side from its outer face, towards +X.
        assert_eq!(p.clearance[0], Bore { base: [-0.5, 62.5, 209.0], axis: 0, dir: 1.0, diameter: 4.5, height: 19.0 });
        // The pilot hole starts at the contact and goes 27 mm into the shelf.
        assert_eq!(p.pilots[1], Bore { base: [17.5, 187.5, 209.0], axis: 0, dir: 1.0, diameter: 3.0, height: 27.5 });
        assert_eq!(p.sinks[0].diameter, 9.0);
    }

    #[test]
    fn bad_screws_are_refused_with_a_reason() {
        let (side, shelf) = side_and_shelf();
        let err = |o: Options| plan(side, shelf, &o).unwrap_err();
        // The other way round, the screw would have to go 300 mm through the shelf, the long way.
        assert!(plan(shelf, side, &Options { length: Some(45.0), ..Options::default() }).unwrap_err().contains("the long way"));
        assert!(err(Options { length: Some(400.0), ..Options::default() }).contains("come through"));
        assert!(err(Options { diameter: Some(10.0), ..Options::default() }).contains("no wood beside"));
        // 15 heads of 8 mm in a 100 mm deep shelf: under 7 mm apart.
        let narrow = (bx([0.0, 0.0, 0.0], [18.0, 100.0, 400.0]), bx([18.0, 0.0, 200.0], [300.0, 100.0, 18.0]));
        assert!(plan(narrow.0, narrow.1, &Options { count: Some(MAX), ..Options::default() }).unwrap_err().contains("do not fit"));
        let apart = bx([30.0, 0.0, 200.0], [300.0, 250.0, 18.0]);
        assert!(plan(side, apart, &Options::default()).unwrap_err().contains("do not touch"));
    }

    #[test]
    fn a_6_m_joint_gets_15_screws_at_most() {
        let (a, b) = (bx([0.0, 0.0, 0.0], [18.0, 6000.0, 400.0]), bx([18.0, 0.0, 200.0], [300.0, 6000.0, 18.0]));
        // Worked out from the length: cut down to MAX.
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!(p.clearance.len() as u32, MAX);
        // Given above MAX: refused before anything is cut.
        assert!(plan(a, b, &Options { count: Some(MAX + 1), ..Options::default() }).unwrap_err().contains("or fewer"));
    }
}
