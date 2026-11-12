//! The drawing being viewed: which layers show, which space (model or a layout) is on screen,
//! and the texts it can find. Nothing here changes the file: layer visibility is the viewer's
//! own, and "As in file" puts back what the file says.

use draftcraft_engine::doc::{Drawing, EntityKind, Space};
use draftcraft_engine::geom::Bounds2;

/// Most text matches listed.
pub const MAX_TEXT_HITS: usize = 200;
/// Most characters of a text shown in the match list.
const MAX_HIT_CHARS: usize = 80;

pub struct Doc {
    pub name: String,
    pub drawing: Drawing,
    /// Each layer's (on, frozen) as the file has it, by index in `drawing.layers`.
    original: Vec<(bool, bool)>,
    /// Top-level objects on each layer (model and paper space), by index in `drawing.layers`.
    pub counts: Vec<usize>,
    pub space: Space,
    /// Bumped on every change that needs a new display list.
    pub revision: u64,
}

impl Doc {
    pub fn new(name: String, drawing: Drawing) -> Doc {
        let original = drawing.layers.iter().map(|l| (l.on, l.frozen)).collect();
        let mut counts = vec![0usize; drawing.layers.len()];
        let index: std::collections::HashMap<String, usize> = drawing.layers.iter().enumerate().map(|(i, l)| (l.name.to_lowercase(), i)).collect();
        for space in all_spaces(&drawing) {
            if let Some(store) = drawing.space(&space) {
                for e in store.iter() {
                    if let Some(c) = index.get(&e.common.layer.to_lowercase()).and_then(|i| counts.get_mut(*i)) {
                        *c = c.saturating_add(1);
                    }
                }
            }
        }
        Doc { name, drawing, original, counts, space: Space::Model, revision: 1 }
    }

    /// Whether layer `i` shows (on and thawed).
    pub fn layer_visible(&self, i: usize) -> bool {
        self.drawing.layers.get(i).is_some_and(|l| l.on && !l.frozen)
    }

    /// Show or hide layer `i`. Showing a frozen layer thaws it.
    pub fn set_layer_visible(&mut self, i: usize, visible: bool) {
        if let Some(l) = self.drawing.layers.get_mut(i) {
            if visible {
                l.on = true;
                l.frozen = false;
            } else {
                l.on = false;
            }
            self.revision += 1;
        }
    }

    pub fn set_all(&mut self, visible: bool) {
        for i in 0..self.drawing.layers.len() {
            self.set_layer_visible(i, visible);
        }
    }

    /// Show layer `i` alone.
    pub fn isolate(&mut self, i: usize) {
        for j in 0..self.drawing.layers.len() {
            self.set_layer_visible(j, j == i);
        }
    }

    /// Every layer as the file has it.
    pub fn reset_layers(&mut self) {
        for (l, (on, frozen)) in self.drawing.layers.iter_mut().zip(&self.original) {
            l.on = *on;
            l.frozen = *frozen;
        }
        self.revision += 1;
    }

    /// How many layers show.
    pub fn visible_count(&self) -> usize {
        (0..self.drawing.layers.len()).filter(|i| self.layer_visible(*i)).count()
    }

    /// Model space and the layouts in tab order, with their names.
    pub fn spaces(&self) -> Vec<Space> {
        all_spaces(&self.drawing)
    }

    pub fn set_space(&mut self, space: Space) {
        if space != self.space {
            self.space = space;
            self.revision += 1;
        }
    }

    /// What zoom to fit shows: a layout's paper, or everything in model space.
    pub fn fit_bounds(&self) -> Bounds2 {
        match &self.space {
            Space::Paper(name) => {
                draftcraft_engine::render::sheet(&self.drawing, name).map(|s| s.bounds()).unwrap_or_else(|| self.drawing.extents(&self.space))
            }
            Space::Model => self.drawing.extents(&Space::Model),
        }
    }

    /// The texts in the current space containing `query` (any case): their words and where they
    /// are. Text inside blocks is not searched.
    pub fn find_text(&self, query: &str) -> Vec<(String, Bounds2)> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let Some(store) = self.drawing.space(&self.space) else { return Vec::new() };
        let mut hits = Vec::new();
        for e in store.iter() {
            let words = match &e.kind {
                EntityKind::Text(t) => t.value.clone(),
                EntityKind::MText(m) => plain_mtext(&m.contents),
                _ => continue,
            };
            if !words.to_lowercase().contains(&q) {
                continue;
            }
            let b = draftcraft_engine::doc::extent_bounds(&self.drawing, e);
            if b.is_empty() {
                continue;
            }
            hits.push((words.chars().take(MAX_HIT_CHARS).collect(), b));
            if hits.len() >= MAX_TEXT_HITS {
                break;
            }
        }
        hits
    }
}

/// Model space, then the layouts by tab order (a layout named Model is model space).
fn all_spaces(d: &Drawing) -> Vec<Space> {
    let mut layouts: Vec<_> = d.layouts.iter().filter(|l| !l.name.eq_ignore_ascii_case("model")).collect();
    layouts.sort_by_key(|l| l.tab_order);
    std::iter::once(Space::Model).chain(layouts.into_iter().map(|l| Space::Paper(l.name.clone()))).collect()
}

/// MTEXT contents without their formatting codes: paragraph breaks become spaces, `\X…;` codes
/// and grouping braces are left out, escaped characters kept.
pub fn plain_mtext(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' => {}
            '\\' => match chars.next() {
                Some('P' | 'p' | 'n' | 'N' | '~') => out.push(' '),
                Some(e @ ('\\' | '{' | '}')) => out.push(e),
                // `\U+00B2`: a character by its code point.
                Some('U' | 'u') if chars.peek() == Some(&'+') => {
                    chars.next();
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(ch);
                    }
                }
                // Codes with an argument up to `;` (font, height, colour, stacking…).
                Some('f' | 'F' | 'H' | 'h' | 'W' | 'w' | 'Q' | 'q' | 'T' | 't' | 'A' | 'a' | 'C' | 'c' | 'S' | 's') => {
                    for d in chars.by_ref() {
                        if d == ';' {
                            break;
                        }
                    }
                }
                // On/off codes (underline, overline, strike).
                Some(_) | None => {}
            },
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use draftcraft_engine::doc::Layer;

    fn doc() -> Doc {
        let mut d = Drawing::default();
        d.layers.push(Layer { name: "Walls".into(), ..Layer::default() });
        d.layers.push(Layer { name: "Frozen".into(), frozen: true, ..Layer::default() });
        Doc::new("t.dxf".into(), d)
    }

    /// Hiding, showing, isolating and putting layers back as in the file.
    #[test]
    fn layer_visibility() {
        let mut d = doc();
        let n = d.drawing.layers.len();
        let walls = d.drawing.layers.iter().position(|l| l.name == "Walls").unwrap_or(0);
        let frozen = d.drawing.layers.iter().position(|l| l.name == "Frozen").unwrap_or(0);
        assert!(!d.layer_visible(frozen));
        let rev = d.revision;
        d.set_layer_visible(frozen, true);
        assert!(d.layer_visible(frozen) && d.revision > rev);
        d.isolate(walls);
        assert_eq!(d.visible_count(), 1);
        assert!(d.layer_visible(walls));
        d.set_all(false);
        assert_eq!(d.visible_count(), 0);
        d.reset_layers();
        assert!(!d.layer_visible(frozen) && d.layer_visible(walls));
        assert_eq!(d.visible_count(), n - 1);
        // Out of range: nothing happens.
        d.set_layer_visible(10_000, false);
        assert!(!d.layer_visible(10_000));
    }

    #[test]
    fn mtext_codes_are_left_out() {
        assert_eq!(plain_mtext(r"{\fArial|b1;Room}\PKitchen \H2.5x;12 m\U+00B2"), "Room Kitchen 12 m²");
        assert_eq!(plain_mtext(r"\U+ZZZZ!"), "!");
        assert_eq!(plain_mtext(r"A\\B\{C\}"), r"A\B{C}");
        assert_eq!(plain_mtext(r"\S1/2;"), "");
        assert_eq!(plain_mtext("plain"), "plain");
        assert_eq!(plain_mtext("\\"), "");
    }

    #[test]
    fn model_space_comes_first() {
        let d = doc();
        assert_eq!(d.spaces().first(), Some(&Space::Model));
        assert!(d.find_text("  ").is_empty());
    }
}
