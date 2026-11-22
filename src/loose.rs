//! Loose tenons across two boards that touch: matching mortises in both, and the tenons.
//! Square-ended to sizes given by hand, or Festool Dominos: their sizes, rounded ends, and the
//! mortise widths of the machine.

use crate::boards::{contact, cut_block, measurable, share, FIT, OVERCUT, too_many};
use crate::dowel::Bore;
use crate::joint::{Aabb, Block};

/// A Festool Domino: its sizes, and what the machine that cuts it allows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Domino {
    pub thickness: f64,
    /// Across the joint, as Festool gives it: the mortise at its tight setting.
    pub width: f64,
    pub length: f64,
    /// The deepest mortise the machine cuts, in each board.
    pub max_depth: f64,
    /// How much wider than the domino the mortise is at the machine's middle and wide settings.
    pub medium: f64,
    pub wide: f64,
    pub machine: &'static str,
}

const fn df500(thickness: f64, length: f64, width: f64) -> Domino {
    Domino { thickness, width, length, max_depth: 28.0, medium: 6.0, wide: 10.0, machine: "DF 500" }
}

/// The DF 700 has two widths: tight, and 3 mm wider.
const fn df700(thickness: f64, length: f64, width: f64) -> Domino {
    Domino { thickness, width, length, max_depth: 70.0, medium: 3.0, wide: 3.0, machine: "DF 700" }
}

/// The beech Dominos Festool makes, by their names (thickness x length): widths from festool.de.
pub const DOMINOS: [(&str, Domino); 13] = [
    ("4x20", df500(4.0, 20.0, 16.6)),
    ("5x30", df500(5.0, 30.0, 18.8)),
    ("6x40", df500(6.0, 40.0, 19.8)),
    ("8x40", df500(8.0, 40.0, 21.9)),
    ("8x50", df500(8.0, 50.0, 21.9)),
    ("10x50", df500(10.0, 50.0, 23.9)),
    ("8x80", df700(8.0, 80.0, 21.9)),
    ("8x100", df700(8.0, 100.0, 21.9)),
    ("10x80", df700(10.0, 80.0, 23.9)),
    ("10x100", df700(10.0, 100.0, 23.9)),
    ("12x100", df700(12.0, 100.0, 25.9)),
    ("14x100", df700(14.0, 100.0, 27.9)),
    ("14x140", df700(14.0, 140.0, 27.9)),
];

pub fn domino(name: &str) -> Option<Domino> {
    DOMINOS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

/// The width setting of the second mortise; the first is always tight.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Fit {
    Tight,
    #[default]
    Medium,
    Wide,
}

/// A mortise or a tenon: a box, or a box with half round ends (a Domino), the ends being
/// cylinders along the joint.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Square(Block),
    Rounded { core: Block, ends: [Bore; 2] },
}

/// The most tenons one call makes: see `boards::too_many`.
pub const MAX: u32 = 40;

/// What the user may set. `None` means the default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    /// Default: the largest of 5, 6, 8 and 10 mm that is at most a third of the thinner side.
    pub thickness: Option<f64>,
    /// Across the joint. Default: its share of the joint, less a shoulder on each side.
    pub width: Option<f64>,
    /// Length of one tenon. Default: five thicknesses, rounded up to 10 mm.
    pub length: Option<f64>,
    /// Default: one, then one every 120 mm or so.
    pub count: Option<u32>,
    /// Extra depth at the bottom of each mortise. Default: 1 mm.
    pub glue_relief: Option<f64>,
    /// A Domino: its sizes replace thickness, width and length, and its ends are rounded.
    pub domino: Option<Domino>,
    /// The second mortise's width setting, for a Domino.
    pub fit: Fit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub mortises_first: Vec<Shape>,
    pub mortises_second: Vec<Shape>,
    pub tenons: Vec<Shape>,
    pub thickness: f64,
    pub width: f64,
    /// The second mortise across the joint: wider than the tenon at a Domino's loose settings.
    pub width_second: f64,
    pub length: f64,
    /// How much of the tenon is in each board.
    pub in_first: f64,
    pub in_second: f64,
}

/// Plan loose tenons across the faces where `first` and `second` touch.
pub fn plan(first: Aabb, second: Aabb, o: &Options) -> Result<Plan, String> {
    measurable(&first)?;
    measurable(&second)?;
    let c = contact(&first, &second)
        .ok_or("the two boards do not touch; move one against the other, where the tenons go")?;
    let n = c.axis;
    let (p, q) = ((n + 1) % 3, (n + 2) % 3);
    // The tenons stand in a row along the longer side of the contact, centred across it.
    let (l, m) = if c.hi[p] - c.lo[p] >= c.hi[q] - c.lo[q] { (p, q) } else { (q, p) };
    let (long, short) = (c.hi[l] - c.lo[l], c.hi[m] - c.lo[m]);

    let thickness = match (o.domino, o.thickness) {
        (Some(d), _) => d.thickness,
        (None, Some(t)) => t,
        (None, None) => [10.0, 8.0, 6.0, 5.0].into_iter().find(|t| *t <= short / 3.0 + FIT).unwrap_or(short / 3.0),
    };
    if !(thickness.is_finite() && thickness > FIT) {
        return Err("thickness must be a positive length".into());
    }
    // 3 mm of wood at least on each side of the mortise.
    if thickness > short - 6.0 + FIT {
        return Err(format!(
            "a {thickness:.3} mm tenon leaves no cheek beside it: the boards touch over {short:.3} mm only"
        ));
    }
    let count = match o.count {
        Some(0) => return Err("count must be at least 1".into()),
        Some(k) if k > MAX => return Err(too_many(k, MAX, "tenons")),
        Some(k) => k,
        None => ((long / 120.0).ceil().max(1.0) as u32).min(MAX),
    };
    let pitch = long / f64::from(count);
    let width = match (o.domino, o.width) {
        (Some(d), _) => d.width,
        (None, Some(w)) => w,
        (None, None) => pitch - 2.0 * (0.5 * (short - thickness)).min(0.2 * pitch),
    };
    let width_second = match (o.domino, o.fit) {
        (Some(d), Fit::Medium) => width + d.medium,
        (Some(d), Fit::Wide) => width + d.wide,
        _ => width,
    };
    if !(width.is_finite() && width > FIT) {
        return Err("width must be a positive length".into());
    }
    if width_second > pitch - 2.0 + FIT {
        let what = if width_second > width { format!("mortises {width_second:.3} mm wide") } else { format!("tenons {width:.3} mm wide") };
        return Err(format!("{count} {what} do not fit in {long:.3} mm; give fewer or narrower ones"));
    }
    let length = match o.domino {
        Some(d) => d.length,
        None => o.length.unwrap_or((5.0 * thickness / 10.0).ceil() * 10.0),
    };
    if !(length.is_finite() && length > FIT) {
        return Err("length must be a positive length".into());
    }
    let relief = o.glue_relief.unwrap_or(1.0);
    if !(relief.is_finite() && relief >= 0.0) {
        return Err("glue_relief must be zero or a positive length".into());
    }
    let (in_first, in_second) = share(length, relief, first.size(n), second.size(n)).map_err(|most| {
        match o.domino {
            Some(_) => format!("a {length:.0} mm Domino is too long for these boards: {most:.3} mm at most; take a shorter one"),
            None => format!("a {length:.3} mm tenon is too long for these boards; give a length of at most {most:.3} mm"),
        }
    })?;
    if let Some(d) = o.domino {
        let deepest = (in_first + relief).max(in_second + relief);
        if deepest > d.max_depth + FIT {
            return Err(format!(
                "this Domino needs a {deepest:.3} mm mortise, and the {} cuts {:.0} mm deep at most; take a shorter one",
                d.machine, d.max_depth
            ));
        }
    }

    let mut plan = Plan {
        mortises_first: Vec::new(),
        mortises_second: Vec::new(),
        tenons: Vec::new(),
        thickness,
        width,
        width_second,
        length,
        in_first,
        in_second,
    };
    let plane = c.plane();
    let rounded = o.domino.is_some();
    let middle = 0.5 * (c.lo[m] + c.hi[m]);
    for i in 0..count {
        let centre = c.lo[l] + pitch * (f64::from(i) + 0.5);
        // A box `across` wide, from `from` to `to` along the joint, counted from the contact
        // towards the second board.
        let span = |across: f64, from: f64, to: f64| {
            let mut lo = [0.0; 3];
            let mut hi = [0.0; 3];
            (lo[l], hi[l]) = (centre - 0.5 * across, centre + 0.5 * across);
            (lo[m], hi[m]) = (middle - 0.5 * thickness, middle + 0.5 * thickness);
            let (a, b) = (plane + c.dir * from, plane + c.dir * to);
            (lo[n], hi[n]) = (a.min(b), a.max(b));
            (lo, hi)
        };
        // A Domino's round ends: cylinders as thick as it, at both ends of its width, from the
        // contact face (`from`, past it by the overcut when `open`) into the board, `depth` long.
        let ends = |across: f64, start: f64, dir: f64, height: f64| {
            let half = 0.5 * (across - thickness);
            [-half, half].map(|off| {
                let mut base = [0.0; 3];
                base[l] = centre + off;
                base[m] = middle;
                base[n] = plane + c.dir * start;
                Bore { base, axis: n, dir: dir * c.dir, diameter: thickness, height }
            })
        };
        let mortise = |board: &Aabb, across: f64, from: f64, to: f64, start: f64, dir: f64, height: f64| {
            if rounded {
                let (lo, hi) = span(across - thickness, from, to);
                Shape::Rounded { core: cut_block(board, lo, hi), ends: ends(across, start, dir, height) }
            } else {
                let (lo, hi) = span(across, from, to);
                Shape::Square(cut_block(board, lo, hi))
            }
        };
        let deep_first = in_first + relief;
        let deep_second = in_second + relief;
        plan.mortises_first.push(mortise(&first, width, -deep_first, 0.0, OVERCUT, -1.0, deep_first + OVERCUT));
        plan.mortises_second.push(mortise(&second, width_second, 0.0, deep_second, -OVERCUT, 1.0, deep_second + OVERCUT));
        let block = |(lo, hi): ([f64; 3], [f64; 3])| Block { corner: lo, size: [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]] };
        plan.tenons.push(if rounded {
            Shape::Rounded { core: block(span(width - thickness, -in_first, in_second)), ends: ends(width, -in_first, 1.0, length) }
        } else {
            Shape::Square(block(span(width, -in_first, in_second)))
        });
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
        assert_eq!((p.thickness, p.width, p.length, p.in_first, p.in_second), (6.0, 33.0, 30.0, 15.0, 15.0));
        // In the rail: 16 mm deep from X = 35, open on the contact face.
        assert_eq!(p.mortises_first, vec![Shape::Square(Block { corner: [34.5, 6.0, 406.0], size: [16.5, 6.0, 33.0] })]);
        // In the stile: 16 mm deep the other way.
        assert_eq!(p.mortises_second, vec![Shape::Square(Block { corner: [19.0, 6.0, 406.0], size: [16.5, 6.0, 33.0] })]);
        // The tenon: 30 long, half in each board.
        assert_eq!(p.tenons, vec![Shape::Square(Block { corner: [20.0, 6.0, 406.0], size: [30.0, 6.0, 33.0] })]);
    }

    #[test]
    fn a_long_joint_gets_several_tenons() {
        // A 360 x 300 x 18 shelf against a side panel 18 mm thick.
        let shelf = bx([18.0, 0.0, 0.0], [360.0, 300.0, 18.0]);
        let side = bx([0.0, 0.0, -100.0], [18.0, 300.0, 400.0]);
        let p = plan(shelf, side, &Options { width: Some(20.0), ..Options::default() }).unwrap();
        assert_eq!((p.thickness, p.tenons.len()), (6.0, 3));
        // The side is 18 mm thick: 14 mm in it at most, so 16 mm in the shelf.
        assert_eq!((p.in_first, p.in_second), (16.0, 14.0));
        let square = |s: &Shape| match s {
            Shape::Square(b) => *b,
            Shape::Rounded { .. } => panic!("a square tenon"),
        };
        assert_eq!(square(&p.tenons[0]).corner, [4.0, 40.0, 6.0]);
        assert_eq!(square(&p.tenons[2]).size, [30.0, 20.0, 6.0]);
    }

    #[test]
    fn bad_loose_tenons_are_refused_with_a_reason() {
        let (rail, stile) = cabinet();
        let err = |o: Options| plan(rail, stile, &o).unwrap_err();
        let apart = bx([45.0, 0.0, 400.0], [360.0, 18.0, 45.0]);
        assert!(plan(apart, stile, &Options::default()).unwrap_err().contains("do not touch"));
        assert!(err(Options { thickness: Some(17.0), ..Options::default() }).contains("no cheek"));
        assert!(err(Options { width: Some(45.0), ..Options::default() }).contains("do not fit"));
        assert!(err(Options { length: Some(400.0), ..Options::default() }).contains("too long"));
        assert!(err(Options { count: Some(0), ..Options::default() }).contains("at least 1"));
    }

    #[test]
    fn a_6_m_joint_gets_40_tenons_at_most() {
        let (a, b) = (bx([0.0, 0.0, 0.0], [6000.0, 100.0, 18.0]), bx([0.0, 100.0, 0.0], [6000.0, 100.0, 18.0]));
        // Worked out from the length: cut down to MAX.
        let p = plan(a, b, &Options::default()).unwrap();
        assert_eq!(p.tenons.len() as u32, MAX);
        // Given above MAX: refused before anything is cut.
        assert!(plan(a, b, &Options { count: Some(MAX + 1), ..Options::default() }).unwrap_err().contains("or fewer"));
    }

    #[test]
    fn a_domino_takes_its_sizes_and_rounded_ends() {
        let (rail, stile) = cabinet();
        let d = domino("6x40").unwrap();
        let p = plan(rail, stile, &Options { domino: Some(d), ..Options::default() }).unwrap();
        // 6 x 19.8 x 40, 20 mm in each board; the second mortise at the middle setting, 6 mm wider.
        assert_eq!((p.thickness, p.width, p.length, p.in_first, p.in_second), (6.0, 19.8, 40.0, 20.0, 20.0));
        assert!((p.width_second - 25.8).abs() < 1e-9);
        let Shape::Rounded { core, ends } = &p.tenons[0] else { panic!("a rounded tenon") };
        // The straight middle is the width less the two half rounds; the ends run the whole length.
        assert!((core.size[2] - 13.8).abs() < 1e-9 && (core.size[0] - 40.0).abs() < 1e-9);
        assert!((ends[1].base[2] - ends[0].base[2] - 13.8).abs() < 1e-9);
        assert_eq!((ends[0].diameter, ends[0].height, ends[0].axis), (6.0, 40.0, 0));
        // The mortises: 21 mm deep, open past the contact face by the overcut.
        let Shape::Rounded { ends: second, .. } = &p.mortises_second[0] else { panic!("a rounded mortise") };
        assert!((second[0].height - 21.5).abs() < 1e-9 && (second[1].base[2] - second[0].base[2] - 19.8).abs() < 1e-9);
        // Tight: both mortises as wide as the domino.
        let p = plan(rail, stile, &Options { domino: Some(d), fit: Fit::Tight, ..Options::default() }).unwrap();
        assert_eq!(p.width_second, p.width);
    }

    #[test]
    fn a_domino_too_long_for_its_machine_or_the_boards_is_refused() {
        let (rail, stile) = cabinet();
        let with = |name: &str| Options { domino: domino(name), ..Options::default() };
        // 14 mm in an 18 mm rail end leaves 2 mm cheeks: too thin (10 mm, with 4 mm, goes).
        assert!(plan(rail, stile, &with("14x100")).unwrap_err().contains("no cheek"));
        assert!(plan(rail, stile, &with("10x50")).is_ok());
        // 8 x 80 into the 35 mm stile goes: 31 mm in the stile, 49 in the rail, the DF 700 cuts 70.
        assert!(plan(rail, stile, &with("8x80")).is_ok());
        // 8 x 50 against a 20 mm board: 16 mm in it, so 34 + 1 mm in the other, past the DF 500's 28.
        let (a, b) = (bx([0.0, 0.0, 0.0], [60.0, 100.0, 40.0]), bx([60.0, 0.0, 0.0], [20.0, 100.0, 40.0]));
        assert!(plan(a, b, &with("8x50")).unwrap_err().contains("DF 500 cuts 28 mm"));
        // 14 x 140 between two 40 mm boards: 36 mm in each at most.
        let (a, b) = (bx([0.0, 0.0, 0.0], [40.0, 200.0, 60.0]), bx([40.0, 0.0, 0.0], [40.0, 200.0, 60.0]));
        assert!(plan(a, b, &with("14x140")).unwrap_err().contains("Domino is too long"));
        assert!(domino("9x99").is_none());
    }
}
