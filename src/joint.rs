//! Mortise and tenon geometry between two axis-aligned boards.
//!
//! Pure arithmetic on bounding boxes: no CAD calls, so it is tested natively. All lengths are
//! millimetres.

/// Axis-aligned bounding box of a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb {
    fn size(&self, axis: usize) -> f64 {
        self.max[axis] - self.min[axis]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variant {
    /// Blind mortise, tenon with a shoulder on each edge.
    #[default]
    Classic,
    /// Blind mortise with a short haunch on the edge nearest the end of the stile.
    Haunched,
    /// Tenon through the whole stile.
    Through,
    /// Open slot at the end of the stile, tongue across the full rail width.
    Bridle,
}

impl Variant {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "classic" => Some(Self::Classic),
            "haunched" => Some(Self::Haunched),
            "through" => Some(Self::Through),
            "bridle" => Some(Self::Bridle),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Haunched => "haunched",
            Self::Through => "through",
            Self::Bridle => "bridle",
        }
    }

    fn blind(self) -> bool {
        matches!(self, Self::Classic | Self::Haunched)
    }
}

/// What the user may set. `None` means the cabinetmaking default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    pub variant: Variant,
    pub thickness: Option<f64>,
    pub length: Option<f64>,
    pub shoulder: Option<f64>,
    pub haunch_width: Option<f64>,
    pub haunch_depth: Option<f64>,
    pub glue_relief: Option<f64>,
    pub clearance: Option<f64>,
    pub offset: Option<f64>,
}

/// An axis-aligned block to add to the rail or remove from the stile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub corner: [f64; 3],
    pub size: [f64; 3],
}

/// The blocks that make one joint, and the sizes that were used.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub tenon: Vec<Block>,
    pub mortise: Vec<Block>,
    pub thickness: f64,
    pub length: f64,
    pub width: f64,
    pub mortise_depth: f64,
}

const TOUCH: f64 = 1e-2;
const FIT: f64 = 1e-6;
/// How far a cutting block runs past the face it enters, so no skin is left on that face.
const OVERCUT: f64 = 0.5;

fn positive(name: &str, v: f64) -> Result<f64, String> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(format!("{name} must be a positive length"))
    }
}

fn not_negative(name: &str, v: f64) -> Result<f64, String> {
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(format!("{name} must be zero or a positive length"))
    }
}

/// Plan a mortise and tenon between the end of `rail` and a face of `stile`.
pub fn plan(rail: Aabb, stile: Aabb, o: &Options) -> Result<Plan, String> {
    for b in [&rail, &stile] {
        for a in 0..3 {
            if !(b.min[a].is_finite() && b.max[a].is_finite() && b.size(a) > FIT) {
                return Err("a body has no measurable size".into());
            }
        }
    }

    // The joint axis: where an end of the rail lies on a face of the stile. When more than one
    // axis fits, the rail's longest one is its length.
    let mut joint: Option<(usize, f64, f64)> = None;
    for a in 0..3 {
        let (p, q) = ((a + 1) % 3, (a + 2) % 3);
        let overlaps =
            |x: usize| rail.min[x].max(stile.min[x]) < rail.max[x].min(stile.max[x]) - FIT;
        if !(overlaps(p) && overlaps(q)) {
            continue;
        }
        let dir = if (rail.max[a] - stile.min[a]).abs() <= TOUCH {
            1.0
        } else if (rail.min[a] - stile.max[a]).abs() <= TOUCH {
            -1.0
        } else {
            continue;
        };
        let contact = if dir > 0.0 { stile.min[a] } else { stile.max[a] };
        if joint.is_none_or(|(b, _, _)| rail.size(a) > rail.size(b)) {
            joint = Some((a, dir, contact));
        }
    }
    let (a, dir, contact) = joint.ok_or(
        "the end of the rail does not lie on a face of the stile; move the rail against the stile first",
    )?;

    // Across the rail end: `w` is its width (the larger side), `e` its thickness.
    let (p, q) = ((a + 1) % 3, (a + 2) % 3);
    let (w, e) = if rail.size(p) >= rail.size(q) { (p, q) } else { (q, p) };
    let (w_rail, e_rail) = (rail.size(w), rail.size(e));
    let stile_depth = stile.size(a);

    let t = positive("thickness", o.thickness.unwrap_or(e_rail / 3.0))?;
    if t >= e_rail - FIT {
        return Err(format!("thickness ({t:.3} mm) must be less than the rail thickness ({e_rail:.3} mm)"));
    }
    let off = o.offset.unwrap_or(0.0);
    if !off.is_finite() || off.abs() + 0.5 * t >= 0.5 * e_rail - FIT {
        return Err(format!("offset ({off:.3} mm) puts the {t:.3} mm tenon outside the {e_rail:.3} mm rail"));
    }
    let clr = not_negative("clearance", o.clearance.unwrap_or(0.0))?;
    let relief = not_negative("glue_relief", o.glue_relief.unwrap_or(if o.variant.blind() { 2.0 } else { 0.0 }))?;

    let length = match o.length {
        Some(l) => l,
        None if o.variant.blind() => {
            // Two thirds of the stile, to the nearest 5 mm, leaving wood behind the mortise.
            let most = (stile_depth - relief - 2.0).max(stile_depth * 0.5);
            ((stile_depth * 2.0 / 3.0 / 5.0).round() * 5.0).clamp(5.0_f64.min(most), most)
        }
        None => stile_depth,
    };
    let length = positive("length", length)?;
    if o.variant.blind() && length + relief >= stile_depth - FIT {
        return Err(format!(
            "the mortise ({:.3} mm with its glue relief) would come through the {stile_depth:.3} mm stile; \
             reduce length or glue_relief, or use the through variant",
            length + relief
        ));
    }

    let default_shoulder = if o.variant == Variant::Bridle { 0.0 } else { ((e_rail - t) * 0.5).min(w_rail * 0.2) };
    let shoulder = not_negative("shoulder", o.shoulder.unwrap_or(default_shoulder))?;

    // Thickness range of the tenon, and how far the stile runs past each edge of the rail.
    let e_mid = 0.5 * (rail.min[e] + rail.max[e]) + off;
    let (e_lo, e_hi) = (e_mid - 0.5 * t, e_mid + 0.5 * t);
    let gap_lo = rail.min[w] - stile.min[w];
    let gap_hi = stile.max[w] - rail.max[w];

    let lands = |lo: f64, hi: f64| -> Result<(), String> {
        if lo < stile.min[w] - FIT || hi > stile.max[w] + FIT || e_lo < stile.min[e] - FIT || e_hi > stile.max[e] + FIT {
            return Err("the tenon does not land on the stile; move the rail onto the stile".into());
        }
        if e_lo - clr <= stile.min[e] + FIT || e_hi + clr >= stile.max[e] - FIT {
            return Err("the mortise leaves no cheek in the stile; reduce thickness, clearance or offset".into());
        }
        Ok(())
    };

    let block = |from: f64, to: f64, w_lo: f64, w_hi: f64, e_lo: f64, e_hi: f64| {
        let (a0, a1) = (contact + dir * from, contact + dir * to);
        let mut corner = [0.0; 3];
        let mut size = [0.0; 3];
        corner[a] = a0.min(a1);
        size[a] = (a1 - a0).abs();
        corner[w] = w_lo;
        size[w] = w_hi - w_lo;
        corner[e] = e_lo;
        size[e] = e_hi - e_lo;
        Block { corner, size }
    };
    // The tenon starts a hair inside the rail, so the two join into one body.
    let back = -0.1_f64.min(e_rail * 0.05);
    let mortise_depth = if o.variant.blind() { length + relief } else { length.max(stile_depth) + OVERCUT };

    let (tenon, mortise, width) = if o.variant == Variant::Haunched {
        let hw = positive("haunch_width", o.haunch_width.unwrap_or((w_rail * 0.2).max(t)))?;
        let hd = positive("haunch_depth", o.haunch_depth.unwrap_or(t.min(length * 0.4)))?;
        if hd >= length - FIT {
            return Err(format!("haunch_depth ({hd:.3} mm) must be less than the tenon length ({length:.3} mm)"));
        }
        if shoulder + hw >= w_rail - FIT {
            return Err("the shoulder and the haunch leave no tenon across the rail".into());
        }
        // The haunch is on the rail edge nearest the end of the stile.
        let at_low = gap_lo < gap_hi;
        let (main_lo, main_hi, h_lo, h_hi) = if at_low {
            (rail.min[w] + hw, rail.max[w] - shoulder, rail.min[w], rail.min[w] + hw)
        } else {
            (rail.min[w] + shoulder, rail.max[w] - hw, rail.max[w] - hw, rail.max[w])
        };
        lands(main_lo.min(h_lo), main_hi.max(h_hi))?;
        // The haunch block laps a hair into the main tenon so the join leaves one body.
        let lap = 0.1_f64.min(hw * 0.1);
        let (hj_lo, hj_hi) = if at_low { (h_lo, h_hi + lap) } else { (h_lo - lap, h_hi) };
        (
            vec![block(back, length, main_lo, main_hi, e_lo, e_hi), block(back, hd, hj_lo, hj_hi, e_lo, e_hi)],
            vec![
                block(-OVERCUT, mortise_depth, main_lo - clr, main_hi + clr, e_lo - clr, e_hi + clr),
                block(-OVERCUT, hd + clr, h_lo - clr, h_hi + clr, e_lo - clr, e_hi + clr),
            ],
            main_hi - main_lo,
        )
    } else {
        if 2.0 * shoulder >= w_rail - FIT {
            return Err("the two shoulders leave no tenon across the rail".into());
        }
        let (lo, hi) = (rail.min[w] + shoulder, rail.max[w] - shoulder);
        lands(lo, hi)?;
        let (open_lo, open_hi) = (gap_lo <= FIT, gap_hi <= FIT);
        let bridle = o.variant == Variant::Bridle;
        if bridle {
            if shoulder > FIT {
                return Err("a bridle joint has no shoulder; leave shoulder out, or use the through variant".into());
            }
            if !(open_lo || open_hi) {
                return Err(format!(
                    "a bridle joint needs the rail flush with the end of the stile (the nearest end is {:.3} mm away); \
                     move the rail, or use the through variant",
                    gap_lo.min(gap_hi)
                ));
            }
        }
        // A bridle slot is cut past the open end of the stile.
        let m_lo = lo - clr - if bridle && open_lo { OVERCUT } else { 0.0 };
        let m_hi = hi + clr + if bridle && open_hi { OVERCUT } else { 0.0 };
        (
            vec![block(back, length, lo, hi, e_lo, e_hi)],
            vec![block(-OVERCUT, mortise_depth, m_lo, m_hi, e_lo - clr, e_hi + clr)],
            hi - lo,
        )
    };

    Ok(Plan { tenon, mortise, thickness: t, length, width, mortise_depth: if o.variant.blind() { length + relief } else { stile_depth } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(min: [f64; 3], size: [f64; 3]) -> Aabb {
        Aabb { min, max: [min[0] + size[0], min[1] + size[1], min[2] + size[2]] }
    }

    /// A 35 x 35 x 830 stile and a 360 x 18 x 45 rail against its X = 35 face, lower edge at `z`.
    fn cabinet(z: f64) -> (Aabb, Aabb) {
        (bx([35.0, 0.0, z], [360.0, 18.0, 45.0]), bx([0.0, 0.0, 0.0], [35.0, 35.0, 830.0]))
    }

    fn volume(blocks: &[Block]) -> f64 {
        blocks.iter().map(|b| b.size[0] * b.size[1] * b.size[2]).sum()
    }

    #[test]
    fn classic_defaults_follow_the_rule_of_thirds() {
        let (rail, stile) = cabinet(400.0);
        let p = plan(rail, stile, &Options::default()).unwrap();
        assert_eq!((p.thickness, p.length, p.width, p.mortise_depth), (6.0, 25.0, 33.0, 27.0));
        // Tenon: 6 x 33 x 25 into the stile (X from 10 to 35), plus the 0.1 mm lap into the rail.
        assert_eq!(p.tenon, vec![Block { corner: [10.0, 6.0, 406.0], size: [25.1, 6.0, 33.0] }]);
        // Mortise: 6 x 33 x 27 inside the stile (X from 8 to 35), plus the 0.5 mm overcut.
        assert_eq!(p.mortise, vec![Block { corner: [8.0, 6.0, 406.0], size: [27.5, 6.0, 33.0] }]);
    }

    #[test]
    fn a_rail_on_the_other_side_points_the_tenon_the_other_way() {
        let rail = bx([-360.0, 0.0, 400.0], [360.0, 18.0, 45.0]);
        let stile = bx([0.0, 0.0, 0.0], [35.0, 35.0, 830.0]);
        let p = plan(rail, stile, &Options::default()).unwrap();
        assert_eq!(p.mortise, vec![Block { corner: [-0.5, 6.0, 406.0], size: [27.5, 6.0, 33.0] }]);
    }

    #[test]
    fn the_haunch_goes_to_the_edge_nearest_the_stile_end() {
        // Rail 20 mm under the top of the stile: haunch on the upper edge.
        let (rail, stile) = cabinet(765.0);
        let o = Options { variant: Variant::Haunched, ..Options::default() };
        let p = plan(rail, stile, &o).unwrap();
        let haunch = p.tenon[1];
        assert!((haunch.corner[2] + haunch.size[2] - 810.0).abs() < 1e-9, "{haunch:?}");
        // Rail 20 mm above the foot of the stile: haunch on the lower edge.
        let (rail, stile) = cabinet(20.0);
        let p = plan(rail, stile, &o).unwrap();
        assert!((p.tenon[1].corner[2] - 20.0).abs() < 1e-9, "{:?}", p.tenon[1]);
        // Default haunch: 9 mm wide (a fifth of 45), 6 mm deep; main tenon 45 - 6 - 9 = 30 wide.
        assert_eq!(p.width, 30.0);
    }

    #[test]
    fn through_and_bridle_run_the_whole_stile() {
        let (rail, stile) = cabinet(785.0);
        let through = plan(rail, stile, &Options { variant: Variant::Through, ..Options::default() }).unwrap();
        assert_eq!((through.length, through.mortise_depth), (35.0, 35.0));
        assert!((volume(&through.tenon) - 6.0 * 33.0 * 35.1).abs() < 1e-6);
        let bridle = plan(rail, stile, &Options { variant: Variant::Bridle, ..Options::default() }).unwrap();
        assert_eq!(bridle.width, 45.0);
        // The slot runs 0.5 mm past the open top of the stile.
        let slot = bridle.mortise[0];
        assert!((slot.corner[2] + slot.size[2] - 830.5).abs() < 1e-9, "{slot:?}");
    }

    #[test]
    fn bad_joints_are_refused_with_a_reason() {
        let (rail, stile) = cabinet(400.0);
        let err = |o: Options| plan(rail, stile, &o).unwrap_err();
        assert!(err(Options { length: Some(34.0), ..Options::default() }).contains("come through"));
        assert!(err(Options { thickness: Some(18.0), ..Options::default() }).contains("less than the rail thickness"));
        assert!(err(Options { variant: Variant::Bridle, ..Options::default() }).contains("flush with the end"));
        assert!(err(Options { shoulder: Some(23.0), ..Options::default() }).contains("no tenon"));
        assert!(err(Options { thickness: Some(f64::NAN), ..Options::default() }).contains("positive"));
        // A rail 10 mm away from the stile.
        let apart = plan(bx([45.0, 0.0, 400.0], [360.0, 18.0, 45.0]), stile, &Options::default()).unwrap_err();
        assert!(apart.contains("does not lie on a face"), "{apart}");
        // A rail as thick as the stile: with its clearance, the mortise opens the stile's sides.
        let fat = plan(
            bx([35.0, 0.0, 400.0], [360.0, 35.0, 45.0]),
            stile,
            &Options { thickness: Some(34.9), clearance: Some(0.1), ..Options::default() },
        );
        assert!(fat.unwrap_err().contains("no cheek"));
    }
}
