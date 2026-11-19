//! Biscuits across two boards that touch: the slot a 100 mm blade leaves in each, and the biscuit.

use crate::boards::{contact, measurable, FIT, too_many};
use crate::dowel::Bore;
use crate::joint::Aabb;

/// The most biscuits one call makes: see `boards::too_many`.
pub const MAX: u32 = 30;

/// The blade of a biscuit joiner, and the thickness of the slot it cuts.
const BLADE: f64 = 100.0;
const KERF: f64 = 4.0;

/// What the user may set. `None` means the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// The size of the biscuit: 0, 10 or 20. Default: 20.
    pub size: Option<u32>,
    /// Default: one every 150 mm or so.
    pub count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// One disc per slot: the blade where it stops in each board.
    pub slots_first: Vec<Bore>,
    pub slots_second: Vec<Bore>,
    pub size: u32,
    /// How far the blade goes into each board.
    pub depth: f64,
    /// Length of the slot along the joint.
    pub slot_length: f64,
    /// Volume of one slot, in mm3. A biscuit is two of them.
    pub slot_volume: f64,
}

/// Plan biscuits across the faces where `first` and `second` touch.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let c = contact(&first, &second)
        .ok_or("the two boards do not touch; move one against the other, where the biscuits go")?;
    let n = c.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    // The biscuits lie in a row along the longer side of the contact, in the middle of the other.
    let (l, m) = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { (p, q) } else { (q, p) };
    let (long, short) = (c.hi[l] - c.lo[l], c.hi[m] - c.lo[m]);
    let size = o.size.unwrap_or(20);
    // Half the width of the biscuit, and a little room for glue.
    let depth = match size {
        0 => 8.0,
        10 => 10.0,
        20 => 12.0,
        _ => return Err("size must be 0, 10 or 20".into()),
    };
    if short < KERF + 6.0 - FIT {
        return Err(format!("the boards touch over {short:.3} mm only: too thin for a {KERF} mm biscuit slot"));
    }
    for (board, name) in [(&first, "first"), (&second, "second")] {
        if depth > board.size(n) - 3.0 + FIT {
            return Err(format!("the {name} board is too thin for a size {size} biscuit; give a smaller size"));
        }
    }
    let radius = 0.5 * BLADE;
    let slot_length = 2.0 * (2.0 * radius * depth - depth * depth).sqrt();
    let count = match o.count {
        Some(0) => return Err("count must be at least 1".into()),
        Some(k) if k > MAX => return Err(too_many(k, MAX, "biscuits")),
        Some(k) => k,
        None => ((long / 150.0).round().max(1.0) as u32).min(MAX),
    };
    let pitch = long / f64::from(count);
    if pitch < slot_length + 10.0 - FIT {
        return Err(format!(
            "{count} size {size} biscuits need {:.3} mm each and do not fit in {long:.3} mm; give fewer or a smaller size",
            slot_length + 10.0
        ));
    }
    let mut plan = Plan {
        slots_first: Vec::new(),
        slots_second: Vec::new(),
        size,
        depth,
        slot_length,
        // A segment of the blade's circle, as thick as the kerf.
        slot_volume: (radius * radius * ((radius - depth) / radius).acos() - (radius - depth) * 0.5 * slot_length) * KERF,
    };
    for i in 0..count {
        let disc = |side: f64| {
            // The blade's centre stays outside the board it cuts, on the other board's side.
            let mut base = [0.0; 3];
            base[l] = c.lo[l] + pitch * (f64::from(i) + 0.5);
            base[m] = 0.5 * (c.lo[m] + c.hi[m]) - 0.5 * KERF;
            base[n] = c.plane() + side * c.dir * (radius - depth);
            Bore { base, axis: m, dir: 1.0, diameter: BLADE, height: KERF }
        };
        plan.slots_first.push(disc(1.0));
        plan.slots_second.push(disc(-1.0));
    }
    Ok(plan)
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
    fn defaults_for_a_panel() {
        let (a, b) = panel();
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!((p.size, p.depth, p.slots_first.len()), (20, 12.0, 4));
        // A 100 mm blade 12 mm deep leaves a slot 65 mm long.
        assert!((p.slot_length - 2.0 * (1200.0_f64 - 144.0).sqrt()).abs() < 1e-9);
        // The first slot: the blade's centre is 38 mm past the edge, on the second board's side.
        assert_eq!(p.slots_first[0], Bore { base: [75.0, 138.0, 7.0], axis: 2, dir: 1.0, diameter: 100.0, height: 4.0 });
        assert_eq!(p.slots_second[0].base, [75.0, 62.0, 7.0]);
    }

    #[test]
    fn bad_biscuits_are_refused_with_a_reason() {
        let (a, b) = panel();
        let err = |o: Options| plan(a, b, &o).unwrap_err();
        assert!(err(Options { size: Some(15), ..Options::default() }).contains("0, 10 or 20"));
        assert!(err(Options { count: Some(12), ..Options::default() }).contains("do not fit"));
        let apart = bx([0.0, 120.0, 0.0], [600.0, 100.0, 18.0]);
        assert!(plan(a, apart, &Options::default()).unwrap_err().contains("do not touch"));
        let thin = bx([0.0, 100.0, 0.0], [600.0, 100.0, 8.0]);
        assert!(plan(thin, a, &Options::default()).is_err());
    }

    #[test]
    fn a_6_m_joint_gets_30_biscuits_at_most() {
        let (a, b) = (bx([0.0, 0.0, 0.0], [6000.0, 100.0, 18.0]), bx([0.0, 100.0, 0.0], [6000.0, 100.0, 18.0]));
        // Worked out from the length: cut down to MAX.
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!(p.slots_first.len() as u32, MAX);
        // Given above MAX: refused before anything is cut.
        assert!(plan(a, b, &Options { count: Some(MAX + 1), ..Options::default() }).unwrap_err().contains("or fewer"));
    }
}
