//! Dowels across two boards that touch: matching holes in both, and the dowels themselves.

use crate::boards::{contact, measurable, share, FIT, OVERCUT};
use crate::joint::Aabb;

/// What the user may set. `None` means the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Default: the largest of 6, 8, 10 and 12 mm that is at most half the thinner side.
    pub diameter: Option<f64>,
    /// Length of one dowel. Default: five diameters.
    pub length: Option<f64>,
    /// Default: one every 96 mm or so, two as soon as there is room.
    pub count: Option<u32>,
    /// Extra depth at the bottom of each hole. Default: 1 mm.
    pub glue_relief: Option<f64>,
}

/// A cylinder: the centre of its base, the axis it runs along and its direction, and its size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bore {
    pub base: [f64; 3],
    pub axis: usize,
    pub dir: f64,
    pub diameter: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub holes_first: Vec<Bore>,
    pub holes_second: Vec<Bore>,
    pub dowels: Vec<Bore>,
    pub diameter: f64,
    pub length: f64,
    /// How much of the dowel is in each board.
    pub in_first: f64,
    pub in_second: f64,
}

/// Plan dowels across the faces where `first` and `second` touch.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let c = contact(&first, &second)
        .ok_or("the two boards do not touch; move one against the other, where the dowels go")?;
    let n = c.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    // The dowels stand in a row along the longer side of the contact, centred across it.
    let (l, m) = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { (p, q) } else { (q, p) };
    let (long, short) = (c.hi[l] - c.lo[l], c.hi[m] - c.lo[m]);

    let diameter = match o.diameter {
        Some(d) => d,
        None => [12.0, 10.0, 8.0, 6.0].into_iter().find(|d| *d <= 0.5 * short + FIT).unwrap_or(short / 3.0),
    };
    if !(diameter.is_finite() && diameter > FIT) {
        return Err("diameter must be a positive length".into());
    }
    if diameter > short - 2.0 + FIT {
        return Err(format!(
            "a {diameter:.3} mm dowel leaves no wood beside it: the boards touch over {short:.3} mm only"
        ));
    }
    let count = match o.count {
        Some(0) => return Err("count must be at least 1".into()),
        Some(k) => k,
        None => {
            let room = (long / (2.5 * diameter)).floor().max(1.0);
            room.min((long / 96.0).ceil().max(2.0)) as u32
        }
    };
    let pitch = long / f64::from(count);
    if 0.5 * pitch < diameter - FIT {
        return Err(format!("{count} dowels of {diameter:.3} mm do not fit in {long:.3} mm; give fewer or thinner ones"));
    }
    let length = o.length.unwrap_or(5.0 * diameter);
    if !(length.is_finite() && length > FIT) {
        return Err("length must be a positive length".into());
    }
    let relief = o.glue_relief.unwrap_or(1.0);
    if !(relief.is_finite() && relief >= 0.0) {
        return Err("glue_relief must be zero or a positive length".into());
    }

    let (in_first, in_second) = share(length, relief, first.size(n), second.size(n)).map_err(|most| {
        format!("a {length:.3} mm dowel is too long for these boards; give a length of at most {most:.3} mm")
    })?;

    let mut plan = Plan {
        holes_first: Vec::new(),
        holes_second: Vec::new(),
        dowels: Vec::new(),
        diameter,
        length,
        in_first,
        in_second,
    };
    for i in 0..count {
        let mut at = [0.0; 3];
        at[l] = c.lo[l] + pitch * (f64::from(i) + 0.5);
        at[m] = 0.5 * (c.lo[m] + c.hi[m]);
        let bore = |from: f64, dir: f64, height: f64| {
            let mut base = at;
            base[n] = c.plane() + from;
            Bore { base, axis: n, dir, diameter, height }
        };
        // Each hole starts just outside its board, so it is open on the contact face.
        plan.holes_first.push(bore(c.dir * OVERCUT, -c.dir, in_first + relief + OVERCUT));
        plan.holes_second.push(bore(-c.dir * OVERCUT, c.dir, in_second + relief + OVERCUT));
        plan.dowels.push(bore(-c.dir * in_first, c.dir, length));
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boards::bx;

    /// A 360 x 18 x 45 rail against the X = 35 face of a 35 x 35 x 830 stile.
    fn cabinet() -> (Aabb, Aabb) {
        (bx([35.0, 0.0, 400.0], [360.0, 18.0, 45.0]), bx([0.0, 0.0, 0.0], [35.0, 35.0, 830.0]))
    }

    #[test]
    fn defaults_for_a_rail_end() {
        let (rail, stile) = cabinet();
        let p = plan(rail, stile, &Options::default()).unwrap();
        assert_eq!((p.diameter, p.length, p.in_first, p.in_second), (8.0, 40.0, 20.0, 20.0));
        assert_eq!(p.dowels.len(), 2);
        // Into the rail, towards +X, from just inside the stile.
        assert_eq!(p.holes_first[0], Bore { base: [34.5, 9.0, 411.25], axis: 0, dir: 1.0, diameter: 8.0, height: 21.5 });
        // Into the stile, towards -X.
        assert_eq!(p.holes_second[1], Bore { base: [35.5, 9.0, 433.75], axis: 0, dir: -1.0, diameter: 8.0, height: 21.5 });
        // The dowel starts 20 mm inside the rail and points at the stile.
        assert_eq!(p.dowels[0], Bore { base: [55.0, 9.0, 411.25], axis: 0, dir: -1.0, diameter: 8.0, height: 40.0 });
    }

    #[test]
    fn a_long_edge_gets_a_dowel_every_96_mm_or_so() {
        // A 360 x 300 x 18 shelf standing on the edge of a side panel.
        let shelf = bx([18.0, 0.0, 0.0], [360.0, 300.0, 18.0]);
        let side = bx([0.0, 0.0, -100.0], [18.0, 300.0, 400.0]);
        let p = plan(shelf, side, &Options::default()).unwrap();
        assert_eq!((p.diameter, p.dowels.len()), (8.0, 4));
        // The side is 18 mm thick: 14 mm in it at most, so 26 mm in the shelf.
        assert_eq!((p.in_first, p.in_second), (26.0, 14.0));
        assert_eq!(p.dowels[0].base, [44.0, 37.5, 9.0]);
    }

    #[test]
    fn bad_dowels_are_refused_with_a_reason() {
        let (rail, stile) = cabinet();
        let err = |o: Options| plan(rail, stile, &o).unwrap_err();
        let apart = bx([45.0, 0.0, 400.0], [360.0, 18.0, 45.0]);
        assert!(plan(apart, stile, &Options::default()).unwrap_err().contains("do not touch"));
        assert!(err(Options { diameter: Some(17.0), ..Options::default() }).contains("no wood beside"));
        assert!(err(Options { count: Some(4), ..Options::default() }).contains("do not fit"));
        assert!(err(Options { length: Some(400.0), ..Options::default() }).contains("too long"));
        assert!(err(Options { count: Some(0), ..Options::default() }).contains("at least 1"));
    }
}
