//! What the joints share: how two axis-aligned boards touch, and how a cut is opened.

use crate::joint::{Aabb, Block};

pub const FIT: f64 = 1e-6;
/// Two faces this close are in contact.
pub const TOUCH: f64 = 1e-2;
/// How far a cutting block runs past a face it opens, so no skin is left there.
pub const OVERCUT: f64 = 0.5;

pub fn measurable(b: &Aabb) -> Result<(), String> {
    for a in 0..3 {
        if !(b.min[a].is_finite() && b.max[a].is_finite() && b.size(a) > FIT) {
            return Err("a body has no measurable size".into());
        }
    }
    Ok(())
}

/// Where a face of the first board lies on a face of the second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// The axis square to the two faces.
    pub axis: usize,
    /// +1 when the second board is on the high side of the first, -1 otherwise.
    pub dir: f64,
    /// The rectangle the two faces share. On `axis`, both bounds are the contact plane.
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

impl Contact {
    pub fn plane(&self) -> f64 {
        self.lo[self.axis]
    }
}

/// The largest rectangle where `a` and `b` touch, if they do.
pub fn contact(a: &Aabb, b: &Aabb) -> Option<Contact> {
    let mut best: Option<(f64, Contact)> = None;
    for n in 0..3 {
        let (dir, plane) = if (a.max[n] - b.min[n]).abs() <= TOUCH {
            (1.0, b.min[n])
        } else if (a.min[n] - b.max[n]).abs() <= TOUCH {
            (-1.0, b.max[n])
        } else {
            continue;
        };
        let mut lo = [plane; 3];
        let mut hi = [plane; 3];
        let mut area = 1.0;
        for x in [(n + 1) % 3, (n + 2) % 3] {
            lo[x] = a.min[x].max(b.min[x]);
            hi[x] = a.max[x].min(b.max[x]);
            area *= (hi[x] - lo[x]).max(0.0);
            if hi[x] - lo[x] <= FIT {
                area = 0.0;
            }
        }
        if area > 0.0 && best.is_none_or(|(most, _)| area > most) {
            best = Some((area, Contact { axis: n, dir, lo, hi }));
        }
    }
    best.map(|(_, c)| c)
}

/// The block that removes `lo..hi` from `target`. It runs past every face of the target that the
/// region reaches, so the cut is open there.
pub fn cut_block(target: &Aabb, lo: [f64; 3], hi: [f64; 3]) -> Block {
    let mut corner = [0.0; 3];
    let mut size = [0.0; 3];
    for a in 0..3 {
        let l = if lo[a] <= target.min[a] + FIT { lo[a] - OVERCUT } else { lo[a] };
        let h = if hi[a] >= target.max[a] - FIT { hi[a] + OVERCUT } else { hi[a] };
        corner[a] = l;
        size[a] = h - l;
    }
    Block { corner, size }
}

#[cfg(test)]
pub fn bx(min: [f64; 3], size: [f64; 3]) -> Aabb {
    Aabb { min, max: [min[0] + size[0], min[1] + size[1], min[2] + size[2]] }
}
