use super::{Bounds2, Builder, Color, Ctx, Drawing, Ink, Rgb, ink};

/// The text style of an entity (Standard defaults when missing).
pub(crate) fn text_style(d: &Drawing, name: &str) -> rustdraft_doc::TextStyle {
    d.text_style(name).cloned().unwrap_or_default()
}

/// The font a text style draws with: its font file (TrueType, or an SHX font found by name),
/// its big font and its vertical flag.
pub fn style_font(ts: &rustdraft_doc::TextStyle) -> rustdraft_fonts::TextFont {
    rustdraft_fonts::TextFont::resolve_style(&ts.font, &ts.big_font, ts.vertical)
}

/// Shape a TEXT / ATTRIB / ATTDEF with its style's font and generation flags.
pub fn place_text_entity(d: &Drawing, t: &rustdraft_doc::Text, value: &str) -> (rustdraft_fonts::Shaped, Bounds2) {
    let ts = text_style(d, &t.style);
    let font = style_font(&ts);
    let (h, v) = align(t.halign, t.valign);
    let p = rustdraft_fonts::TextParams {
        insert: t.insert.xy(),
        align_pt: t.align_pt.map(|p| p.xy()),
        height: t.height,
        rotation: t.rotation,
        width_factor: t.width_factor,
        oblique: t.oblique,
        h,
        v,
        backwards: ts.backwards,
        upside_down: ts.upside_down,
    };
    rustdraft_fonts::place(&font, value, &p)
}

/// Lay out an MTEXT with its style's font.
pub fn layout_mtext_entity(d: &Drawing, t: &rustdraft_doc::MText) -> rustdraft_fonts::MTextLayout {
    let ts = text_style(d, &t.style);
    let params = rustdraft_fonts::MTextParams {
        insert: t.insert.xy(),
        height: t.height,
        width: t.width,
        attach: t.attach,
        rotation: t.rotation,
        line_spacing: t.line_spacing,
        line_spacing_exact: t.line_spacing_exact,
        font: style_font(&ts),
        width_factor: ts.width_factor,
        oblique: ts.oblique,
        columns: t.columns.as_ref().map(|c| rustdraft_fonts::MTextColumnSpec {
            dynamic: c.kind == rustdraft_doc::MTextColumnType::Dynamic,
            count: usize::from(c.count.max(1)),
            gutter: c.gutter,
            height: c.height,
            heights: if c.kind == rustdraft_doc::MTextColumnType::Dynamic && !c.auto_height { c.heights.clone() } else { Vec::new() },
            reversed: c.flow_reversed,
        }),
    };
    rustdraft_fonts::layout_mtext_with(&t.contents, &params)
}

fn mtext_color(c: Option<rustdraft_fonts::MTextColor>, rgb: Ink) -> Ink {
    match c {
        Some(rustdraft_fonts::MTextColor::Aci(i)) => {
            u8::try_from(i).ok().filter(|i| *i > 0).map(|i| ink(Color::Index(i), Color::Index(7), Color::Index(7))).unwrap_or(rgb)
        }
        Some(rustdraft_fonts::MTextColor::Rgb(v)) => {
            Ink { rgb: Rgb(((v >> 16) & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, (v & 0xFF) as u8), aci7: false, aci: 0 }
        }
        None => rgb,
    }
}

pub(crate) fn mtext(b: &mut Builder, ctx: &Ctx, t: &rustdraft_doc::MText, rgb: Ink, lw: f32) {
    let l = layout_mtext_entity(ctx.d, t);
    for p in &l.pieces {
        b.shaped(ctx, mtext_color(p.color, rgb), lw, &p.shaped);
    }
}

/// A TEXT's justification as the fonts' alignment (used where text is placed outside a build).
pub fn text_align(h: rustdraft_doc::HAlign, v: rustdraft_doc::VAlign) -> (rustdraft_fonts::Align, rustdraft_fonts::VAlign) {
    align(h, v)
}

fn align(h: rustdraft_doc::HAlign, v: rustdraft_doc::VAlign) -> (rustdraft_fonts::Align, rustdraft_fonts::VAlign) {
    use rustdraft_fonts::{Align as A, VAlign as V};
    let ha = match h {
        rustdraft_doc::HAlign::Left => A::Left,
        rustdraft_doc::HAlign::Center => A::Center,
        rustdraft_doc::HAlign::Right => A::Right,
        rustdraft_doc::HAlign::Aligned => A::Aligned,
        rustdraft_doc::HAlign::Middle => A::Middle,
        rustdraft_doc::HAlign::Fit => A::Fit,
    };
    let va = match v {
        rustdraft_doc::VAlign::Baseline => V::Baseline,
        rustdraft_doc::VAlign::Bottom => V::Bottom,
        rustdraft_doc::VAlign::Middle => V::Middle,
        rustdraft_doc::VAlign::Top => V::Top,
    };
    (ha, va)
}
