//! A groove, dado or rabbet along one face of an axis-aligned board.
//!
//! Pure arithmetic on a bounding box, in millimetres.

use crate::joint::{Aabb, Block};

/// What the user may set. `None` means the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Across the face. Default: 6 mm.
    pub width: Option<f64>,
    /// Into the board. Default: 8 mm.
    pub depth: Option<f64>,
    /// From the edge of the face with the lower coordinate to the groove. Default: centred.
    pub offset: Option<f64>,
    /// Wood left uncut at the end with the lower coordinate. Default: none, the groove runs out.
    pub stop_start: Option<f64>,
    /// Wood left uncut at the other end.
    pub stop_end: Option<f64>,
}

/// A face of the bounding box: its axis (0 = X, 1 = Y, 2 = Z) and whether it is the high side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    pub axis: usize,
    pub high: bool,
}

impl Face {
    pub fn parse(s: &str) -> Option<Self> {
        let (high, axis) = match s {
            "+x" => (true, 0),
            "-x" => (false, 0),
            "+y" => (true, 1),
            "-y" => (false, 1),
            "+z" => (true, 2),
            "-z" => (false, 2),
            _ => return None,
        };
        Some(Self { axis, high })
    }
}

/// The block to cut, and the sizes used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub cut: Block,
    pub width: f64,
    pub depth: f64,
    pub length: f64,
}

const FIT: f64 = 1e-6;
/// How far the cutting block runs past a face it opens, so no skin is left there.
const OVERCUT: f64 = 0.5;

fn size(name: &str, v: f64, zero_ok: bool) -> Result<f64, String> {
    if v.is_finite() && (v > 0.0 || (zero_ok && v == 0.0)) {
        Ok(v)
    } else if zero_ok {
        Err(format!("{name} must be zero or a positive length"))
    } else {
        Err(format!("{name} must be a positive length"))
    }
}

/// Plan a groove in `face` of `board`, running along the longer side of that face.
pub fn plan(board: Aabb, face: Face, o: &Options) -> Result<Plan, String> {
    let extent = |a: usize| board.max[a] - board.min[a];
    for a in 0..3 {
        if !(board.min[a].is_finite() && board.max[a].is_finite() && extent(a) > FIT) {
            return Err("the body has no measurable size".into());
        }
    }
    let n = face.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    // `along` is the direction the groove runs, `across` the width of the face.
    let (along, across) = if extent(p) >= extent(q) { (p, q) } else { (q, p) };

    let width = size("width", o.width.unwrap_or(6.0), false)?;
    let depth = size("depth", o.depth.unwrap_or(8.0), false)?;
    if depth >= extent(n) - FIT {
        return Err(format!("depth ({depth:.3} mm) would cut through the {:.3} mm board", extent(n)));
    }
    if width > extent(across) + FIT {
        return Err(format!("width ({width:.3} mm) is more than the {:.3} mm face", extent(across)));
    }
    let offset = size("offset", o.offset.unwrap_or(0.5 * (extent(across) - width)), true)?;
    if offset + width > extent(across) + FIT {
        return Err(format!(
            "offset ({offset:.3} mm) puts the {width:.3} mm groove outside the {:.3} mm face",
            extent(across)
        ));
    }
    let stop_start = size("stop_start", o.stop_start.unwrap_or(0.0), true)?;
    let stop_end = size("stop_end", o.stop_end.unwrap_or(0.0), true)?;
    let length = extent(along) - stop_start - stop_end;
    if length <= FIT {
        return Err("the two stops leave no groove".into());
    }

    let mut corner = [0.0; 3];
    let mut dims = [0.0; 3];
    // Across: a groove that reaches an edge is a rabbet, cut past that edge.
    let lo = board.min[across] + offset;
    let hi = lo + width;
    let lo = if offset <= FIT { lo - OVERCUT } else { lo };
    let hi = if hi >= board.max[across] - FIT { hi + OVERCUT } else { hi };
    corner[across] = lo;
    dims[across] = hi - lo;
    // Along: an end with no stop is cut past the end of the board.
    let start = board.min[along] + stop_start - if stop_start <= FIT { OVERCUT } else { 0.0 };
    let end = board.max[along] - stop_end + if stop_end <= FIT { OVERCUT } else { 0.0 };
    corner[along] = start;
    dims[along] = end - start;
    // Into the board from the face, starting just outside it.
    corner[n] = if face.high { board.max[n] - depth } else { board.min[n] - OVERCUT };
    dims[n] = depth + OVERCUT;

    Ok(Plan { cut: Block { corner, size: dims }, width, depth, length })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 35 x 35 x 830 mm stile standing on the origin.
    fn stile() -> Aabb {
        Aabb { min: [0.0, 0.0, 0.0], max: [35.0, 35.0, 830.0] }
    }

    #[test]
    fn default_groove_is_centred_and_runs_out_both_ends() {
        let p = plan(stile(), Face { axis: 0, high: true }, &Options::default()).unwrap();
        assert_eq!((p.width, p.depth, p.length), (6.0, 8.0, 830.0));
        // In the +X face: 8 deep from X = 35, centred across Y, past both ends in Z.
        assert_eq!(p.cut, Block { corner: [27.0, 14.5, -0.5], size: [8.5, 6.0, 831.0] });
    }

    #[test]
    fn low_face_offset_and_stops() {
        let o = Options { offset: Some(6.0), stop_start: Some(80.0), stop_end: Some(0.0), ..Options::default() };
        let p = plan(stile(), Face { axis: 1, high: false }, &o).unwrap();
        assert_eq!(p.length, 750.0);
        // In the -Y face: X from 6 to 12, Z from 80 to past the top.
        assert_eq!(p.cut, Block { corner: [6.0, -0.5, 80.0], size: [6.0, 8.5, 750.5] });
    }

    #[test]
    fn a_groove_at_the_edge_is_a_rabbet() {
        let o = Options { offset: Some(0.0), width: Some(10.0), depth: Some(5.0), ..Options::default() };
        let p = plan(stile(), Face { axis: 0, high: true }, &o).unwrap();
        assert_eq!(p.cut.corner[1], -0.5);
        assert_eq!(p.cut.size[1], 10.5);
    }

    #[test]
    fn bad_grooves_are_refused_with_a_reason() {
        let err = |o: Options| plan(stile(), Face { axis: 0, high: true }, &o).unwrap_err();
        assert!(err(Options { depth: Some(35.0), ..Options::default() }).contains("cut through"));
        assert!(err(Options { width: Some(40.0), ..Options::default() }).contains("more than"));
        assert!(err(Options { offset: Some(30.0), ..Options::default() }).contains("outside"));
        assert!(err(Options { stop_start: Some(500.0), stop_end: Some(400.0), ..Options::default() }).contains("no groove"));
        assert!(err(Options { width: Some(-1.0), ..Options::default() }).contains("positive"));
        assert!(Face::parse("up").is_none());
    }
}
