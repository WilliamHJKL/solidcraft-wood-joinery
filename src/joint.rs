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
    pub fn size(&self, axis: usize) -> f64 {
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
    /// Two tenons side by side across the thickness of the rail, each in its own mortise.
    pub double: bool,
    pub position: Position,
    /// Pegs through the stile and the tenon: 0, 1 or 2.
    pub pegs: u32,
    /// Default: 6 mm, 8 mm when the tenon is 30 mm long or more.
    pub peg_diameter: Option<f64>,
    /// Draw-boring: the peg hole in the tenon sits this much nearer the shoulder than the hole in
    /// the stile, so the peg pulls the shoulder tight as it goes in. 0: a plain peg.
    pub drawbore: f64,
    /// Wedges driven into the end of a through tenon: 0, 1 or 2 per tenon.
    pub wedges: u32,
    /// Thickness of a wedge at the end of the tenon. Default: 4 mm.
    pub wedge_thickness: Option<f64>,
}

/// A wedge: the block it is cut from, and the two tools that slope its sides to a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wedge {
    pub blank: Block,
    pub sides: [crate::slant::Tool; 2],
}

/// Where the tenon sits across the thickness of the rail.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Position {
    #[default]
    Centre,
    /// Level with the face of the rail that has the lower coordinate: a bare-faced tenon.
    Low,
    /// Level with the other face.
    High,
}

impl Position {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "centre" => Some(Self::Centre),
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// A peg: a cylinder from `base` along axis `axis`, towards the higher coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Peg {
    pub base: [f64; 3],
    pub axis: usize,
    pub diameter: f64,
    pub height: f64,
    /// Where the hole in the tenon is, when it is draw-bored: `base` moved towards the shoulder.
    pub tenon_base: Option<[f64; 3]>,
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
    pub shoulder: f64,
    pub glue_relief: f64,
    /// Width and depth of the haunch, for the haunched variant.
    pub haunch: Option<(f64, f64)>,
    /// One per peg, through the whole stile.
    pub pegs: Vec<Peg>,
    /// Each wedge fills a slot of its own shape in the end of the tenon.
    pub wedges: Vec<Wedge>,
    /// What is made, but against common practice: a thin mortise cheek, a mortise across the
    /// grain of the stile.
    pub warnings: Vec<String>,
    /// Volume of one wedge, in mm3.
    pub wedge_volume: f64,
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
    // The joint is as wide as the two boards share: a rail wider than the stile, or one that
    // overhangs its end, gets a tenon over the part that lies on the stile.
    let (c_lo, c_hi) = (rail.min[w].max(stile.min[w]), rail.max[w].min(stile.max[w]));
    let (w_rail, e_rail) = (c_hi - c_lo, rail.size(e));
    let stile_depth = stile.size(a);

    // A double tenon: two tenons a fifth of the rail thick, with as much wood between them.
    let t = positive("thickness", o.thickness.unwrap_or(e_rail / if o.double { 5.0 } else { 3.0 }))?;
    if t >= e_rail - FIT {
        return Err(format!("thickness ({t:.3} mm) must be less than the rail thickness ({e_rail:.3} mm)"));
    }
    let off = o.offset.unwrap_or(0.0);
    // Half of what the tenons take across the rail.
    let half = if o.double { 1.5 * t } else { 0.5 * t };
    if o.position != Position::Centre && (o.double || off.abs() > FIT) {
        return Err("a tenon level with a face of the rail is single, and takes no offset".into());
    }
    if !off.is_finite() || off.abs() + half >= 0.5 * e_rail - FIT {
        return Err(format!("offset ({off:.3} mm) puts the tenon outside the {e_rail:.3} mm rail; reduce thickness or offset"));
    }
    let clr = not_negative("clearance", o.clearance.unwrap_or(0.0))?;
    let relief = not_negative("glue_relief", o.glue_relief.unwrap_or(if o.variant.blind() { 2.0 } else { 0.0 }))?;

    let length = match o.length {
        Some(l) => l,
        None if o.variant.blind() => {
            // Three quarters of the stile, to the nearest 5 mm, the mortise 2 mm deeper (glue
            // relief), leaving wood behind it.
            let most = (stile_depth - relief - 2.0).max(stile_depth * 0.5);
            ((stile_depth * 0.75 / 5.0).round() * 5.0).clamp(5.0_f64.min(most), most)
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
    let e_mid = match o.position {
        Position::Centre => 0.5 * (rail.min[e] + rail.max[e]) + off,
        Position::Low => rail.min[e] + 0.5 * t,
        Position::High => rail.max[e] - 0.5 * t,
    };
    // One range across the rail thickness per tenon, then the range they take together.
    let cheeks: Vec<(f64, f64)> = if o.double {
        vec![(e_mid - 1.5 * t, e_mid - 0.5 * t), (e_mid + 0.5 * t, e_mid + 1.5 * t)]
    } else {
        vec![(e_mid - 0.5 * t, e_mid + 0.5 * t)]
    };
    let (e_lo, e_hi) = (e_mid - half, e_mid + half);
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

    type Made = (Vec<Block>, Vec<Block>, f64, Option<(f64, f64)>, (f64, f64));
    let make = |e_lo: f64, e_hi: f64| -> Result<Made, String> {
        Ok(if o.variant == Variant::Haunched {
        let hw = positive("haunch_width", o.haunch_width.unwrap_or((w_rail * 0.25).max(t)))?;
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
            (c_lo + hw, c_hi - shoulder, c_lo, c_lo + hw)
        } else {
            (c_lo + shoulder, c_hi - hw, c_hi - hw, c_hi)
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
            Some((hw, hd)),
            (main_lo, main_hi),
        )
    } else {
        if 2.0 * shoulder >= w_rail - FIT {
            return Err("the two shoulders leave no tenon across the rail".into());
        }
        let (lo, hi) = (c_lo + shoulder, c_hi - shoulder);
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
            None,
            (lo, hi),
        )
        })
    };
    let (mut tenon, mut mortise) = (Vec::new(), Vec::new());
    let (mut width, mut haunch, mut across) = (0.0, None, (0.0, 0.0));
    for (lo, hi) in cheeks.iter().copied() {
        let made = make(lo, hi)?;
        tenon.extend(made.0);
        mortise.extend(made.1);
        (width, haunch, across) = (made.2, made.3, made.4);
    }

    // Wedges: in the end of a through tenon, three quarters of its length deep. The wood is not
    // spread: each wedge fills a slot of its own shape, so the volumes stay true.
    let mut wedges = Vec::new();
    let mut wedge_volume = 0.0;
    if o.wedges > 2 {
        return Err("wedges must be 0, 1 or 2".into());
    }
    if o.wedges > 0 {
        if o.variant != Variant::Through {
            return Err("wedges go in a through tenon; choose the through joint".into());
        }
        let thick = positive("wedge_thickness", o.wedge_thickness.unwrap_or(4.0))?;
        let (lo, hi) = across;
        let room = (hi - lo) / f64::from(o.wedges + 1);
        if thick > 0.5 * room + FIT {
            return Err(format!("{} wedges {thick:.3} mm thick leave too little tenon between them; give fewer or thinner ones", o.wedges));
        }
        let deep = 0.75 * length;
        let (end, tip) = (contact + dir * length, contact + dir * (length - deep));
        let run = (deep * deep + 0.25 * thick * thick).sqrt();
        for (e_lo, e_hi) in cheeks.iter().copied() {
            for i in 0..o.wedges {
                // One wedge in the middle, or two a fifth of the width in from each edge.
                let middle = if o.wedges == 1 { 0.5 * (lo + hi) } else { lo + (hi - lo) * (0.2 + 0.6 * f64::from(i)) };
                let mut corner = [0.0; 3];
                let mut size = [0.0; 3];
                corner[a] = end.min(tip);
                size[a] = deep;
                corner[w] = middle - 0.5 * thick;
                size[w] = thick;
                corner[e] = e_lo;
                size[e] = e_hi - e_lo;
                let mut point = [0.0; 3];
                point[a] = tip;
                point[w] = middle;
                let mut keep = point;
                keep[a] = contact + dir * (length - 0.25 * deep);
                let side = |sign: f64| {
                    let mut along = [0.0; 3];
                    along[w] = sign * 0.5 * thick / run;
                    along[a] = dir * deep / run;
                    crate::slant::beyond(keep, point, along, e, w, (e_lo, e_hi), deep + thick)
                };
                wedges.push(Wedge { blank: Block { corner, size }, sides: [side(-1.0), side(1.0)] });
            }
        }
        wedge_volume = 0.5 * thick * deep * t;
    }

    // Pegs: through the stile and the tenon. One sits half way along the tenon; two sit on a
    // diagonal, at a third and two thirds of it, so they do not split the stile along one line.
    let mut pegs = Vec::new();
    if o.pegs > 2 {
        return Err("pegs must be 0, 1 or 2".into());
    }
    if !(o.drawbore.is_finite() && o.drawbore >= 0.0) || (o.drawbore > FIT && o.pegs == 0) {
        return Err("drawbore must be 0 or more, and needs pegs".into());
    }
    if o.pegs > 0 {
        let diameter = positive("peg_diameter", o.peg_diameter.unwrap_or(if length >= 30.0 - FIT { 8.0 } else { 6.0 }))?;
        let reach = length.min(stile_depth);
        if diameter > reach - 6.0 + FIT || diameter * f64::from(o.pegs) > (across.1 - across.0) - 6.0 * f64::from(o.pegs) + FIT {
            return Err(format!("a {diameter:.3} mm peg leaves too little wood around it in this tenon; give a thinner peg"));
        }
        for i in 0..o.pegs {
            let mut base = [0.0; 3];
            let along = if o.pegs == 1 { 0.5 } else { (f64::from(i) + 1.0) / 3.0 };
            base[a] = contact + dir * along * reach;
            base[w] = across.0 + (across.1 - across.0) * (f64::from(i) + 0.5) / f64::from(o.pegs);
            base[e] = stile.min[e];
            let tenon_base = (o.drawbore > FIT).then(|| {
                let mut b = base;
                b[a] -= dir * o.drawbore;
                b
            });
            pegs.push(Peg { base, axis: e, diameter, height: stile.size(e), tenon_base });
        }
    }

    // Common practice, checked but not enforced: a mortise cheek of 8 mm at least, and a mortise
    // that runs along the grain of the stile (its length), whatever the rail.
    let mut warnings = Vec::new();
    let cheek = (e_lo - clr - stile.min[e]).min(stile.max[e] - e_hi - clr);
    if cheek < 8.0 - FIT {
        warnings.push(format!("the mortise leaves a {cheek:.1} mm cheek in the stile, under the usual 8 mm; offset the tenon or use a thinner one"));
    }
    if crate::boards::long_axis(&stile) == Some(e) {
        warnings.push("the mortise runs across the grain of the stile and may split it; a rail lying flat would rather have its tenon on edge, or dowels".into());
    }

    Ok(Plan {
        tenon,
        mortise,
        thickness: t,
        length,
        width,
        mortise_depth: if o.variant.blind() { length + relief } else { stile_depth },
        shoulder,
        glue_relief: if o.variant.blind() { relief } else { 0.0 },
        haunch,
        pegs,
        wedges,
        wedge_volume,
        warnings,
    })
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
    fn a_rail_wider_than_the_stile_gets_a_tenon_over_what_they_share() {
        // A 22 x 275 x 250 panel standing on a 350 x 250 x 22 one, overhanging 12.5 mm each side.
        let rail = bx([0.0, 12.5, 22.0], [22.0, 275.0, 250.0]);
        let stile = bx([0.0, 25.0, 0.0], [350.0, 250.0, 22.0]);
        let p = plan(rail, stile, &Options::default()).unwrap();
        let t = 22.0 / 3.0;
        assert!((p.thickness - t).abs() < 1e-9 && (p.shoulder - t).abs() < 1e-9);
        assert_eq!((p.length, p.mortise_depth), (15.0, 17.0));
        // The tenon runs over the 250 mm of the lower panel, a shoulder in from each of its ends.
        assert!((p.width - (250.0 - 2.0 * t)).abs() < 1e-9);
        assert!((p.tenon[0].corner[1] - (25.0 + t)).abs() < 1e-9);
    }

    #[test]
    fn a_double_tenon_is_two_tenons_a_fifth_of_the_rail_thick() {
        let (rail, stile) = cabinet(400.0);
        let p = plan(rail, stile, &Options { double: true, ..Options::default() }).unwrap();
        assert!((p.thickness - 3.6).abs() < 1e-9);
        assert_eq!((p.tenon.len(), p.mortise.len()), (2, 2));
        // Centred on the 18 mm rail, 3.6 mm apart: Y from 3.6 to 7.2 and from 10.8 to 14.4.
        assert!((p.tenon[0].corner[1] - 3.6).abs() < 1e-9 && (p.tenon[1].corner[1] - 10.8).abs() < 1e-9);
        assert!((p.tenon[1].size[1] - 3.6).abs() < 1e-9);
    }

    #[test]
    fn a_tenon_can_lie_level_with_a_face_of_the_rail() {
        // The rail stands 10 mm in from the face of the stile, so the mortise keeps a cheek.
        let rail = bx([35.0, 10.0, 400.0], [360.0, 18.0, 45.0]);
        let (_, stile) = cabinet(400.0);
        let low = plan(rail, stile, &Options { position: Position::Low, ..Options::default() }).unwrap();
        assert_eq!((low.tenon[0].corner[1], low.tenon[0].size[1]), (10.0, 6.0));
        let high = plan(rail, stile, &Options { position: Position::High, ..Options::default() }).unwrap();
        assert_eq!((high.tenon[0].corner[1], high.tenon[0].size[1]), (22.0, 6.0));
        let err = plan(rail, stile, &Options { position: Position::Low, double: true, ..Options::default() }).unwrap_err();
        assert!(err.contains("is single"));
    }

    #[test]
    fn a_drawbored_peg_hole_sits_nearer_the_shoulder_in_the_tenon() {
        let (rail, stile) = cabinet(400.0);
        let p = plan(rail, stile, &Options { pegs: 1, drawbore: 1.5, ..Options::default() }).unwrap();
        // The shoulder is at X = 35; the tenon runs towards X = 0. Its hole is 1.5 mm nearer X = 35.
        assert_eq!(p.pegs[0].tenon_base, Some([24.0, 0.0, 422.5]));
        assert!(plan(rail, stile, &Options { drawbore: 1.5, ..Options::default() }).unwrap_err().contains("needs pegs"));
    }

    #[test]
    fn thin_cheeks_and_mortises_across_the_grain_are_warned_of() {
        // The rail is flush with a face of the 35 mm stile: the cheek on that side is (18 - 6) / 2 = 6 mm.
        let (rail, stile) = cabinet(400.0);
        let p = plan(rail, stile, &Options::default()).unwrap();
        assert_eq!(p.warnings.len(), 1, "{:?}", p.warnings);
        assert!(p.warnings[0].contains("6.0 mm cheek"));
        // A rail lying flat: its tenon lies across the stile, so the mortise crosses the grain.
        let flat = bx([35.0, 0.0, 400.0], [360.0, 35.0, 18.0]);
        let p = plan(flat, stile, &Options::default()).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("across the grain")), "{:?}", p.warnings);
        // Centred in a 45 mm stile, an 18 mm rail leaves 19.5 mm cheeks, on edge: no warning.
        let wide = bx([0.0, -5.0, 0.0], [35.0, 45.0, 830.0]);
        assert!(plan(rail, wide, &Options::default()).unwrap().warnings.is_empty());
    }

    #[test]
    fn pegs_go_through_the_stile_half_way_along_the_tenon() {
        let (rail, stile) = cabinet(400.0);
        let p = plan(rail, stile, &Options { pegs: 1, ..Options::default() }).unwrap();
        // 6 mm, across the 35 mm stile along Y, 12.5 mm from the shoulder, in the middle of the tenon.
        assert_eq!(p.pegs, vec![Peg { base: [22.5, 0.0, 422.5], axis: 1, diameter: 6.0, height: 35.0, tenon_base: None }]);
        // Two pegs on a diagonal: a third and two thirds along the 25 mm tenon, low and high.
        let two = plan(rail, stile, &Options { pegs: 2, ..Options::default() }).unwrap();
        assert_eq!((two.pegs[0].base[2], two.pegs[1].base[2]), (414.25, 430.75));
        assert!((two.pegs[0].base[0] - (35.0 - 25.0 / 3.0)).abs() < 1e-9 && (two.pegs[1].base[0] - (35.0 - 50.0 / 3.0)).abs() < 1e-9);
        assert!(plan(rail, stile, &Options { pegs: 3, ..Options::default() }).unwrap_err().contains("0, 1 or 2"));
        assert!(plan(rail, stile, &Options { pegs: 1, peg_diameter: Some(20.0), ..Options::default() }).unwrap_err().contains("too little wood"));
    }

    #[test]
    fn wedges_go_in_the_end_of_a_through_tenon() {
        let (rail, stile) = cabinet(400.0);
        let through = Options { variant: Variant::Through, wedges: 2, ..Options::default() };
        let p = plan(rail, stile, &through).unwrap();
        // The tenon is 35 long and 33 wide: wedges 4 thick, 26.25 deep, a fifth in from each edge.
        assert_eq!(p.wedges.len(), 2);
        assert_eq!(p.wedge_volume, 0.5 * 4.0 * 26.25 * 6.0);
        assert_eq!(p.wedges[0].blank, Block { corner: [0.0, 6.0, 410.6], size: [26.25, 6.0, 4.0] });
        assert!((p.wedges[1].blank.corner[2] - 430.4).abs() < 1e-9);
        // Both sides of a wedge turn about its point, 26.25 mm from the end of the tenon.
        assert_eq!(p.wedges[0].sides[0].origin, p.wedges[0].sides[1].origin);
        assert!((p.wedges[0].sides[0].origin[0] - 26.25).abs() < 1e-9);
        let blind = plan(rail, stile, &Options { wedges: 1, ..Options::default() }).unwrap_err();
        assert!(blind.contains("through tenon"));
        assert!(plan(rail, stile, &Options { wedges: 3, ..through }).unwrap_err().contains("0, 1 or 2"));
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
        // Default haunch: 11.25 mm wide (a quarter of 45), 6 mm deep; main tenon 45 - 6 - 11.25 = 27.75 wide.
        assert_eq!(p.width, 27.75);
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
