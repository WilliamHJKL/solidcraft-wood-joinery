//! Cuts at an angle. A box along the axes cannot make one, so the cut is made by a tool box that
//! is built along the axes and then turned about a line.

use crate::boards::OVERCUT;
use crate::joint::Block;

/// The box that cuts: built along the axes, then turned by `angle` degrees about the line
/// through `origin` along `axis`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tool {
    pub block: Block,
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub angle: f64,
}

pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The tool that removes what lies beyond a plane.
///
/// The plane holds the line through `origin` along axis `u`, and the direction `w` (a unit
/// vector square to `u`). `keep` is any point on the side that stays. The tool covers `reach`
/// beyond the plane and `reach` each way along `w`, over `span` (low, high) along `u`. `e` is an
/// axis other than `u`: the tool is built with its cutting face square to it.
pub fn beyond(keep: [f64; 3], origin: [f64; 3], w: [f64; 3], u: usize, e: usize, span: (f64, f64), reach: f64) -> Tool {
    debug_assert!(u != e && u < 3 && e < 3, "beyond needs two different axes");
    let f = 3 - u - e;
    let mut axis = [0.0; 3];
    axis[u] = 1.0;
    let side = cross(axis, w);
    let mut to_keep = [keep[0] - origin[0], keep[1] - origin[1], keep[2] - origin[2]];
    to_keep[u] = 0.0;
    // `d` looks from the plane into what is removed.
    let d = if dot(side, to_keep) > 0.0 { [-side[0], -side[1], -side[2]] } else { side };
    let mut face = [0.0; 3];
    face[e] = 1.0;
    let angle = dot(axis, cross(face, d)).atan2(dot(face, d)).to_degrees();
    let mut corner = origin;
    let mut size = [0.0; 3];
    size[e] = reach;
    corner[f] = origin[f] - reach;
    size[f] = 2.0 * reach;
    corner[u] = span.0 - OVERCUT;
    size[u] = span.1 - span.0 + 2.0 * OVERCUT;
    let mut pivot = origin;
    pivot[u] = 0.0;
    Tool { block: Block { corner, size }, origin: pivot, axis, angle }
}
