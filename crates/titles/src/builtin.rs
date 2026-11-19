//! Built-in designs made in code (theme-packs.md step M1c): the Banner Build Moves
//! (`https://claude.ai/artifact/XpSgpxrM6iDqzfqry8qVRv`) as EffectCraft compositions, with the
//! same controls a theme pack's `banner` design has, so a pack can replace them.
//!
//! The moves' timing is the mock's: each animated value is sampled once per frame from the
//! mock's own curves and keyed, so the comp plays the mock's eases exactly and renders the same
//! frame every time (no randomness at render time; the decode's glyphs come from a fixed seed).

use effectcraft_color::Label;
use effectcraft_keyframe::{Justify, Keyframe, ShapePath, TextDoc, Value};
use effectcraft_project::build::{self, Ids};
use effectcraft_project::essential::{EgControl, EgKind, EgLink, EssentialGraphics};
use effectcraft_project::props::PropGroup;
use effectcraft_project::{Comp, ItemId, ItemKind, Layer, LayerId, LayerSource, MaskMode, Project, Uid};
use effectcraft_time::{FrameRate, Tick};

use crate::{Design, DesignError};

/// The built-in designs' ids.
pub const LASER_TRACE: &str = "banner-laser-trace";
/// Stack: the banner drops in as pieces.
pub const STACK: &str = "banner-stack";

/// Every built-in design id.
pub const BUILTINS: [&str; 2] = [LASER_TRACE, STACK];

/// A built-in design by id.
pub fn builtin(id: &str) -> Option<Result<Design, DesignError>> {
    match id {
        LASER_TRACE => Some(laser_trace()),
        STACK => Some(stack()),
        _ => None,
    }
}

// ---- the stage (the mock's 960 × 540, doubled) ----------------------------------------------

const W: f64 = 1920.0;
const H: f64 = 1080.0;
const FPS: f64 = 30.0;
const CYCLE: f64 = 6.0;
const OUT: f64 = 4.4;

/// The banner: left, top, width, height, accent bar, text padding.
const BX: f64 = 128.0;
const BH: f64 = 236.0;
const BY: f64 = H - BH - 128.0;
const BW: f64 = 1100.0;
const BAR: f64 = 12.0;
const PAD: f64 = 60.0;

const GOLD: [f64; 4] = [0.910, 0.722, 0.290, 1.0];
const PLATE: [f64; 4] = [0.047, 0.055, 0.075, 0.9];
const WHITE: [f64; 4] = [1.0, 1.0, 1.0, 1.0];

/// Shape-layer space: comp coordinates relative to the comp's centre (the layer's anchor).
fn local(x: f64, y: f64) -> [f64; 2] {
    [x - W / 2.0, y - H / 2.0]
}

fn tick(secs: f64) -> Tick {
    Tick::from_seconds_f64(secs)
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}
fn ease_out3(p: f64) -> f64 {
    1.0 - (1.0 - p).powi(3)
}
fn ease_in2(p: f64) -> f64 {
    p * p
}
fn ease_in3(p: f64) -> f64 {
    p * p * p
}

/// Keys sampled once per frame from `t0` to `t1` (both included).
fn sampled(t0: f64, t1: f64, f: impl Fn(f64) -> Value) -> Vec<Keyframe> {
    let n = ((t1 - t0) * FPS).ceil().max(1.0) as usize;
    (0..=n).map(|i| t0 + (t1 - t0) * i as f64 / n as f64).map(|t| Keyframe::new(tick(t), f(t))).collect()
}

/// Keys from `(time, value)` pairs, each holding until the next.
fn held(points: &[(f64, f64)]) -> Vec<Keyframe> {
    points.iter().map(|&(t, v)| Keyframe::new(tick(t), Value::Scalar(v)).hold()).collect()
}

fn set_keys(group: &mut PropGroup, path: &str, keys: Vec<Keyframe>) {
    if let Some(p) = group.prop_mut(path) {
        p.keys = keys;
    }
}

fn uid(group: &PropGroup, path: &str) -> Option<Uid> {
    group.prop(path).map(|p| p.uid)
}

/// A deterministic stream of numbers in 0..1 (the mock's mulberry32).
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// The project, its comp and the empty comp value to build layers against.
fn stage(name: &str) -> (Project, ItemId, Comp) {
    let mut p = Project::default();
    let comp = Comp::new(W as u32, H as u32, FrameRate::FPS_30, tick(CYCLE));
    let cid = p.add_item(name, Label::Sandstone, None, ItemKind::Comp(comp.clone().into()));
    (p, cid, comp)
}

fn control(
    p: &mut Project,
    name: &str,
    layer: LayerId,
    prop: Uid,
    links: Vec<EgLink>,
    as_type: Option<effectcraft_project::essential::ControlType>,
) -> EgControl {
    EgControl { id: p.alloc(), name: name.into(), kind: EgKind::Property { layer, prop, links }, as_type }
}

/// A text layer at comp position `at` (the baseline's left end).
fn words(p: &mut Project, comp: &Comp, name: &str, doc: TextDoc, at: [f64; 2]) -> Layer {
    let mut l = build::layer(p, comp, name, LayerSource::Text, (comp.width, comp.height), None);
    if let Some(prop) = l.props.prop_mut("text/sourceText") {
        prop.value = Value::Text(Box::new(doc));
    }
    if let Some(prop) = l.props.prop_mut("transform/position") {
        prop.value = Value::Vec3([at[0], at[1], 0.0]);
    }
    l
}

/// Add a text animator to a text layer.
fn add_animator(l: &mut Layer, animator: PropGroup) {
    if let Some(a) = l.props.group_mut("text/animators") {
        a.children.push(animator.into());
    }
}

// ---- Laser Trace ------------------------------------------------------------------------------

const FLING: f64 = 0.26;
const TRACE: f64 = 0.3;
const TRAIL: f64 = 260.0 * 2.0;
const FAR: f64 = 1300.0 * 2.0;
const GLYPH_OFFSETS: f64 = 30.0;

struct Edge {
    a: [f64; 2],
    b: [f64; 2],
    p0: [f64; 2],
    start: f64,
    l1: f64,
    l2: f64,
}

fn edges() -> Vec<Edge> {
    let (tl, tr, br, bl) = ([BX, BY], [BX + BW, BY], [BX + BW, BY + BH], [BX, BY + BH]);
    let mut r = Rng(77);
    [(tl, tr), (br, bl), (bl, tl), (tr, br)]
        .into_iter()
        .enumerate()
        .map(|(i, (a, b))| {
            // Angled flings (the mock's default): from far off at a seeded angle.
            let ang = r.next() * std::f64::consts::TAU;
            let p0 = [a[0] + ang.cos() * FAR, a[1] + ang.sin() * FAR];
            let l1 = (a[0] - p0[0]).hypot(a[1] - p0[1]);
            let l2 = (b[0] - a[0]).hypot(b[1] - a[1]);
            Edge { a, b, p0, start: 0.2 + i as f64 * 0.14, l1, l2 }
        })
        .collect()
}

/// How far along P0 → A → B the beam's head is at `t` (`None` before it starts).
fn head_at(e: &Edge, t: f64) -> Option<f64> {
    let k = t - e.start;
    if k <= 0.0 {
        return None;
    }
    if k < FLING {
        return Some(e.l1 * ease_in2(k / FLING));
    }
    Some(e.l1 + e.l2 * ease_out3(clamp01((k - FLING) / TRACE)))
}

/// One beam's strokes: a white core over a wide accent glow.
fn beam_strokes(ids: &mut Ids, core: f64, accent_uids: &mut Vec<Uid>) -> Vec<PropGroup> {
    let mut glow = build::shape_stroke(ids, GOLD, core * 3.0);
    if let Some(p) = glow.prop_mut("opacity") {
        p.value = Value::Scalar(50.0);
    }
    if let Some(p) = glow.prop_mut("cap") {
        p.value = Value::Enum(1);
    }
    accent_uids.extend(uid(&glow, "color"));
    let mut white = build::shape_stroke(ids, WHITE, core);
    if let Some(p) = white.prop_mut("cap") {
        p.value = Value::Enum(1);
    }
    // The first item in a group draws on top: the white core over its glow.
    vec![white, glow]
}

/// Laser Trace: four beams fling in and trace the banner's edges (the traced edge stays as the
/// border), the fill wipes in, the words decode letter by letter; out, the words scramble away,
/// the fill wipes back and the border folds to the banner's centre.
pub fn laser_trace() -> Result<Design, DesignError> {
    let (mut p, cid, comp) = stage("Banner · Laser Trace");
    let es = edges();
    let traced = es.iter().map(|e| e.start + FLING + TRACE).fold(0.0, f64::max);
    let (fill0, fill1) = (traced + 0.05, traced + 0.5);
    let centre = [BX + BW / 2.0, BY + BH / 2.0];
    let (fold0, fold1) = (OUT + 0.45, OUT + 0.8);
    let fill_w = move |t: f64| {
        if t < OUT + 0.15 {
            BW * ease_out3(clamp01((t - fill0) / (fill1 - fill0)))
        } else {
            BW * (1.0 - ease_in2(clamp01((t - (OUT + 0.15)) / 0.3)))
        }
    };
    let mut accent: Vec<(LayerId, Uid)> = Vec::new();

    // Fill: the plate and the accent bar, sized by the wipe (shape layers have no masks).
    let mut plate = build::layer(&mut p, &comp, "Fill", LayerSource::Shape, (comp.width, comp.height), None);
    let mut next = p.next_id;
    let mut ids = Ids(&mut next);
    // Size and centre keys for a rectangle whose left edge stays at `left` and whose width is `w_of(t)`.
    let wipe_spans = [(fill0, fill1), (OUT + 0.15, OUT + 0.45)];
    let size_keys = |w_of: &dyn Fn(f64) -> f64| {
        wipe_spans
            .iter()
            .flat_map(|&(t0, t1)| sampled(t0, t1, |t| Value::Vec2([w_of(t).max(0.0), BH])))
            .collect::<Vec<_>>()
    };
    let pos_keys = |w_of: &dyn Fn(f64) -> f64, left: f64| {
        wipe_spans
            .iter()
            .flat_map(|&(t0, t1)| sampled(t0, t1, |t| Value::Vec2(local(left + w_of(t).max(0.0) / 2.0, BY + BH / 2.0))))
            .collect::<Vec<_>>()
    };
    let mut plate_rect = build::shape_rect(&mut ids, [0.0, BH], local(BX, BY + BH / 2.0), 0.0);
    set_keys(&mut plate_rect, "size", size_keys(&fill_w));
    set_keys(&mut plate_rect, "position", pos_keys(&fill_w, BX));
    let plate_fill = build::shape_fill(&mut ids, PLATE);
    let surface_uid = uid(&plate_fill, "color");
    let plate_group = build::shape_group(&mut ids, "Plate", vec![plate_rect, plate_fill]);
    let bar_w = move |t: f64| fill_w(t).min(BAR);
    let mut bar_rect = build::shape_rect(&mut ids, [0.0, BH], local(BX, BY + BH / 2.0), 0.0);
    set_keys(&mut bar_rect, "size", size_keys(&bar_w));
    set_keys(&mut bar_rect, "position", pos_keys(&bar_w, BX));
    let bar_fill = build::shape_fill(&mut ids, GOLD);
    let bar_uid = uid(&bar_fill, "color");
    let bar_group = build::shape_group(&mut ids, "Accent Bar", vec![bar_rect, bar_fill]);
    p.next_id = next;
    if let Some(c) = plate.props.sub_mut("contents") {
        c.children.push(bar_group.into());
        c.children.push(plate_group.into());
    }
    accent.extend(bar_uid.map(|u| (plate.id, u)));

    // Beams and border: one shape layer, folded to the banner's centre by its scale.
    let mut beams = build::layer(&mut p, &comp, "Beams", LayerSource::Shape, (comp.width, comp.height), None);
    let mut next = p.next_id;
    let mut ids = Ids(&mut next);
    let mut groups = Vec::new();
    let mut accent_uids = Vec::new();
    for (i, e) in es.iter().enumerate() {
        let total = e.l1 + e.l2;
        // The moving beam on P0 → A → B: a trail behind the head while it travels.
        let path = ShapePath::polygon(&[local(e.p0[0], e.p0[1]), local(e.a[0], e.a[1]), local(e.b[0], e.b[1])], false);
        let shape = build::shape_path(&mut ids, path);
        let mut trim = build::shape_trim(&mut ids, 0.0, 0.0, 0.0);
        let done = e.start + FLING + TRACE;
        let span = (e.start, done + 1.0 / FPS);
        let pct = |s: f64| 100.0 * (s / total).clamp(0.0, 1.0);
        let head = |t: f64| head_at(e, t).map_or(0.0, |s| if s >= total - 1e-6 { total } else { s });
        set_keys(&mut trim, "end", sampled(span.0, span.1, |t| Value::Scalar(pct(head(t)))));
        set_keys(
            &mut trim,
            "start",
            sampled(span.0, span.1, |t| {
                let s = head(t);
                Value::Scalar(if s >= total - 1e-6 { 100.0 } else { pct((s - TRAIL).max(0.0)) })
            }),
        );
        let mut items = vec![shape, trim];
        items.extend(beam_strokes(&mut ids, 4.0, &mut accent_uids));
        groups.push(build::shape_group(&mut ids, &format!("Beam {}", i + 1), items));

        // The border: the edge A → B as far as the head has come.
        let edge = ShapePath::polygon(&[local(e.a[0], e.a[1]), local(e.b[0], e.b[1])], false);
        let shape = build::shape_path(&mut ids, edge);
        let mut trim = build::shape_trim(&mut ids, 0.0, 0.0, 0.0);
        set_keys(
            &mut trim,
            "end",
            sampled(e.start + FLING, done, |t| Value::Scalar(100.0 * clamp01((head(t) - e.l1) / e.l2))),
        );
        let mut items = vec![shape, trim];
        items.extend(beam_strokes(&mut ids, 3.0, &mut accent_uids));
        groups.push(build::shape_group(&mut ids, &format!("Edge {}", i + 1), items));
    }
    // The wipe's leading edge: a vertical beam riding the fill's edge while it moves.
    let line = ShapePath::polygon(&[local(BX, BY), local(BX, BY + BH)], false);
    let shape = build::shape_path(&mut ids, line);
    let mut items = vec![shape];
    items.extend(beam_strokes(&mut ids, 3.0, &mut accent_uids));
    let mut wipe = build::shape_group(&mut ids, "Wipe Edge", items);
    let edge_keys = |t0: f64, t1: f64| sampled(t0, t1, |t| Value::Vec2([fill_w(t), 0.0]));
    let mut k = edge_keys(fill0, fill1);
    k.extend(edge_keys(OUT + 0.15, OUT + 0.45));
    set_keys(&mut wipe, "transform/position", k);
    let visible = |t: f64| {
        let w = fill_w(t);
        if w > 0.5 && w < BW - 0.5 { 80.0 } else { 0.0 }
    };
    let mut k = sampled(fill0 - 1.0 / FPS, fill1 + 1.0 / FPS, |t| Value::Scalar(visible(t)));
    k.extend(sampled(OUT + 0.15 - 1.0 / FPS, OUT + 0.45 + 1.0 / FPS, |t| Value::Scalar(visible(t))));
    for key in &mut k {
        *key = key.clone().hold();
    }
    set_keys(&mut wipe, "transform/opacity", k);
    groups.push(wipe);
    p.next_id = next;
    if let Some(c) = beams.props.sub_mut("contents") {
        for g in groups.into_iter().rev() {
            c.children.push(g.into());
        }
    }
    // Fold: scale about the banner's centre to nothing, then gone.
    let c_local = local(centre[0], centre[1]);
    if let Some(a) = beams.props.prop_mut("transform/anchor") {
        a.value = Value::Vec3([c_local[0], c_local[1], 0.0]);
    }
    if let Some(a) = beams.props.prop_mut("transform/position") {
        a.value = Value::Vec3([centre[0], centre[1], 0.0]);
    }
    set_keys(
        &mut beams.props,
        "transform/scale",
        sampled(fold0, fold1, |t| {
            let s = 100.0 * (1.0 - ease_in3(clamp01((t - fold0) / (fold1 - fold0))));
            Value::Vec3([s, s, 100.0])
        }),
    );
    set_keys(&mut beams.props, "transform/opacity", held(&[(0.0, 100.0), (fold1, 0.0)]));
    accent.extend(accent_uids.into_iter().map(|u| (beams.id, u)));

    // Words: the headline and the detail line, decoded letter by letter.
    let title_doc = TextDoc {
        text: "LOCATION".into(),
        font: "Inter".into(),
        style: "Bold".into(),
        size: 92.0,
        fill: [0.965, 0.973, 0.98, 1.0],
        tracking: 160.0,
        justify: Justify::Left,
        ..Default::default()
    };
    let detail_doc = TextDoc {
        text: "DETAIL · 00:00 HRS".into(),
        font: "Inter".into(),
        style: "SemiBold".into(),
        size: 30.0,
        fill: [0.965, 0.973, 0.98, 1.0],
        tracking: 220.0,
        justify: Justify::Left,
        ..Default::default()
    };
    let text_x = BX + BAR + PAD;
    let mut title = words(&mut p, &comp, "Headline", title_doc, [text_x, BY + 132.0]);
    let mut detail = words(&mut p, &comp, "Caption", detail_doc, [text_x, BY + 188.0]);
    let mut rng = Rng(2026);
    let mut decode = |p: &mut Project, l: &mut Layer, t0: f64, step: f64, chars: f64| {
        let mut next = p.next_id;
        let mut ids = Ids(&mut next);
        let props = build::text_anim_props(&mut ids, "characterOffset", false);
        let mut anim = build::text_animator(&mut ids, "Decode", props);
        p.next_id = next;
        let end = t0 + chars * step;
        // Locked letters run left to right: the selection (still scrambled) starts after them.
        let mut start = sampled(t0, end, |t| Value::Scalar(100.0 * clamp01(((t - t0) / step + 1.0).floor() / chars)));
        start.push(Keyframe::new(tick(OUT), Value::Scalar(100.0)).hold());
        start.push(Keyframe::new(tick(OUT + 1.0 / FPS), Value::Scalar(0.0)).hold());
        set_keys(&mut anim, "selectors/#1/start", start.into_iter().map(Keyframe::hold).collect());
        // A new glyph every frame while scrambled.
        let mut offsets = Vec::new();
        let mut t = t0 - 2.0 / FPS;
        while t <= end + 1.0 / FPS {
            offsets.push(Keyframe::new(tick(t), Value::Scalar((1.0 + rng.next() * GLYPH_OFFSETS).floor())).hold());
            t += 1.0 / FPS;
        }
        let mut t = OUT;
        while t <= OUT + 0.3 {
            offsets.push(Keyframe::new(tick(t), Value::Scalar((1.0 + rng.next() * GLYPH_OFFSETS).floor())).hold());
            t += 1.0 / FPS;
        }
        set_keys(&mut anim, "properties/characterOffset", offsets);
        add_animator(l, anim);
        set_keys(&mut l.props, "transform/opacity", held(&[(0.0, 0.0), (t0 - 0.05, 100.0), (OUT + 0.2, 0.0)]));
    };
    let title_chars = 8.0;
    decode(&mut p, &mut title, fill0 + 0.25, 0.045, title_chars);
    decode(&mut p, &mut detail, fill0 + 0.55, 0.025, 18.0);
    // The detail line in the accent: a Fill Color animator over every letter.
    let mut next = p.next_id;
    let mut ids = Ids(&mut next);
    let props = build::text_anim_props(&mut ids, "fillColor", false);
    let mut tint = build::text_animator(&mut ids, "Accent", props);
    p.next_id = next;
    if let Some(c) = tint.prop_mut("properties/fillColor") {
        c.value = Value::Color(GOLD);
    }
    let tint_uid = uid(&tint, "properties/fillColor");
    add_animator(&mut detail, tint);
    accent.extend(tint_uid.map(|u| (detail.id, u)));

    // Controls: what a pack's banner exposes.
    let headline = title.props.prop("text/sourceText").map(|p| p.uid);
    let caption = detail.props.prop("text/sourceText").map(|p| p.uid);
    let mut controls = Vec::new();
    if let Some(u) = headline {
        controls.push(control(&mut p, "Headline", title.id, u, vec![], None));
    }
    if let Some(u) = caption {
        controls.push(control(&mut p, "Caption", detail.id, u, vec![], None));
    }
    if let Some(&(layer, prop)) = accent.first() {
        let links = accent[1..].iter().map(|&(layer, prop)| EgLink { layer, prop }).collect();
        controls.push(control(&mut p, "Accent", layer, prop, links, None));
    }
    if let Some(u) = surface_uid {
        controls.push(control(&mut p, "Surface", plate.id, u, vec![], None));
    }
    if let Some(u) = headline {
        controls.push(control(
            &mut p,
            "Headline Font",
            title.id,
            u,
            vec![],
            Some(effectcraft_project::essential::ControlType::Font),
        ));
    }
    if let Some(c) = p.comp_mut(cid) {
        // Layer 0 is the top layer.
        for l in [beams, title, detail, plate] {
            c.layers.push(l);
        }
        c.essential = Some(EssentialGraphics { name: "Laser Trace".into(), controls });
    }
    Design::from_project(p, cid)
}

// ---- Stack ------------------------------------------------------------------------------------

const COLS: usize = 16;
const ROWS: usize = 4;
const SPREAD: f64 = 1.0;
const DROP: f64 = 0.42;
const BOUNCE: f64 = 10.0;
/// The words' rows (y spans in comp pixels, generous): pieces outside them carry no text copy.
const TITLE_SPAN: (f64, f64) = (BY + 132.0 - 76.0, BY + 132.0 + 24.0);
const DETAIL_SPAN: (f64, f64) = (BY + 188.0 - 28.0, BY + 188.0 + 10.0);

struct Piece {
    cells: Vec<(usize, usize)>,
    shift: f64,
    key: f64,
    tint: f64,
    start: f64,
}

/// The mock's pieces: the grid cut into 2–4-cell pieces, bottom row first, landing in order.
fn pieces() -> Vec<Piece> {
    let mut r = Rng(1987 + (COLS * 31 + ROWS) as u32);
    let mut owner = vec![usize::MAX; COLS * ROWS];
    let mut out: Vec<Piece> = Vec::new();
    for row in (0..ROWS).rev() {
        for c in 0..COLS {
            if owner[row * COLS + c] != usize::MAX {
                continue;
            }
            let id = out.len();
            let mut cells = vec![(c, row)];
            owner[row * COLS + c] = id;
            let target = 2 + (r.next() * 3.0).floor() as usize;
            while cells.len() < target {
                let mut cand = Vec::new();
                for &(cc, rr) in &cells {
                    for (dc, dr) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                        let (nc, nr) = (cc as i64 + dc, rr as i64 + dr);
                        if nc >= 0
                            && (nc as usize) < COLS
                            && nr >= 0
                            && (nr as usize) < ROWS
                            && owner[nr as usize * COLS + nc as usize] == usize::MAX
                        {
                            cand.push((nc as usize, nr as usize));
                        }
                    }
                }
                if cand.is_empty() {
                    break;
                }
                let pick = cand[((r.next() * cand.len() as f64).floor() as usize).min(cand.len() - 1)];
                owner[pick.1 * COLS + pick.0] = id;
                cells.push(pick);
            }
            let bottom = cells.iter().map(|c| c.1).max().unwrap_or(row);
            // JavaScript's Math.round: halves round up.
            let shift = (r.next() * 4.0 - 2.0 + 0.5).floor();
            let key = -(bottom as f64) + r.next() * 0.9;
            out.push(Piece { cells, shift, key, tint: r.next(), start: 0.0 });
        }
    }
    out.sort_by(|a, b| a.key.total_cmp(&b.key));
    let n = out.len();
    for (i, p) in out.iter_mut().enumerate() {
        p.start = 0.2 + if n > 1 { i as f64 / (n - 1) as f64 * SPREAD } else { 0.0 };
    }
    out
}

fn cell_rect(c: usize, r: usize) -> [f64; 4] {
    let (cw, ch) = (BW / COLS as f64, BH / ROWS as f64);
    [BX + c as f64 * cw, BY + r as f64 * ch, cw, ch]
}

/// A closed rectangle path, moved into a layer's space (`origin` = where its (0, 0) sits in the comp).
fn rect_path([x, y, w, h]: [f64; 4], origin: [f64; 2]) -> ShapePath {
    let (x, y) = (x - origin[0], y - origin[1]);
    ShapePath::polygon(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]], true)
}

/// What every copy of the banner adds to the controls.
#[derive(Default)]
struct Uids {
    headline: Vec<(LayerId, Uid)>,
    caption: Vec<(LayerId, Uid)>,
    accent: Vec<(LayerId, Uid)>,
    surface: Vec<(LayerId, Uid)>,
}

fn title_doc() -> TextDoc {
    TextDoc {
        text: "LOCATION".into(),
        font: "Inter".into(),
        style: "Bold".into(),
        size: 92.0,
        fill: [0.965, 0.973, 0.98, 1.0],
        tracking: 160.0,
        justify: Justify::Left,
        ..Default::default()
    }
}

fn detail_doc() -> TextDoc {
    TextDoc {
        text: "DETAIL · 00:00 HRS".into(),
        font: "Inter".into(),
        style: "SemiBold".into(),
        size: 30.0,
        fill: [0.965, 0.973, 0.98, 1.0],
        tracking: 220.0,
        justify: Justify::Left,
        ..Default::default()
    }
}

/// One copy of the whole banner (plate and bar, and the words whose rows `spans` touches),
/// masked to `rects`, parented to `parent`, shown from `in_t` to `out_t`.
#[allow(clippy::too_many_arguments)]
fn banner_copy(
    p: &mut Project,
    comp: &Comp,
    tag: &str,
    rects: &[[f64; 4]],
    parent: LayerId,
    (in_t, out_t): (f64, f64),
    seam: Option<&[Keyframe]>,
    uids: &mut Uids,
) -> Vec<Layer> {
    let (top, bottom) = rects.iter().fold((f64::MAX, f64::MIN), |(a, b), r| (a.min(r[1]), b.max(r[1] + r[3])));
    let touches = |span: (f64, f64)| top < span.1 && bottom > span.0;
    let mut layers = Vec::new();

    let mut plate = build::layer(p, comp, &format!("Plate {tag}"), LayerSource::Shape, (comp.width, comp.height), None);
    let mut next = p.next_id;
    let mut ids = Ids(&mut next);
    let plate_rect = build::shape_rect(&mut ids, [BW, BH], local(BX + BW / 2.0, BY + BH / 2.0), 0.0);
    let plate_fill = build::shape_fill(&mut ids, PLATE);
    let bar_rect = build::shape_rect(&mut ids, [BAR, BH], local(BX + BAR / 2.0, BY + BH / 2.0), 0.0);
    let bar_fill = build::shape_fill(&mut ids, GOLD);
    uids.surface.extend(uid(&plate_fill, "color").map(|u| (plate.id, u)));
    uids.accent.extend(uid(&bar_fill, "color").map(|u| (plate.id, u)));
    let plate_group = build::shape_group(&mut ids, "Plate", vec![plate_rect, plate_fill]);
    let bar_group = build::shape_group(&mut ids, "Accent Bar", vec![bar_rect, bar_fill]);
    p.next_id = next;
    if let Some(c) = plate.props.sub_mut("contents") {
        c.children.push(bar_group.into());
        c.children.push(plate_group.into());
    }
    layers.push((plate, [W / 2.0, H / 2.0]));

    let text_x = BX + BAR + PAD;
    if touches(TITLE_SPAN) {
        let at = [text_x, BY + 132.0];
        let title = words(p, comp, &format!("Headline {tag}"), title_doc(), at);
        uids.headline.extend(title.props.prop("text/sourceText").map(|pr| (title.id, pr.uid)));
        layers.push((title, at));
    }
    if touches(DETAIL_SPAN) {
        let at = [text_x, BY + 188.0];
        let mut detail = words(p, comp, &format!("Caption {tag}"), detail_doc(), at);
        let mut next = p.next_id;
        let mut ids = Ids(&mut next);
        let props = build::text_anim_props(&mut ids, "fillColor", false);
        let mut tint = build::text_animator(&mut ids, "Accent", props);
        p.next_id = next;
        if let Some(c) = tint.prop_mut("properties/fillColor") {
            c.value = Value::Color(GOLD);
        }
        let tint_uid = uid(&tint, "properties/fillColor");
        add_animator(&mut detail, tint);
        uids.accent.extend(tint_uid.map(|u| (detail.id, u)));
        uids.caption.extend(detail.props.prop("text/sourceText").map(|pr| (detail.id, pr.uid)));
        layers.push((detail, at));
    }

    layers
        .into_iter()
        .map(|(mut l, origin)| {
            let mut next = p.next_id;
            let mut ids = Ids(&mut next);
            let masks: Vec<PropGroup> = rects
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let mut m = build::mask(
                        &mut ids,
                        &format!("Cell {}", i + 1),
                        rect_path(*r, origin),
                        MaskMode::Add,
                        [255, 255, 255],
                    );
                    if let Some(k) = seam {
                        set_keys(&mut m, "expansion", k.to_vec());
                    }
                    m
                })
                .collect();
            p.next_id = next;
            if let Some(g) = l.props.sub_mut("masks") {
                g.children.extend(masks.into_iter().map(Into::into));
            }
            l.parent = Some(parent);
            l.in_point = tick(in_t);
            l.out_point = tick(out_t);
            l
        })
        .collect()
}

/// A white shape over `rects` (the landing flash and seam tint), its opacity keyed.
fn white_over(
    p: &mut Project,
    comp: &Comp,
    name: &str,
    rects: &[[f64; 4]],
    parent: LayerId,
    span: (f64, f64),
    opacity: Vec<Keyframe>,
) -> Layer {
    let mut l = build::layer(p, comp, name, LayerSource::Shape, (comp.width, comp.height), None);
    let mut next = p.next_id;
    let mut ids = Ids(&mut next);
    let mut items: Vec<PropGroup> = rects
        .iter()
        .map(|r| build::shape_rect(&mut ids, [r[2], r[3]], local(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0), 0.0))
        .collect();
    items.push(build::shape_fill(&mut ids, WHITE));
    let g = build::shape_group(&mut ids, "Cells", items);
    p.next_id = next;
    if let Some(c) = l.props.sub_mut("contents") {
        c.children.push(g.into());
    }
    set_keys(&mut l.props, "transform/opacity", opacity);
    l.parent = Some(parent);
    l.in_point = tick(span.0);
    l.out_point = tick(span.1);
    l
}

/// A null that moves its children: anchor and position keys in comp pixels.
fn mover(p: &mut Project, comp: &Comp, name: &str, anchor: [f64; 2]) -> Layer {
    let mut n = build::layer(p, comp, name, LayerSource::Null, (100, 100), None);
    if let Some(a) = n.props.prop_mut("transform/anchor") {
        a.value = Value::Vec3([anchor[0], anchor[1], 0.0]);
    }
    if let Some(a) = n.props.prop_mut("transform/position") {
        a.value = Value::Vec3([anchor[0], anchor[1], 0.0]);
    }
    n
}

/// Stack: the banner, words included, is cut into a grid of 2–4-cell pieces that drop in bottom
/// row first, each landing with a small bounce and flash; the seams close after the last one.
/// Out is a line clear: rows flash and collapse bottom to top, the rows above dropping into place.
pub fn stack() -> Result<Design, DesignError> {
    let (mut p, cid, comp) = stage("Banner · Stack");
    let ps = pieces();
    let (cw, ch) = (BW / COLS as f64, BH / ROWS as f64);
    let lock_all = 0.2 + SPREAD + DROP + 0.16;
    let drop_dist = BY + BH + 80.0;
    let settled = lock_all + 0.3 + 1.0 / FPS;
    let seam_of = move |t: f64| if t < lock_all { 1.0 } else { 1.0 - clamp01((t - lock_all) / 0.3) };
    // Mask expansion: −1 px while the seams show; +0.5 px once closed, so neighbouring cells'
    // antialiased edges overlap instead of leaving hairlines.
    let seam_keys = sampled(lock_all, lock_all + 0.3, |t| Value::Scalar(-seam_of(t) * 1.5 + 0.5));
    let seam_keys: Vec<Keyframe> =
        std::iter::once(Keyframe::new(tick(0.0), Value::Scalar(-1.0))).chain(seam_keys).collect();
    let mut uids = Uids::default();
    // Each entry lists one piece's (or row's) layers top to bottom.
    let mut groups: Vec<Vec<Layer>> = Vec::new();

    for (i, pc) in ps.iter().enumerate() {
        let rects: Vec<[f64; 4]> = pc.cells.iter().map(|&(c, r)| cell_rect(c, r)).collect();
        let mut null = mover(&mut p, &comp, &format!("Piece {}", i + 1), [0.0, 0.0]);
        let (start, shift) = (pc.start, pc.shift);
        let offset = move |t: f64| {
            let q = clamp01((t - start) / DROP);
            let mut dy = -drop_dist * (1.0 - ease_in2(q));
            let dx = shift * cw * (1.0 - ease_out3(q));
            let k = t - (start + DROP);
            if q >= 1.0 && k < 0.16 {
                dy -= BOUNCE * (std::f64::consts::PI * k / 0.16).sin() * (1.0 - k / 0.16);
            }
            Value::Vec3([dx, dy, 0.0])
        };
        set_keys(&mut null.props, "transform/position", sampled(start, start + DROP + 0.16, offset));
        // Pieces give way to one whole banner once the seams close (`settled`): the hold draws
        // one copy, not one per piece.
        null.in_point = tick(start);
        null.out_point = tick(settled);
        let tint = pc.tint;
        let white = move |t: f64| {
            let k = t - (start + DROP);
            let flash = if (0.0..0.16).contains(&k) { 0.35 * (1.0 - k / 0.16) } else { 0.0 };
            Value::Scalar(100.0 * ((0.05 + 0.05 * tint) * seam_of(t) + flash).min(1.0))
        };
        let mut wk = sampled(start, start + DROP + 0.16, white);
        wk.extend(sampled(lock_all, lock_all + 0.3, white));
        wk.sort_by_key(|k| k.time);
        wk.dedup_by_key(|k| k.time);
        let copies = banner_copy(
            &mut p,
            &comp,
            &format!("{}", i + 1),
            &rects,
            null.id,
            (start, settled),
            Some(&seam_keys),
            &mut uids,
        );
        let over = white_over(&mut p, &comp, &format!("Flash {}", i + 1), &rects, null.id, (start, settled), wk);
        groups.push(std::iter::once(over).chain(copies.into_iter().rev()).chain(std::iter::once(null)).collect());
    }

    // The hold: the whole banner, still, from the moment the seams have closed until the clear.
    let still = mover(&mut p, &comp, "Banner", [0.0, 0.0]);
    let whole = [BX, BY, BW, BH];
    let closed = [Keyframe::new(tick(0.0), Value::Scalar(0.5))];
    let copies = banner_copy(&mut p, &comp, "hold", &[whole], still.id, (settled, OUT), Some(&closed), &mut uids);
    groups.push(copies.into_iter().rev().chain(std::iter::once(still)).collect());

    // Line clear: one band per row, bottom first; each squashes to the bottom of what's below it.
    let row_start = |r: usize| OUT + (ROWS - 1 - r) as f64 * 0.16;
    let height = move |r: usize, t: f64| ch * (1.0 - ease_in2(clamp01((t - (row_start(r) + 0.07)) / 0.14)));
    let bottom_of = move |r: usize, t: f64| BY + BH - ((r + 1)..ROWS).map(|below| height(below, t)).sum::<f64>();
    let end = row_start(0) + 0.21 + 1.0 / FPS;
    for r in 0..ROWS {
        let rect = [BX, BY + r as f64 * ch, BW, ch];
        let row_bottom = BY + (r + 1) as f64 * ch;
        let mut null = mover(&mut p, &comp, &format!("Row {}", r + 1), [0.0, row_bottom]);
        set_keys(
            &mut null.props,
            "transform/position",
            sampled(OUT, end, |t| Value::Vec3([0.0, bottom_of(r, t), 0.0])),
        );
        set_keys(
            &mut null.props,
            "transform/scale",
            sampled(OUT, end, |t| Value::Vec3([100.0, 100.0 * height(r, t) / ch, 100.0])),
        );
        let gone = row_start(r) + 0.21;
        null.in_point = tick(OUT);
        null.out_point = tick(gone);
        let s0 = row_start(r);
        let flash = move |t: f64| {
            let fl = if t >= s0 { clamp01(1.0 - ((t - (s0 + 0.07)) / 0.09).abs()) } else { 0.0 };
            Value::Scalar(90.0 * fl)
        };
        let over = white_over(
            &mut p,
            &comp,
            &format!("Clear {}", r + 1),
            &[rect],
            null.id,
            (OUT, gone),
            sampled(OUT, gone, flash),
        );
        let copies = banner_copy(
            &mut p,
            &comp,
            &format!("row {}", r + 1),
            &[rect],
            null.id,
            (OUT, gone),
            Some(&closed),
            &mut uids,
        );
        groups.push(std::iter::once(over).chain(copies.into_iter().rev()).chain(std::iter::once(null)).collect());
    }

    // Controls: the same as Laser Trace's, each driving every copy.
    let mut controls = Vec::new();
    let mut add = |p: &mut Project, name: &str, list: &[(LayerId, Uid)], as_type| {
        if let Some(&(layer, prop)) = list.first() {
            let links = list[1..].iter().map(|&(layer, prop)| EgLink { layer, prop }).collect();
            controls.push(control(p, name, layer, prop, links, as_type));
        }
    };
    add(&mut p, "Headline", &uids.headline, None);
    add(&mut p, "Caption", &uids.caption, None);
    add(&mut p, "Accent", &uids.accent, None);
    add(&mut p, "Surface", &uids.surface, None);
    add(&mut p, "Headline Font", &uids.headline, Some(effectcraft_project::essential::ControlType::Font));
    if let Some(c) = p.comp_mut(cid) {
        // Layer 0 is the top layer: later pieces (landing later) draw over earlier ones.
        for l in groups.into_iter().rev().flatten() {
            c.layers.push(l);
        }
        c.essential = Some(EssentialGraphics { name: "Stack".into(), controls });
    }
    Design::from_project(p, cid)
}
