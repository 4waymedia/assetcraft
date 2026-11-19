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
use effectcraft_project::{Comp, ItemId, ItemKind, Layer, LayerId, LayerSource, Project, Uid};
use effectcraft_time::{FrameRate, Tick};

use crate::{Design, DesignError};

/// The built-in designs' ids.
pub const LASER_TRACE: &str = "banner-laser-trace";

/// Every built-in design id.
pub const BUILTINS: [&str; 1] = [LASER_TRACE];

/// A built-in design by id.
pub fn builtin(id: &str) -> Option<Result<Design, DesignError>> {
    match id {
        LASER_TRACE => Some(laser_trace()),
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
