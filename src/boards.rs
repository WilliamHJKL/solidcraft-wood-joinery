//! What the joints share: how two axis-aligned boards touch, and how a cut is opened.

use crate::joint::{Aabb, Block};

pub const FIT: f64 = 1e-6;
/// Two faces this close are in contact.
pub const TOUCH: f64 = 1e-2;
/// How far a cutting block runs past a face it opens, so no skin is left there.
pub const OVERCUT: f64 = 0.5;
/// How far apart, in mm, two boards may lie for a joint to bring them together by itself;
/// farther, only Align does.
pub const FARTHEST: f64 = 100.0;

/// SolidCraft stops a plugin call after 30 s, or at its 256th command. Each joint with a count
/// has a `MAX` that stays well inside both on a Raspberry Pi 5: a count given above it is
/// refused, and a count worked out from the length is cut down to it.
pub fn too_many(count: u32, max: u32, what: &str) -> String {
    format!("{count} {what} are more than the {max} one call can make before SolidCraft stops it; give {max} or fewer")
}

/// The axis along which a board is longest, or `None` when two of its sides are about equal (a
/// square panel has no length).
pub fn long_axis(b: &Aabb) -> Option<usize> {
    let mut axes = [0, 1, 2];
    axes.sort_by(|&x, &y| b.size(y).total_cmp(&b.size(x)));
    (b.size(axes[0]) > b.size(axes[1]) + FIT).then_some(axes[0])
}

/// The two sides of a contact, `(across, other)`, for joints whose sockets go right through the
/// second board (box joints, dovetails). A socket runs through that board along the contact's
/// axis and spans the whole contact along `other`: it must not span the board there too, or it
/// would cut the board in two. In a drawer corner `across` is the height; with the boards lying
/// flat, end against edge, it is their thickness. Of two sides that would do, the longer.
pub fn across(c: &Contact, second: &Aabb) -> (usize, usize) {
    let (p, q) = ((c.axis + 1) % 3, (c.axis + 2) % 3);
    let longer = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { [(p, q), (q, p)] } else { [(q, p), (p, q)] };
    longer.into_iter().find(|&(_, m)| c.hi[m] - c.lo[m] < second.size(m) - FIT).unwrap_or(longer[0])
}

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
///
/// Two boards already joined by tenons, fingers or tails no longer touch by their boxes: the
/// box of the first reaches into the second. They are then read as touching on the face of the
/// second that the first enters.
pub fn contact(a: &Aabb, b: &Aabb) -> Option<Contact> {
    touching(a, b).or_else(|| entering(a, b))
}

/// Where `a` enters `b` through one of its faces without coming out of the other side.
fn entering(a: &Aabb, b: &Aabb) -> Option<Contact> {
    let mut best: Option<(f64, Contact)> = None;
    for n in 0..3 {
        // From the low side: `a` starts outside `b` and ends inside it. Or the same from the high side.
        let (dir, plane, depth) = if a.min[n] < b.min[n] - FIT && a.max[n] > b.min[n] + TOUCH && a.max[n] <= b.max[n] + FIT {
            (1.0, b.min[n], a.max[n] - b.min[n])
        } else if a.max[n] > b.max[n] + FIT && a.min[n] < b.max[n] - TOUCH && a.min[n] >= b.min[n] - FIT {
            (-1.0, b.max[n], b.max[n] - a.min[n])
        } else {
            continue;
        };
        let mut lo = [plane; 3];
        let mut hi = [plane; 3];
        let mut shared = true;
        for x in [(n + 1) % 3, (n + 2) % 3] {
            lo[x] = a.min[x].max(b.min[x]);
            hi[x] = a.max[x].min(b.max[x]);
            shared &= hi[x] - lo[x] > FIT;
        }
        // Of several faces, the one `a` enters least is the joint.
        if shared && best.is_none_or(|(least, _)| depth < least) {
            best = Some((depth, Contact { axis: n, dir, lo, hi }));
        }
    }
    best.map(|(_, c)| c)
}

fn touching(a: &Aabb, b: &Aabb) -> Option<Contact> {
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
    for (a, s) in shift.iter_mut().enumerate() {
        *s = if a != n {
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
    fn boards_joined_by_fingers_still_have_a_contact() {
        // A drawer side whose fingers go through the 18 mm front: its box starts at X = 0.
        let side = bx([0.0, 0.0, 0.0], [318.0, 18.0, 100.0]);
        let front = bx([0.0, 0.0, 0.0], [18.0, 300.0, 100.0]);
        let c = contact(&side, &front).unwrap();
        // They are read as touching on the inner face of the front, X = 18.
        assert_eq!((c.axis, c.dir, c.plane()), (0, -1.0, 18.0));
        assert_eq!((c.lo[1], c.hi[1], c.lo[2], c.hi[2]), (0.0, 18.0, 0.0, 100.0));
        // Two boards that cross, as for a half lap, do not: each comes out of the other.
        let x = bx([0.0, 0.0, 0.0], [300.0, 45.0, 18.0]);
        let y = bx([100.0, -100.0, 0.0], [45.0, 300.0, 18.0]);
        assert!(contact(&x, &y).is_none());
    }

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
