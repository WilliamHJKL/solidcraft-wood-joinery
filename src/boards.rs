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

/// How far to slide `first` along `axis` so that `second` covers it there, or it covers `second`.
/// Zero when that is already so: a board that is in place does not move.
fn cover(first: &Aabb, second: &Aabb, axis: usize) -> f64 {
    let low = second.min[axis] - first.min[axis];
    let high = second.max[axis] - first.max[axis];
    // `low` and `high` have opposite signs, or one is zero, when one board covers the other.
    if low * high <= 0.0 {
        0.0
    } else if low.abs() < high.abs() {
        low
    } else {
        high
    }
}

/// How far to move `first` so that it lies against `second`, face on face.
///
/// It moves along its length when there is a gap that way, otherwise across the widest gap.
/// Boards that run into each other part along the axis where they overlap least. On the two other axes it moves only as far as it must
/// to land on `second`.
pub fn align_end(first: &Aabb, second: &Aabb) -> Result<[f64; 3], String> {
    measurable(first)?;
    measurable(second)?;
    let gap = |a: usize| (second.min[a] - first.max[a]).max(first.min[a] - second.max[a]);
    let by = |a: usize, b: usize, f: &dyn Fn(usize) -> f64| f(a).total_cmp(&f(b));
    let length = (0..3).max_by(|&a, &b| by(a, b, &|x| first.size(x))).unwrap_or(0);
    let widest = (0..3).max_by(|&a, &b| by(a, b, &gap)).unwrap_or(0);
    // Boards that run into each other part along the axis where they overlap least.
    let n = if gap(length) > TOUCH { length } else { widest };
    let mut shift = [0.0; 3];
    for a in 0..3 {
        shift[a] = if a != n {
            cover(first, second, a)
        } else if first.min[a] + first.max[a] >= second.min[a] + second.max[a] {
            second.max[a] - first.min[a]
        } else {
            second.min[a] - first.max[a]
        };
    }
    Ok(shift)
}

/// How a fastener of `length` is shared between two boards `first_depth` and `second_depth`
/// thick along it: half in each, unless a board is too thin. 3 mm of wood and the glue relief
/// stay under each hole. `Err` gives the longest fastener that fits.
pub fn share(length: f64, relief: f64, first_depth: f64, second_depth: f64) -> Result<(f64, f64), f64> {
    let most_first = first_depth - 3.0 - relief;
    let most_second = second_depth - 3.0 - relief;
    let mut in_first = (0.5 * length).min(most_first);
    let mut in_second = length - in_first;
    if in_second > most_second {
        in_second = most_second;
        in_first = length - in_second;
    }
    if in_first > most_first + FIT || in_first <= FIT || in_second <= FIT {
        return Err((most_first + most_second).max(0.0));
    }
    Ok((in_first, in_second))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_end_closes_the_gap_and_keeps_what_is_in_place() {
        let stile = bx([0.0, 0.0, 0.0], [35.0, 35.0, 830.0]);
        // In place already: nothing moves, the rail stays where it is on the stile.
        let rail = bx([35.0, 10.0, 400.0], [360.0, 18.0, 45.0]);
        assert_eq!(align_end(&rail, &stile).unwrap(), [0.0; 3]);
        // 10 mm away and hanging 5 mm off the front of the stile.
        let loose = bx([45.0, -5.0, 400.0], [360.0, 18.0, 45.0]);
        assert_eq!(align_end(&loose, &stile).unwrap(), [-10.0, 5.0, 0.0]);
        // On the other side, run 4 mm into the stile.
        let into = bx([-356.0, 0.0, 400.0], [360.0, 18.0, 45.0]);
        assert_eq!(align_end(&into, &stile).unwrap(), [-4.0, 0.0, 0.0]);
    }

    #[test]
    fn align_end_does_not_take_the_longest_side_for_the_length() {
        // A shelf deeper (400) than it is long (300), run 6 mm into an upright panel.
        let shelf = bx([12.0, 0.0, 200.0], [300.0, 400.0, 18.0]);
        let upright = bx([0.0, 0.0, 0.0], [18.0, 400.0, 600.0]);
        assert_eq!(align_end(&shelf, &upright).unwrap(), [6.0, 0.0, 0.0]);
    }

    #[test]
    fn align_end_joins_two_boards_edge_to_edge() {
        // Two boards of a panel, 20 mm apart across their width, one 30 mm further along.
        let a = bx([30.0, 120.0, 0.0], [600.0, 100.0, 18.0]);
        let b = bx([0.0, 0.0, 0.0], [600.0, 100.0, 18.0]);
        assert_eq!(align_end(&a, &b).unwrap(), [-30.0, -20.0, 0.0]);
    }
}
