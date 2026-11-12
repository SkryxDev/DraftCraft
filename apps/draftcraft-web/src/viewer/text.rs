//! The viewer's words, in English and Italian, picked from the browser's languages. The viewer
//! keeps its own small table instead of the full app's (`crates/ui-egui/src/i18n`): it lives on
//! the web branch only, and the full app's tables stay as they are on main.

/// Every string the viewer shows.
pub struct Words {
    pub app: &'static str,
    pub open: &'static str,
    pub open_hint: &'static str,
    pub sample: &'static str,
    pub read_only: &'static str,
    pub loading: &'static str,
    pub fit: &'static str,
    pub layers: &'static str,
    pub measure: &'static str,
    pub layouts: &'static str,
    pub more: &'static str,
    pub model: &'static str,
    pub search: &'static str,
    pub all_on: &'static str,
    pub all_off: &'static str,
    pub reset: &'static str,
    pub only: &'static str,
    pub objects: &'static str,
    pub no_layers: &'static str,
    pub distance: &'static str,
    pub area: &'static str,
    pub length: &'static str,
    pub angle: &'static str,
    pub perimeter: &'static str,
    pub points: &'static str,
    pub undo_point: &'static str,
    pub clear: &'static str,
    pub done: &'static str,
    pub tap_first: &'static str,
    pub tap_next: &'static str,
    pub drag_hint: &'static str,
    pub background: &'static str,
    pub dark: &'static str,
    pub light: &'static str,
    pub lineweights: &'static str,
    pub info: &'static str,
    pub file: &'static str,
    pub entities: &'static str,
    pub units: &'static str,
    pub size: &'static str,
    pub full_app: &'static str,
    pub full_app_hint: &'static str,
    pub find_text: &'static str,
    pub no_matches: &'static str,
    pub close: &'static str,
    pub on: &'static str,
    pub off: &'static str,
    pub truncated: &'static str,
    pub gpu_failed: &'static str,
}

pub const EN: Words = Words {
    app: "DraftCraft Viewer",
    open: "Open",
    open_hint: "Open a DWG or DXF drawing to view it. Pinch to zoom, drag to move, double-tap to zoom in.",
    sample: "Sample drawing",
    read_only: "Read only",
    loading: "Opening…",
    fit: "Fit",
    layers: "Layers",
    measure: "Measure",
    layouts: "Layouts",
    more: "More",
    model: "Model",
    search: "Search",
    all_on: "All on",
    all_off: "All off",
    reset: "As in file",
    only: "Only",
    objects: "objects",
    no_layers: "No layers match.",
    distance: "Distance",
    area: "Area",
    length: "Length",
    angle: "Angle",
    perimeter: "Perimeter",
    points: "points",
    undo_point: "Undo",
    clear: "Clear",
    done: "Done",
    tap_first: "Tap the first point",
    tap_next: "Tap the next point",
    drag_hint: "Drag with one finger to aim precisely; two fingers move the view.",
    background: "Background",
    dark: "Dark",
    light: "Light",
    lineweights: "Lineweights",
    info: "Drawing",
    file: "File",
    entities: "Objects",
    units: "Units",
    size: "Size",
    full_app: "Full app",
    full_app_hint: "Opens the complete DraftCraft (made for larger screens).",
    find_text: "Find text",
    no_matches: "No text matches.",
    close: "Close",
    on: "On",
    off: "Off",
    truncated: "Very large drawing: some block contents are not shown.",
    gpu_failed: "The graphics card stopped responding: reload the page.",
};

pub const IT: Words = Words {
    app: "DraftCraft Viewer",
    open: "Apri",
    open_hint: "Apri un disegno DWG o DXF per vederlo. Pizzica per lo zoom, trascina per spostarti, tocca due volte per ingrandire.",
    sample: "Disegno di esempio",
    read_only: "Sola lettura",
    loading: "Apertura…",
    fit: "Adatta",
    layers: "Layer",
    measure: "Misura",
    layouts: "Layout",
    more: "Altro",
    model: "Modello",
    search: "Cerca",
    all_on: "Tutti accesi",
    all_off: "Tutti spenti",
    reset: "Come nel file",
    only: "Solo",
    objects: "oggetti",
    no_layers: "Nessun layer trovato.",
    distance: "Distanza",
    area: "Area",
    length: "Lunghezza",
    angle: "Angolo",
    perimeter: "Perimetro",
    points: "punti",
    undo_point: "Annulla",
    clear: "Cancella",
    done: "Fine",
    tap_first: "Tocca il primo punto",
    tap_next: "Tocca il punto successivo",
    drag_hint: "Trascina con un dito per mirare con precisione; con due dita sposti la vista.",
    background: "Sfondo",
    dark: "Scuro",
    light: "Chiaro",
    lineweights: "Spessori di linea",
    info: "Disegno",
    file: "File",
    entities: "Oggetti",
    units: "Unità",
    size: "Dimensioni",
    full_app: "App completa",
    full_app_hint: "Apre DraftCraft completo (pensato per schermi grandi).",
    find_text: "Cerca testo",
    no_matches: "Nessun testo trovato.",
    close: "Chiudi",
    on: "Acceso",
    off: "Spento",
    truncated: "Disegno molto grande: alcuni contenuti dei blocchi non sono mostrati.",
    gpu_failed: "La scheda grafica ha smesso di rispondere: ricarica la pagina.",
};

/// The words for the browser's preferred languages (`navigator.languages`, best first): Italian
/// when Italian comes before English, English otherwise.
pub fn pick(languages: &[String]) -> &'static Words {
    for l in languages {
        let l = l.to_ascii_lowercase();
        if l == "it" || l.starts_with("it-") {
            return &IT;
        }
        if l == "en" || l.starts_with("en-") {
            return &EN;
        }
    }
    &EN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_first_known_language() {
        let langs = |l: &[&str]| l.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(pick(&langs(&["it-IT", "en"])).open, "Apri");
        assert_eq!(pick(&langs(&["de", "it"])).open, "Apri");
        assert_eq!(pick(&langs(&["en-GB", "it"])).open, "Open");
        assert_eq!(pick(&langs(&["fr"])).open, "Open");
        assert_eq!(pick(&[]).open, "Open");
        assert_eq!(pick(&langs(&["item"])).open, "Open");
    }
}
