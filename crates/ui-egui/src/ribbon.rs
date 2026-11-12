//! The Windows-style window top: the application bar (application button, Quick Access
//! Toolbar, workspace, title and search) and the ribbon — tabs (Home, Insert, Annotate, …) of
//! panels whose large, small and split buttons start the same commands as the menus and the
//! command line. Icons are DraftCraft's own (`icons.rs`); only the arrangement follows the
//! Windows workspace.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use serde_json::json;

use crate::CadApp;
use crate::icons::{self, Icon};

pub(crate) mod context;
use crate::theme::Tokens;

/// A menu entry: icon, command id, label.
pub(crate) type Tool = (Icon, &'static str, &'static str);

/// A ribbon button; `more` makes it a split button whose arrow opens those alternatives.
#[derive(Clone, Copy)]
pub(crate) struct Btn {
    pub icon: Icon,
    pub cmd: &'static str,
    pub label: &'static str,
    pub more: &'static [Tool],
}

const fn b(icon: Icon, cmd: &'static str, label: &'static str) -> Btn {
    Btn { icon, cmd, label, more: &[] }
}

const fn d(icon: Icon, cmd: &'static str, label: &'static str, more: &'static [Tool]) -> Btn {
    Btn { icon, cmd, label, more }
}

/// One column of a ribbon panel.
pub(crate) enum Col {
    /// A large button: big icon over its label.
    Large(Btn),
    /// Up to three rows of small buttons with labels (the labels go when space is short).
    Small(&'static [Btn]),
    /// Up to three rows of icon-only buttons.
    Icons(&'static [Btn]),
    /// The layer list over two rows of layer tools, each row ending in one labelled tool.
    Layers([&'static [Btn]; 2], [Btn; 2]),
    /// Current colour, linetype and lineweight lists.
    Props,
}

pub(crate) struct RibbonPanel {
    pub title: &'static str,
    pub cols: &'static [Col],
}

pub(crate) struct RibbonTab {
    pub name: &'static str,
    pub panels: &'static [RibbonPanel],
}

use Col::{Icons, Large, Layers, Props, Small};

/// A separator line between groups of a drop-down's entries.
const SEP: Tool = (Icon::Menu, "-", "");

const CIRCLE_MORE: &[Tool] = &[
    (Icon::Circle, "circle", "Center, Radius"),
    (Icon::Circle, "circle.cd", "Center, Diameter"),
    SEP,
    (Icon::Circle, "circle.2p", "2-Point"),
    (Icon::Circle, "circle.3p", "3-Point"),
    SEP,
    (Icon::Circle, "circle.ttr", "Tan, Tan, Radius"),
    (Icon::Circle, "circle.ttt", "Tan, Tan, Tan"),
];
const ARC_MORE: &[Tool] = &[
    (Icon::Arc, "arc", "3-Point"),
    SEP,
    (Icon::Arc, "arc.sce", "Start, Center, End"),
    (Icon::Arc, "arc.sca", "Start, Center, Angle"),
    (Icon::Arc, "arc.scl", "Start, Center, Length"),
    SEP,
    (Icon::Arc, "arc.sea", "Start, End, Angle"),
    (Icon::Arc, "arc.sed", "Start, End, Direction"),
    (Icon::Arc, "arc.ser", "Start, End, Radius"),
    SEP,
    (Icon::Arc, "arc.cse", "Center, Start, End"),
    (Icon::Arc, "arc.csa", "Center, Start, Angle"),
    (Icon::Arc, "arc.csl", "Center, Start, Length"),
    SEP,
    (Icon::Arc, "arc.continue", "Continue"),
];
const RECT_MORE: &[Tool] = &[(Icon::Rectangle, "rectang", "Rectangle"), (Icon::Polygon, "polygon", "Polygon")];
const ELLIPSE_MORE: &[Tool] =
    &[(Icon::Ellipse, "ellipse", "Center"), (Icon::Ellipse, "ellipse.axis", "Axis, End"), (Icon::Ellipse, "ellipse.arc", "Elliptical Arc")];
const SPLINE_MORE: &[Tool] =
    &[(Icon::Spline, "spline", "Spline Fit"), (Icon::Spline, "spline.cv", "Spline CV"), SEP, (Icon::Conic, "conic", "Conic")];
const HATCH_MORE: &[Tool] = &[(Icon::Hatch, "hatch", "Hatch"), (Icon::Gradient, "gradient", "Gradient"), (Icon::Boundary, "boundary", "Boundary")];
const TRIM_MORE: &[Tool] = &[(Icon::Trim, "trim", "Trim"), (Icon::Extend, "extend", "Extend")];
const FILLET_MORE: &[Tool] = &[(Icon::Fillet, "fillet", "Fillet"), (Icon::Chamfer, "chamfer", "Chamfer"), (Icon::Spline, "blend", "Blend Curves")];
const ARRAY_MORE: &[Tool] = &[
    (Icon::ArrayRect, "arrayrect", "Rectangular Array"),
    (Icon::Spline, "arraypath", "Path Array"),
    (Icon::ArrayPolar, "arraypolar", "Polar Array"),
];
const TEXT_MORE: &[Tool] = &[(Icon::MText, "mtext", "Multiline Text"), (Icon::Text, "text", "Single Line")];
const DIM_MORE: &[Tool] = &[
    (Icon::DimLinear, "dimlinear", "Linear"),
    (Icon::DimAligned, "dimaligned", "Aligned"),
    (Icon::DimAngular, "dimangular", "Angular"),
    (Icon::DimArc, "dimarc", "Arc Length"),
    (Icon::DimRadius, "dimradius", "Radius"),
    (Icon::DimDiameter, "dimdiameter", "Diameter"),
    (Icon::DimRadius, "dimjogged", "Jogged"),
    (Icon::DimOrdinate, "dimordinate", "Ordinate"),
];
const LEADER_MORE: &[Tool] = &[
    (Icon::MLeader, "mleader", "Multileader"),
    (Icon::AddLeader, "mleaderedit.add", "Add Leader"),
    (Icon::RemoveLeader, "mleaderedit.remove", "Remove Leader"),
    (Icon::AlignLeaders, "mleaderalign", "Align"),
    (Icon::MLeader, "mleadercollect", "Collect"),
];
const INSERT_MORE: &[Tool] = &[(Icon::Insert, "insert", "Insert"), (Icon::Blocks, "ui.dialog.blocks", "Blocks Palette")];
const ATT_MORE: &[Tool] =
    &[(Icon::AttDef, "attedit", "Single"), (Icon::List, "battman", "Block Attribute Manager"), (Icon::Redo, "attsync", "Synchronize")];
const MEASURE_MORE: &[Tool] = &[
    (Icon::Distance, "dist", "Distance"),
    (Icon::DimRadius, "radius.measure", "Radius"),
    (Icon::DimAngular, "angle.measure", "Angle"),
    (Icon::Area, "area", "Area"),
    (Icon::Rectangle, "volume.measure", "Volume"),
];
const PASTE_MORE: &[Tool] = &[
    (Icon::Import, "pasteclip", "Paste"),
    (Icon::Blocks, "pasteblock", "Paste as Block"),
    (Icon::Import, "pasteorig", "Paste to Original Coordinates"),
];
const COPYCLIP_MORE: &[Tool] = &[(Icon::Copy, "copyclip", "Copy Clip"), (Icon::Copy, "copybase", "Copy with Base Point")];

/// The ribbon tabs and panels, in the order the Windows "2D Drafting" workspace shows them.
/// Slide-out panels: the tools a panel keeps below its title (tab, panel, tools). Its title
/// opens them; a collapsed panel lists them after its own.
const SLIDE_OUTS: &[(&str, &str, &[Tool])] = &[
    (
        "Home",
        "Draw",
        &[
            (Icon::XLine, "xline", "Construction Line"),
            (Icon::Ray, "ray", "Ray"),
            (Icon::Point, "point", "Multiple Points"),
            (Icon::Point, "divide", "Divide"),
            (Icon::Point, "measure", "Measure"),
            SEP,
            (Icon::Donut, "donut", "Donut"),
            (Icon::RevCloud, "revcloud", "Revision Cloud"),
            (Icon::Region, "region", "Region"),
            (Icon::Wipeout, "wipeout", "Wipeout"),
            (Icon::Boundary, "boundary", "Boundary"),
        ],
    ),
    (
        "Home",
        "Modify",
        &[
            (Icon::Break, "break", "Break"),
            (Icon::Break, "breakatpoint", "Break at Point"),
            (Icon::Join, "join", "Join"),
            (Icon::Lengthen, "lengthen", "Lengthen"),
            (Icon::Chamfer, "chamfer", "Chamfer"),
            (Icon::Spline, "blend", "Blend Curves"),
            SEP,
            (Icon::Polyline, "pedit", "Edit Polyline"),
            (Icon::Spline, "splinedit", "Edit Spline"),
            (Icon::Move, "align", "Align"),
            (Icon::Redo, "reverse", "Reverse"),
            (Icon::Erase, "overkill", "Delete Duplicate Objects"),
            (Icon::MoveTop, "draworder", "Draw Order"),
        ],
    ),
    (
        "Home",
        "Annotation",
        &[
            (Icon::TextStyle, "style", "Text Style"),
            (Icon::DimStyle, "dimstyle", "Dimension Style"),
            (Icon::MLeader, "mleaderstyle", "Multileader Style"),
            (Icon::TextStyle, "tablestyle", "Table Style"),
        ],
    ),
    (
        "Home",
        "Layers",
        &[
            (Icon::MakeCurrent, "laycur", "Change to Current Layer"),
            (Icon::LayerMatch, "laymrg", "Merge"),
            (Icon::Erase, "laydel", "Delete"),
            (Icon::LayerPrev, "layerp", "Previous"),
            (Icon::LayerWalk, "laywalk", "Layer Walk"),
        ],
    ),
    (
        "Home",
        "Block",
        &[
            (Icon::List, "battman", "Block Attribute Manager"),
            (Icon::Redo, "attsync", "Synchronize"),
            (Icon::BlockCreate, "wblock", "Write Block"),
            (Icon::Insert, "blockreplace", "Replace Block"),
        ],
    ),
    (
        "Home",
        "Utilities",
        &[(Icon::Distance, "dist", "Distance"), (Icon::Point, "id", "ID Point"), (Icon::Area, "area", "Area"), (Icon::List, "list", "List")],
    ),
];

/// The slide-out tools of `tab`'s panel `title`, if it has any.
fn slide_out(tab: &str, title: &str) -> &'static [Tool] {
    SLIDE_OUTS.iter().find(|(t, p, _)| *t == tab && *p == title).map_or(&[], |s| s.2)
}

pub(crate) const TABS: &[RibbonTab] = &[
    RibbonTab {
        name: "Home",
        panels: &[
            RibbonPanel {
                title: "Draw",
                cols: &[
                    Large(b(Icon::Line, "line", "Line")),
                    Large(b(Icon::Polyline, "pline", "Polyline")),
                    Large(d(Icon::Circle, "circle", "Circle", CIRCLE_MORE)),
                    Large(d(Icon::Arc, "arc", "Arc", ARC_MORE)),
                    Icons(&[
                        d(Icon::Rectangle, "rectang", "Rectangle", RECT_MORE),
                        d(Icon::Ellipse, "ellipse", "Ellipse", ELLIPSE_MORE),
                        d(Icon::Hatch, "hatch", "Hatch", HATCH_MORE),
                    ]),
                    Icons(&[d(Icon::Spline, "spline", "Spline Fit", SPLINE_MORE), b(Icon::Conic, "conic", "Conic")]),
                ],
            },
            RibbonPanel {
                title: "Modify",
                cols: &[
                    Small(&[b(Icon::Move, "move", "Move"), b(Icon::Copy, "copy", "Copy"), b(Icon::Stretch, "stretch", "Stretch")]),
                    Small(&[b(Icon::Rotate, "rotate", "Rotate"), b(Icon::Mirror, "mirror", "Mirror"), b(Icon::Scale, "scale", "Scale")]),
                    Icons(&[
                        d(Icon::Trim, "trim", "Trim", TRIM_MORE),
                        d(Icon::Fillet, "fillet", "Fillet", FILLET_MORE),
                        d(Icon::ArrayRect, "arrayrect", "Array", ARRAY_MORE),
                    ]),
                    Icons(&[b(Icon::Erase, "erase", "Erase"), b(Icon::Explode, "explode", "Explode"), b(Icon::Offset, "offset", "Offset")]),
                ],
            },
            RibbonPanel {
                title: "Annotation",
                cols: &[
                    Large(d(Icon::MText, "mtext", "Text", TEXT_MORE)),
                    Large(d(Icon::DimLinear, "dim", "Dimension", DIM_MORE)),
                    Icons(&[
                        d(Icon::DimLinear, "dimlinear", "Linear", DIM_MORE),
                        d(Icon::MLeader, "mleader", "Leader", LEADER_MORE),
                        b(Icon::Table, "table", "Table"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Layers",
                cols: &[
                    Large(b(Icon::LayerProps, "ui.dialog.layers", "Layer Properties")),
                    Layers(
                        [
                            &[
                                b(Icon::LayerOff, "layoff", "Off"),
                                b(Icon::LayerIso, "layiso", "Isolate"),
                                b(Icon::LayerFreeze, "layfrz", "Freeze"),
                                b(Icon::LayerLock, "laylck", "Lock"),
                            ],
                            &[
                                b(Icon::Bulb, "layon", "Turn All Layers On"),
                                b(Icon::LayerPrev, "layuniso", "Unisolate"),
                                b(Icon::Sun, "laythw", "Thaw All Layers"),
                                b(Icon::Unlock, "layulk", "Unlock"),
                            ],
                        ],
                        [b(Icon::MakeCurrent, "laymcur", "Make Current"), b(Icon::LayerMatch, "laymch", "Match Layer")],
                    ),
                ],
            },
            RibbonPanel {
                title: "Block",
                cols: &[
                    Large(d(Icon::Insert, "insert", "Insert", INSERT_MORE)),
                    Icons(&[
                        b(Icon::BlockCreate, "block", "Create"),
                        b(Icon::BlockEdit, "bedit", "Edit"),
                        d(Icon::AttDef, "attedit", "Edit Attributes", ATT_MORE),
                    ]),
                ],
            },
            RibbonPanel { title: "Properties", cols: &[Large(b(Icon::MatchProp, "matchprop", "Match Properties")), Props] },
            RibbonPanel {
                title: "Groups",
                cols: &[
                    Large(b(Icon::SelectObjects, "group", "Group")),
                    Icons(&[
                        b(Icon::Explode, "ungroup", "Ungroup"),
                        b(Icon::BlockEdit, "groupedit", "Group Edit"),
                        b(Icon::PickAdd, "pickstyle", "Group Selection On/Off"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Utilities",
                cols: &[
                    Large(d(Icon::Measure, "measuregeom", "Measure", MEASURE_MORE)),
                    Icons(&[
                        b(Icon::QuickSelect, "qselect", "Quick Select"),
                        b(Icon::SelectObjects, "selectall", "Select All"),
                        b(Icon::Table, "ui.toggle.quickcalc", "QuickCalc"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Clipboard",
                cols: &[
                    Large(d(Icon::Import, "pasteclip", "Paste", PASTE_MORE)),
                    Icons(&[b(Icon::Trim, "cutclip", "Cut"), d(Icon::Copy, "copyclip", "Copy Clip", COPYCLIP_MORE)]),
                ],
            },
            RibbonPanel { title: "View", cols: &[Large(b(Icon::ZoomExtents, "zoom.extents", "Zoom Extents"))] },
        ],
    },
    RibbonTab {
        name: "Insert",
        panels: &[
            RibbonPanel {
                title: "Block",
                cols: &[
                    Large(d(Icon::Insert, "insert", "Insert", INSERT_MORE)),
                    Large(b(Icon::BlockCreate, "block", "Create Block")),
                    Small(&[
                        b(Icon::BlockEdit, "bedit", "Block Editor"),
                        b(Icon::Save, "wblock", "Write Block"),
                        b(Icon::Blocks, "ui.dialog.blocks", "Blocks"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Block Definition",
                cols: &[
                    Large(b(Icon::AttDef, "attdef", "Define Attributes")),
                    Small(&[
                        b(Icon::List, "battman", "Manage Attributes"),
                        b(Icon::Text, "attedit", "Edit Attribute"),
                        b(Icon::Redo, "attsync", "Synchronize"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Reference",
                cols: &[
                    Large(b(Icon::Attach, "attach", "Attach")),
                    Small(&[
                        b(Icon::Rectangle, "xclip", "Clip"),
                        b(Icon::Attach, "xref", "External References"),
                        b(Icon::Open, "xopen", "Open Reference"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Import",
                cols: &[
                    Large(b(Icon::Import, "import", "Import")),
                    Large(b(Icon::Pdf, "pdfimport", "PDF Import")),
                    Small(&[
                        b(Icon::Hatch, "imageattach", "Image"),
                        b(Icon::Pdf, "pdfattach", "PDF Underlay"),
                        b(Icon::List, "ulayers", "Underlay Layers"),
                    ]),
                ],
            },
            RibbonPanel { title: "Point Cloud", cols: &[Large(b(Icon::PointCloud, "pointcloudattach", "Attach"))] },
            RibbonPanel { title: "Location", cols: &[Large(b(Icon::GeoLocation, "geographiclocation", "Set Location"))] },
            RibbonPanel {
                title: "Data",
                cols: &[
                    Large(b(Icon::Table, "datalink", "Data Link")),
                    Small(&[b(Icon::Table, "dataextraction", "Extract Data"), b(Icon::Redo, "datalinkupdate", "Update Fields")]),
                ],
            },
        ],
    },
    RibbonTab {
        name: "Annotate",
        panels: &[
            RibbonPanel {
                title: "Text",
                cols: &[
                    Large(d(Icon::MText, "mtext", "Multiline Text", TEXT_MORE)),
                    Small(&[
                        b(Icon::TextStyle, "style", "Text Style"),
                        b(Icon::Search, "find", "Find Text"),
                        b(Icon::Text, "txt2mtxt", "Text to MText"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Dimensions",
                cols: &[
                    Large(d(Icon::DimLinear, "dim", "Dimension", DIM_MORE)),
                    Small(&[
                        d(Icon::DimLinear, "dimlinear", "Linear", DIM_MORE),
                        b(Icon::QDim, "qdim", "Quick"),
                        b(Icon::DimContinue, "dimcontinue", "Continue"),
                    ]),
                    Icons(&[
                        b(Icon::DimStyle, "dimstyle", "Dimension Style"),
                        b(Icon::Break, "dimbreak", "Break"),
                        b(Icon::Distance, "dimspace", "Adjust Space"),
                    ]),
                    Icons(&[
                        b(Icon::DimBaseline, "dimbaseline", "Baseline"),
                        b(Icon::Text, "dimtedit", "Text Angle"),
                        b(Icon::Search, "diminspect", "Inspect"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Leaders",
                cols: &[
                    Large(d(Icon::MLeader, "mleader", "Multileader", LEADER_MORE)),
                    Small(&[
                        b(Icon::AddLeader, "mleaderedit.add", "Add Leader"),
                        b(Icon::RemoveLeader, "mleaderedit.remove", "Remove Leader"),
                        b(Icon::AlignLeaders, "mleaderalign", "Align"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Tables",
                cols: &[
                    Large(b(Icon::Table, "table", "Table")),
                    Small(&[b(Icon::TextStyle, "tablestyle", "Table Style"), b(Icon::Export, "tableexport", "Export")]),
                ],
            },
            RibbonPanel {
                title: "Markup",
                cols: &[
                    Large(b(Icon::RevCloud, "revcloud", "Revision Cloud")),
                    Small(&[b(Icon::Wipeout, "wipeout", "Wipeout"), b(Icon::Text, "tolerance", "Tolerance")]),
                ],
            },
            RibbonPanel {
                title: "Annotation Scaling",
                cols: &[Large(b(Icon::Annotation, "objectscale", "Add/Delete Scales")), Small(&[b(Icon::List, "scalelistedit", "Scale List")])],
            },
        ],
    },
    RibbonTab {
        name: "Parametric",
        panels: &[
            RibbonPanel {
                title: "Geometric",
                cols: &[
                    Large(b(Icon::Constraint, "autoconstrain", "Auto Constrain")),
                    Icons(&[
                        b(Icon::Coincident, "gccoincident", "Coincident"),
                        b(Icon::Collinear, "gccollinear", "Collinear"),
                        b(Icon::Concentric, "gcconcentric", "Concentric"),
                    ]),
                    Icons(&[
                        b(Icon::Fix, "gcfix", "Fix"),
                        b(Icon::Parallel, "gcparallel", "Parallel"),
                        b(Icon::Perpendicular, "gcperpendicular", "Perpendicular"),
                    ]),
                    Icons(&[
                        b(Icon::Horizontal, "gchorizontal", "Horizontal"),
                        b(Icon::Vertical, "gcvertical", "Vertical"),
                        b(Icon::Tangent, "gctangent", "Tangent"),
                    ]),
                    Icons(&[
                        b(Icon::Smooth, "gcsmooth", "Smooth"),
                        b(Icon::Symmetric, "gcsymmetric", "Symmetric"),
                        b(Icon::Equal, "gcequal", "Equal"),
                    ]),
                    Small(&[b(Icon::Bulb, "constraintbar.showall", "Show All"), b(Icon::BulbOff, "constraintbar.hideall", "Hide All")]),
                ],
            },
            RibbonPanel {
                title: "Dimensional",
                cols: &[
                    Large(b(Icon::DimLinear, "dclinear", "Linear")),
                    Large(b(Icon::DimAligned, "dcaligned", "Aligned")),
                    Icons(&[
                        b(Icon::DimRadius, "dcradius", "Radius"),
                        b(Icon::DimDiameter, "dcdiameter", "Diameter"),
                        b(Icon::DimAngular, "dcangular", "Angular"),
                    ]),
                    Small(&[b(Icon::Bulb, "dcdisplay.showall", "Show All"), b(Icon::BulbOff, "dcdisplay.hideall", "Hide All")]),
                ],
            },
            RibbonPanel {
                title: "Manage",
                cols: &[
                    Large(b(Icon::DelConstraint, "delconstraint", "Delete Constraints")),
                    Large(b(Icon::Parameters, "parameters", "Parameters Manager")),
                ],
            },
        ],
    },
    RibbonTab {
        name: "View",
        panels: &[
            RibbonPanel {
                title: "Viewport Tools",
                cols: &[
                    Large(b(Icon::Osnap, "ui.toggle.ucsicon", "UCS Icon")),
                    Large(b(Icon::Orbit, "ui.toggle.viewcube", "View Cube")),
                    Large(b(Icon::Menu, "ui.toggle.history", "Text Window")),
                ],
            },
            RibbonPanel {
                title: "Navigate",
                cols: &[
                    Large(b(Icon::ZoomExtents, "zoom.extents", "Extents")),
                    Small(&[b(Icon::Pan, "pan", "Pan"), b(Icon::ZoomWindow, "zoom.window", "Window"), b(Icon::ZoomFit, "zoom.previous", "Previous")]),
                    Small(&[b(Icon::ZoomIn, "zoom.in", "Zoom In"), b(Icon::ZoomOut, "zoom.out", "Zoom Out"), b(Icon::Orbit, "3dorbit", "Orbit")]),
                ],
            },
            RibbonPanel {
                title: "Model Viewports",
                cols: &[
                    Large(b(Icon::Rectangle, "vports", "Viewport Configuration")),
                    Small(&[b(Icon::Rectangle, "vports.1", "Single"), b(Icon::Grid, "vports.2", "Two"), b(Icon::Grid, "vports.4", "Four")]),
                ],
            },
            RibbonPanel {
                title: "Palettes",
                cols: &[
                    Large(b(Icon::ToolPalette, "toolpalettes", "Tool Palettes")),
                    Large(b(Icon::Properties, "ui.panel.properties", "Properties")),
                    Large(b(Icon::Layers, "ui.panel.layers", "Layers")),
                    Small(&[
                        b(Icon::SheetSet, "sheetset", "Sheet Set Manager"),
                        b(Icon::DesignCenter, "adcenter", "DesignCenter"),
                        b(Icon::Table, "ui.toggle.quickcalc", "QuickCalc"),
                    ]),
                    Small(&[b(Icon::Attach, "xref", "External References"), b(Icon::Blocks, "ui.dialog.blocks", "Blocks")]),
                ],
            },
            RibbonPanel {
                title: "Interface",
                cols: &[
                    Large(b(Icon::Switcher, "ui.toggle.filetabs", "File Tabs")),
                    Small(&[
                        b(Icon::Menu, "ui.toggle.statusbar", "Status Bar"),
                        b(Icon::Text, "ui.toggle.cmdline", "Command Line"),
                        b(Icon::Menu, "ui.toggle.menubar", "Menu Bar"),
                    ]),
                ],
            },
        ],
    },
    RibbonTab {
        name: "Manage",
        panels: &[
            RibbonPanel { title: "Action Recorder", cols: &[Large(b(Icon::Text, "script", "Run Script"))] },
            RibbonPanel {
                title: "Customization",
                cols: &[
                    Large(b(Icon::Customize, "cui", "User Interface")),
                    Large(b(Icon::Gear, "options", "Options")),
                    Small(&[
                        b(Icon::Text, "aliasedit", "Edit Aliases"),
                        b(Icon::Menu, "ui.dialog.commands", "Commands"),
                        b(Icon::Workspace, "workspace", "Workspaces"),
                    ]),
                ],
            },
            RibbonPanel { title: "Applications", cols: &[Large(b(Icon::Open, "appload", "Load Application"))] },
            RibbonPanel {
                title: "CAD Standards",
                cols: &[
                    Large(b(Icon::Erase, "purge", "Purge")),
                    Large(b(Icon::LayerTranslate, "laytrans", "Layer Translator")),
                    Small(&[b(Icon::Search, "audit", "Audit"), b(Icon::Join, "overkill", "Overkill"), b(Icon::Redo, "recover", "Recover")]),
                ],
            },
            RibbonPanel {
                title: "Settings",
                cols: &[
                    Large(b(Icon::Gear, "ui.dialog.dsettings", "Drafting Settings")),
                    Small(&[
                        b(Icon::Distance, "units", "Units"),
                        b(Icon::Rectangle, "limits", "Limits"),
                        b(Icon::List, "sysvars", "System Variables"),
                    ]),
                ],
            },
        ],
    },
    RibbonTab {
        name: "Output",
        panels: &[
            RibbonPanel {
                title: "Plot",
                cols: &[
                    Large(b(Icon::Plot, "plot", "Plot")),
                    Small(&[
                        b(Icon::PageSetup, "pagesetup", "Page Setup Manager"),
                        b(Icon::Preview, "preview", "Preview"),
                        b(Icon::Gear, "plottermanager", "Plotter Manager"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Export to DWF/PDF",
                cols: &[
                    Large(b(Icon::Export, "exportpdf", "Export PDF")),
                    Small(&[b(Icon::Export, "export", "Export"), b(Icon::Publish, "publish", "Batch Plot")]),
                ],
            },
        ],
    },
    RibbonTab {
        name: "Add-ins",
        panels: &[RibbonPanel {
            title: "Applications",
            cols: &[
                Large(b(Icon::Open, "appload", "Load Application")),
                Small(&[b(Icon::Text, "script", "Run Script"), b(Icon::Text, "lisp", "AutoLISP")]),
            ],
        }],
    },
    RibbonTab {
        name: "Collaborate",
        panels: &[
            RibbonPanel { title: "Share", cols: &[Large(b(Icon::Share, "share", "Share Drawing"))] },
            RibbonPanel { title: "Compare", cols: &[Large(b(Icon::Search, "compare", "DWG Compare"))] },
            RibbonPanel { title: "Count", cols: &[Large(b(Icon::List, "count", "Count"))] },
        ],
    },
    RibbonTab {
        name: "Extra Tools",
        panels: &[
            RibbonPanel {
                title: "Blocks",
                cols: &[Small(&[
                    b(Icon::Explode, "burst", "Explode Attributes"),
                    b(Icon::Blocks, "blockreplace", "Replace Block"),
                    b(Icon::Text, "attdisp", "Attribute Display"),
                ])],
            },
            RibbonPanel {
                title: "Text",
                cols: &[Small(&[
                    b(Icon::Text, "txt2mtxt", "Convert to MText"),
                    b(Icon::Search, "find", "Find"),
                    b(Icon::Text, "texttofront", "Text to Front"),
                ])],
            },
            RibbonPanel {
                title: "Modify",
                cols: &[Small(&[
                    b(Icon::Join, "overkill", "Delete Duplicate Objects"),
                    b(Icon::Break, "breakline", "Break-line Symbol"),
                    b(Icon::Copy, "ncopy", "Copy Nested Objects"),
                ])],
            },
            RibbonPanel {
                title: "Layers",
                cols: &[Small(&[
                    b(Icon::LayerMatch, "laymrg", "Merge"),
                    b(Icon::Erase, "laydel", "Delete"),
                    b(Icon::LayerProps, "layerstate.list", "Layer States"),
                ])],
            },
        ],
    },
];

/// The Quick Access Toolbar in the application bar.
const QAT: &[Tool] = &[
    (Icon::New, "new", "New"),
    (Icon::Open, "open", "Open"),
    (Icon::Save, "qsave", "Save"),
    (Icon::SaveAs, "saveas", "Save As"),
    (Icon::Plot, "plot", "Plot"),
    (Icon::Undo, "u", "Undo"),
    (Icon::Redo, "redo", "Redo"),
];

/// The application menu (the button at the window's top left).
const APP_MENU: &[(Icon, &str, &str)] = &[
    (Icon::New, "new", "New"),
    (Icon::Open, "open", "Open"),
    (Icon::Save, "qsave", "Save"),
    (Icon::SaveAs, "saveas", "Save As"),
    (Icon::Import, "import", "Import"),
    (Icon::Export, "export", "Export"),
    (Icon::Publish, "publish", "Publish"),
    (Icon::Transmit, "etransmit", "eTransmit"),
    (Icon::Plot, "plot", "Print"),
    (Icon::Search, "audit", "Drawing Utilities"),
    (Icon::Close, "close", "Close"),
];

const APP_BAR: f32 = 32.0;
const TAB_ROW: f32 = 26.0;
const BODY: f32 = 76.0;
const PANEL_TITLE: f32 = 20.0;
const ROW: f32 = 23.0;
const LARGE_ICON: f32 = 32.0;
const SMALL_ICON: f32 = 16.0;
const MINI: f32 = 26.0;
const ARROW: f32 = 11.0;
const LAYER_LIST: f32 = 200.0;
const PROP_LIST: f32 = 140.0;
/// Where title-row content may start right of the macOS traffic lights.
const MAC_CONTENT_LEFT: f32 = 78.0;

/// Per-frame state the ribbon keeps outside the saved preferences.
#[derive(Clone, Default)]
struct Scratch {
    search: String,
}

fn known(id: &str) -> bool {
    draftcraft_engine::find_command(id).is_some() || crate::menus::is_ui_command(id)
}

fn label_font() -> FontId {
    FontId::proportional(12.0)
}

/// The application bar and tab row: darker than the ribbon body, as in the Windows dark theme.
fn bar_color(t: &Tokens) -> Color32 {
    t.chrome_dark.lerp_to_gamma(t.border, 0.45)
}

/// A large button's label split into at most two lines (at the space nearest the middle).
fn large_lines(label: &str) -> (String, Option<String>) {
    if label.chars().count() <= 9 || !label.contains(' ') {
        return (label.to_string(), None);
    }
    let mid = label.len() / 2;
    let split = label.char_indices().filter(|(_, c)| *c == ' ').map(|(i, _)| i).min_by_key(|i| i.abs_diff(mid));
    match split {
        Some(i) => (label.get(..i).unwrap_or(label).to_string(), label.get(i + 1..).map(str::to_string)),
        None => (label.to_string(), None),
    }
}

fn text_w(p: &egui::Painter, s: &str) -> f32 {
    p.layout_no_wrap(s.to_string(), label_font(), Color32::WHITE).size().x
}

/// A column's width; `compact` drops the small buttons' labels.
fn col_width(p: &egui::Painter, col: &Col, compact: bool) -> f32 {
    match col {
        Large(btn) => {
            let (a, b) = large_lines(crate::i18n::t(btn.label));
            let w = text_w(p, &a).max(b.as_deref().map(|b| text_w(p, b) + if btn.more.is_empty() { 0.0 } else { ARROW }).unwrap_or(0.0));
            w.max(LARGE_ICON) + 14.0
        }
        Small(btns) => {
            let arrow = if btns.iter().any(|b| !b.more.is_empty()) { ARROW } else { 0.0 };
            if compact {
                MINI + arrow + 2.0
            } else {
                btns.iter().map(|b| text_w(p, crate::i18n::t(b.label))).fold(0.0, f32::max) + SMALL_ICON + 16.0 + arrow
            }
        }
        Icons(btns) => MINI + if btns.iter().any(|b| !b.more.is_empty()) { ARROW } else { 0.0 } + 2.0,
        Layers(rows, labelled) => {
            let icons = rows.iter().map(|r| r.len()).max().unwrap_or(0) as f32 * MINI;
            let label =
                if compact { MINI } else { labelled.iter().map(|b| text_w(p, crate::i18n::t(b.label))).fold(0.0, f32::max) + SMALL_ICON + 14.0 };
            if compact { (icons + label).max(150.0) + 6.0 } else { LAYER_LIST.max(icons + label) + 6.0 }
        }
        Props => PROP_LIST + 30.0,
    }
}

fn panel_width(p: &egui::Painter, panel: &RibbonPanel, compact: bool) -> f32 {
    let cols: f32 = panel.cols.iter().map(|c| col_width(p, c, compact)).sum();
    let title = text_w(p, crate::i18n::t(panel.title)) + 34.0;
    (cols + 12.0).max(title)
}

fn collapsed_width(p: &egui::Painter, panel: &RibbonPanel) -> f32 {
    text_w(p, crate::i18n::t(panel.title)).max(LARGE_ICON) + 26.0
}

fn first_icon(panel: &RibbonPanel) -> Icon {
    panel
        .cols
        .iter()
        .find_map(|c| match c {
            Large(b) => Some(b.icon),
            Small(b) | Icons(b) => b.first().map(|b| b.icon),
            Layers(_, _) => Some(Icon::LayerProps),
            Props => Some(Icon::MatchProp),
        })
        .unwrap_or(Icon::Menu)
}

/// Every command a panel reaches, for its collapsed button and its title menu.
/// The icon the ribbon shows for `cmd`, if it has a button there.
pub(crate) fn icon_for(cmd: &str) -> Option<Icon> {
    TABS.iter().flat_map(|t| t.panels.iter()).flat_map(all_tools).find(|(_, c, _)| c.eq_ignore_ascii_case(cmd)).map(|(icon, _, _)| icon)
}

fn all_tools(panel: &RibbonPanel) -> Vec<Tool> {
    let mut out: Vec<Tool> = Vec::new();
    let mut push = |b: &Btn| {
        out.push((b.icon, b.cmd, b.label));
        out.extend(b.more.iter().filter(|m| m.1 != b.cmd && m.1 != SEP.1).copied());
    };
    for c in panel.cols {
        match c {
            Large(b) => push(b),
            Small(bs) | Icons(bs) => bs.iter().for_each(&mut push),
            Layers(rows, labelled) => {
                rows.iter().for_each(|r| r.iter().for_each(&mut push));
                labelled.iter().for_each(&mut push);
            }
            Props => {
                push(&b(Icon::Properties, "color", "Color"));
                push(&b(Icon::Lineweight, "linetype", "Linetype"));
                push(&b(Icon::Lineweight, "lweight", "Lineweight"));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|t| seen.insert((t.1, t.2)));
    out
}

/// A panel's accessible name ("Draw panel"): distinct from the menu titles and commands that
/// share its words (the View menu, the Insert command).
fn panel_name(title: &str) -> String {
    crate::tf!("{panel} panel", panel = crate::i18n::t(title))
}

fn tooltip(label: &str, cmd: &str) -> String {
    let name = crate::i18n::t(label);
    if cmd.starts_with("ui.") { name.to_string() } else { format!("{name}\n{}", cmd.split('.').next().unwrap_or(cmd).to_uppercase()) }
}

/// Hover and press feedback for a ribbon control, then its interaction.
fn hit(ui: &egui::Ui, rect: Rect, id: egui::Id, label: &str, cmd: &str, enabled: bool) -> egui::Response {
    let t = Tokens::get();
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, id, sense);
    if enabled && resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 2.0, t.tab_active.gamma_multiply(1.2));
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 2.0, t.control_hover.gamma_multiply(0.55));
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, t.control_hover), egui::StrokeKind::Inside);
    }
    icons::describe_control(ui, &resp, label, None, false);
    resp.on_hover_text(tooltip(label, cmd))
}

fn arrow(p: &egui::Painter, center: egui::Pos2, color: Color32) {
    let s = 3.0;
    p.add(egui::Shape::convex_polygon(
        vec![center + vec2(-s, -s * 0.5), center + vec2(s, -s * 0.5), center + vec2(0.0, s * 0.6)],
        color,
        Stroke::NONE,
    ));
}

/// A drop-down's entries, as the Windows ribbon lists them: a large picture of each method,
/// its name, groups between separator lines, and no wider than its longest name.
fn tool_menu(ui: &mut egui::Ui, tools: &[Tool]) -> Option<&'static str> {
    let t = Tokens::get();
    let mut picked = None;
    let shown: Vec<&Tool> = tools.iter().filter(|(_, cmd, _)| *cmd == "-" || known(cmd)).collect();
    let width = shown.iter().filter(|e| e.1 != "-").map(|(_, _, l)| text_w(ui.painter(), crate::i18n::t(l))).fold(0.0, f32::max) + 52.0;
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    let mut last_sep = true;
    for (icon, cmd, label) in shown {
        if *cmd == "-" {
            if !last_sep {
                let (r, _) = ui.allocate_exact_size(vec2(width, 5.0), Sense::hover());
                ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.border));
            }
            last_sep = true;
            continue;
        }
        last_sep = false;
        let name = crate::i18n::t(label);
        let (r, resp) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, 2.0, t.control_hover.gamma_multiply(0.7));
            ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, t.control_hover), egui::StrokeKind::Inside);
        }
        let ir = Rect::from_min_size(pos2(r.left() + 4.0, r.center().y - 12.0), vec2(24.0, 24.0));
        if !paint_variant(ui.painter(), ir, cmd) {
            icons::paint(ui.painter(), ir.shrink(2.0), *icon, false);
        }
        ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, name, label_font(), t.text);
        crate::control::record_widget(ui, &resp, label, "button", None);
        if resp.on_hover_text(tooltip(label, cmd)).clicked() {
            picked = Some(*cmd);
            ui.close();
        }
    }
    picked
}

/// A picture of an ARC or CIRCLE method in a 24-point box: the curve, and the points the method
/// asks for (DraftCraft's own drawing). False for any other command.
fn paint_variant(p: &egui::Painter, r: Rect, cmd: &str) -> bool {
    let t = Tokens::get();
    let s = r.width() / 24.0;
    let at = |x: f32, y: f32| pos2(r.left() + x * s, r.top() + y * s);
    let line = Stroke::new(1.3, t.icon);
    let guide = Stroke::new(1.0, t.icon_accent);
    let dot = |x: f32, y: f32| {
        p.circle_filled(at(x, y), 2.2 * s, t.icon_point);
    };
    let arc_pts = |cx: f32, cy: f32, rad: f32, a0: f32, a1: f32| -> Vec<egui::Pos2> {
        (0..=16).map(|i| a0 + (a1 - a0) * i as f32 / 16.0).map(|a| at(cx + rad * a.cos(), cy - rad * a.sin())).collect()
    };
    use std::f32::consts::{FRAC_PI_2, PI};
    if let Some(m) = cmd.strip_prefix("arc") {
        // Quarter arc from S (left) to E (top), centre C at the lower right.
        let (cx, cy, rad) = (19.0, 19.0, 14.0);
        p.add(egui::Shape::line(arc_pts(cx, cy, rad, PI, FRAC_PI_2), line));
        let (sx, sy, ex, ey) = (cx - rad, cy, cx, cy - rad);
        match m {
            "" => {
                dot(sx, sy);
                dot(cx - rad * 0.707, cy - rad * 0.707);
                dot(ex, ey);
            }
            ".continue" => {
                p.line_segment([at(sx, 23.0), at(sx, sy)], guide);
                dot(sx, sy);
            }
            _ => {
                let pts: Vec<char> = m.trim_start_matches('.').chars().collect();
                for c in &pts {
                    match c {
                        's' => dot(sx, sy),
                        'e' => dot(ex, ey),
                        'c' => {
                            p.line_segment([at(cx, cy), at(sx, sy)], guide);
                            dot(cx, cy);
                        }
                        'a' => {
                            p.add(egui::Shape::line(arc_pts(cx, cy, 5.0, PI, FRAC_PI_2), guide));
                        }
                        'l' => {
                            p.line_segment([at(sx, sy), at(ex, ey)], guide);
                        }
                        'd' => {
                            p.line_segment([at(sx, sy), at(sx, sy - 7.0)], guide);
                            p.line_segment([at(sx - 2.0, sy - 5.0), at(sx, sy - 7.0)], guide);
                            p.line_segment([at(sx + 2.0, sy - 5.0), at(sx, sy - 7.0)], guide);
                        }
                        'r' => {
                            p.line_segment([at(cx, cy), at(cx - rad * 0.707, cy - rad * 0.707)], guide);
                        }
                        _ => {}
                    }
                }
            }
        }
        return true;
    }
    if let Some(m) = cmd.strip_prefix("circle") {
        let (cx, cy, rad) = (12.0, 12.0, if m == ".ttt" { 5.5 } else { 8.5 });
        p.circle_stroke(at(cx, cy), rad * s, line);
        match m {
            "" => {
                p.line_segment([at(cx, cy), at(cx + rad * 0.707, cy - rad * 0.707)], guide);
                dot(cx, cy);
            }
            ".cd" => {
                p.line_segment([at(cx - rad, cy), at(cx + rad, cy)], guide);
                dot(cx, cy);
            }
            ".2p" => {
                dot(cx - rad, cy);
                dot(cx + rad, cy);
            }
            ".3p" => {
                dot(cx, cy - rad);
                dot(cx - rad * 0.87, cy + rad * 0.5);
                dot(cx + rad * 0.87, cy + rad * 0.5);
            }
            ".ttr" => {
                p.line_segment([at(1.0, cy + rad), at(23.0, cy + rad)], guide);
                p.line_segment([at(cx - rad, 1.0), at(cx - rad, 23.0)], guide);
                p.line_segment([at(cx, cy), at(cx + rad * 0.707, cy - rad * 0.707)], guide);
            }
            ".ttt" => {
                // An equilateral triangle around the circle (its incircle).
                let k = rad * 2.0;
                let tri = [at(cx, cy - k), at(cx + k * 0.866, cy + k * 0.5), at(cx - k * 0.866, cy + k * 0.5)];
                p.add(egui::Shape::closed_line(tri.to_vec(), guide));
            }
            _ => return false,
        }
        return true;
    }
    false
}

/// What the ribbon asked for this frame.
#[derive(Default)]
struct Out {
    command: Option<&'static str>,
    /// A property picked from a list of the Layers or Properties panel (see [`set_property`]).
    prop: Option<Prop>,
    run: Option<(&'static str, serde_json::Value)>,
    typed: Option<String>,
}

/// A property picked from a list of the Layers or Properties panel: `key` and `value` as CHPROP
/// takes them, and the command (with its parameters) that makes it the current one.
struct Prop {
    key: &'static str,
    value: serde_json::Value,
    current: (&'static str, serde_json::Value),
}

/// Apply a property picked from a ribbon list, as AutoCAD's layer and property controls do: with
/// objects selected it changes them (CHPROP, which leaves out those on locked layers and says
/// how many), and the current layer or property stays as it was; with nothing selected it becomes
/// the current one.
fn set_property(app: &mut CadApp, prop: Prop) {
    let sel = app.session.selection();
    let r = if sel.is_empty() {
        app.run(prop.current.0, prop.current.1)
    } else {
        let handles: Vec<String> = sel.iter().map(|h| h.hex()).collect();
        let mut params = serde_json::Map::new();
        params.insert("handles".into(), json!(handles));
        params.insert(prop.key.into(), prop.value);
        app.run("chprop", serde_json::Value::Object(params)).inspect(|r| match r.get("locked").and_then(serde_json::Value::as_u64).unwrap_or(0) {
            0 => {}
            1 => app.session.echo("1 object was on a locked layer."),
            n => app.session.echo(format!("{n} objects were on a locked layer.")),
        })
    };
    if let Err(e) = r {
        app.set_status(e);
    }
}

/// What the selected objects share for one property: `None` with nothing selected, `Some(None)`
/// when they differ, `Some(Some(v))` when every one has `v`. The ribbon's lists show it in place
/// of the current value while objects are selected, as AutoCAD's do.
fn selection_shares<T: PartialEq>(app: &CadApp, f: impl Fn(&draftcraft_engine::doc::Entity) -> T) -> Option<Option<T>> {
    let sel = app.session.selection();
    let d = app.session.doc().ok()?;
    let mut values = sel.iter().filter_map(|h| d.entity(*h)).map(|e| f(e));
    let first = values.next()?;
    Some(values.all(|v| v == first).then_some(first))
}

/// The application button: the DraftCraft logo ([`crate::brand`]) and the menu arrow.
fn paint_app_button(p: &egui::Painter, r: Rect, hovered: bool) {
    let t = Tokens::get();
    let mark = Rect::from_min_size(r.min, vec2(r.height(), r.height()));
    if hovered {
        p.rect_filled(r.expand(2.0), 4.0, t.hover);
    }
    crate::brand::paint(p, mark);
    arrow(p, pos2(mark.right() + 7.0, r.center().y), t.text_dim);
}

/// Application bar, ribbon tabs and the active tab's panels.
pub fn title_and_ribbon(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let ribbon_h = if app.ui.show_toolbar { TAB_ROW + if app.ui.ribbon_minimized { 0.0 } else { BODY + PANEL_TITLE + 4.0 } } else { 0.0 };
    let mut out = Out::default();
    context::update(app);
    egui::Panel::top("cc_title_ribbon").exact_size(APP_BAR + ribbon_h).frame(egui::Frame::NONE.fill(bar_color(&t))).show(ui, |ui| {
        let r = ui.max_rect();
        app_bar(app, ui, Rect::from_min_size(r.min, vec2(r.width(), APP_BAR)), &mut out);
        if ribbon_h > 0.0 {
            ribbon(app, ui, Rect::from_min_max(pos2(r.left(), r.top() + APP_BAR), r.max), &mut out);
        }
    });
    if let Some(prop) = out.prop {
        set_property(app, prop);
    }
    if let Some((cmd, params)) = out.run
        && let Err(e) = app.run(cmd, params)
    {
        app.set_status(e);
    }
    if let Some(text) = out.typed {
        app.cmdline(&text);
    }
    if let Some(cmd) = out.command {
        app.start(cmd);
    }
}

fn app_bar(app: &mut CadApp, ui: &mut egui::Ui, bar: Rect, out: &mut Out) {
    let t = Tokens::get();
    if app.integrated_titlebar || app.own_window_frame {
        // The empty bar moves the window; it goes first so later widgets take their own clicks.
        craft_window::title_bar::empty_area(ui, bar);
    }
    let p = ui.painter().clone();
    let cy = bar.center().y;
    let mut x = bar.left() + if app.integrated_titlebar { MAC_CONTENT_LEFT } else { 8.0 };
    // Application button and menu.
    let ab = Rect::from_min_size(pos2(x, cy - 11.0), vec2(36.0, 22.0));
    let aresp = ui.interact(ab, ui.scope_id().with("app_button"), Sense::click());
    paint_app_button(&p, ab, aresp.hovered());
    icons::describe_control(ui, &aresp, "Application Menu", None, false);
    egui::Popup::menu(&aresp).show(|ui| {
        let tools: Vec<Tool> = APP_MENU.to_vec();
        if let Some(c) = tool_menu(ui, &tools) {
            out.command = Some(c);
        }
        ui.separator();
        // Options and Exit as framed buttons at the bottom right, as the Windows application menu has them.
        // Only as wide as the entries above: a popup's own width is unbounded.
        let width = ui.min_rect().width();
        ui.allocate_ui_with_layout(vec2(width, 30.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.spacing_mut().button_padding = vec2(10.0, 4.0);
            if ui.add(egui::Button::new(crate::tl!("Exit DraftCraft")).frame(true)).clicked() {
                out.command = Some("ui.quit");
                ui.close();
            }
            if ui.add(egui::Button::new(crate::tl!("Options")).frame(true)).clicked() {
                out.command = Some("options");
                ui.close();
            }
        });
    });
    aresp.on_hover_text(crate::tl!("Application Menu"));
    x = ab.right() + 10.0;
    // Quick Access Toolbar.
    for (icon, cmd, label) in &qat_tools(app) {
        if !known(cmd) {
            continue;
        }
        let br = Rect::from_min_size(pos2(x, cy - 12.0), vec2(24.0, 24.0));
        let resp = hit(ui, br, ui.scope_id().with(("qat", *cmd)), label, cmd, true);
        icons::paint(&p, br.shrink(3.5), *icon, false);
        if resp.clicked() {
            out.command = Some(cmd);
        }
        x += 26.0;
        if *cmd == "u" || *cmd == "redo" {
            // Undo and Redo carry a history arrow in the Windows toolbar.
            arrow(&p, pos2(x + 1.0, cy), t.text_dim);
            x += 8.0;
        }
    }
    // Workspace switcher.
    x += 8.0;
    let ws = Rect::from_min_size(pos2(x, cy - 11.0), vec2(170.0, 22.0));
    let wresp = ui.interact(ws, ui.scope_id().with("workspace"), Sense::click());
    p.rect_filled(ws, 2.0, if wresp.hovered() { t.control } else { t.chrome_dark });
    p.rect_stroke(ws, 2.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let current = crate::workspaces::current(app);
    let shown = if current == crate::workspaces::BUILT_IN { crate::i18n::t(crate::workspaces::BUILT_IN).to_string() } else { current.clone() };
    p.with_clip_rect(ws.shrink(2.0)).text(pos2(ws.left() + 7.0, cy), Align2::LEFT_CENTER, shown, label_font(), t.text);
    arrow(&p, pos2(ws.right() - 9.0, cy), t.text);
    icons::describe_control(ui, &wresp, "Workspace", None, false);
    let names = crate::workspaces::names(app);
    let mut switch_to: Option<String> = None;
    egui::Popup::menu(&wresp).show(|ui| {
        for n in &names {
            let label = if n == crate::workspaces::BUILT_IN { crate::i18n::t(n).to_string() } else { n.clone() };
            if ui.selectable_label(*n == current, label).clicked() && *n != current {
                switch_to = Some(n.clone());
                ui.close();
            }
        }
        ui.separator();
        if ui.button(crate::tl!("Save Current As...")).clicked() {
            out.command = Some("cui");
            ui.close();
        }
        if ui.button(crate::tl!("Customize...")).clicked() {
            out.command = Some("cui");
            ui.close();
        }
        if ui.button(crate::tl!("Reset Palettes")).clicked() {
            out.command = Some("ui.dock.reset");
            ui.close();
        }
    });
    if let Some(n) = switch_to
        && let Err(e) = crate::workspaces::set_current(app, &n)
    {
        app.set_status(e);
    }
    let qat_end = ws.right() + 14.0;
    // Search box on the right: typed text runs as a command, like the command line.
    let search_w = 220.0;
    let sr = Rect::from_min_size(pos2(bar.right() - search_w - 14.0, cy - 11.0), vec2(search_w, 22.0));
    if sr.left() > qat_end + 120.0 {
        let id = ui.scope_id().with("ribbon_search");
        let mut scratch: Scratch = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        p.rect_filled(sr, 2.0, t.field);
        p.rect_stroke(sr, 2.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        icons::paint(&p, Rect::from_center_size(pos2(sr.right() - 12.0, cy), vec2(14.0, 14.0)), Icon::Search, false);
        let field = Rect::from_min_max(sr.min + vec2(4.0, 2.0), sr.max - vec2(24.0, 2.0));
        let resp = ui.put(
            field,
            egui::TextEdit::singleline(&mut scratch.search)
                .frame(egui::Frame::NONE)
                .hint_text(egui::RichText::new(crate::tl!("Type a keyword or phrase")).italics().color(t.text_faint))
                .font(label_font())
                .text_color(t.text)
                .align(egui::Align2::LEFT_CENTER),
        );
        // Screen readers name the field like its placeholder.
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, crate::tl!("Type a keyword or phrase")));
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let text = scratch.search.trim().to_string();
            if !text.is_empty() {
                out.typed = Some(text);
            }
            scratch.search.clear();
        }
        ui.data_mut(|d| d.insert_temp(id, scratch));
    }
    // Window title, centred, between the toolbar and the search box.
    let title = match app.session.state() {
        Ok(st) if !app.ui.start_tab => format!("DraftCraft   {}{}", st.title, if st.title.contains('.') { "" } else { ".dwg" }),
        _ => format!("DraftCraft   {}", crate::tl!("Start")),
    };
    let galley = p.layout_no_wrap(title, FontId::proportional(13.0), t.text);
    let right_limit = if sr.left() > qat_end + 120.0 { sr.left() - 10.0 } else { bar.right() - 8.0 };
    let tx = (bar.center().x - galley.size().x / 2.0).max(qat_end);
    if tx + galley.size().x < right_limit {
        p.galley(pos2(tx, cy - galley.size().y / 2.0), galley, t.text);
    }
}

/// Fit a tab's panels in `available` points: full panels, then small buttons without labels
/// (from the right), then panels collapsed to one button (from the right). Returns each panel's
/// compact flag, full width, collapsed width and collapsed flag.
fn fit_panels(p: &egui::Painter, tab: &RibbonTab, available: f32) -> (Vec<bool>, Vec<f32>, Vec<f32>, Vec<bool>) {
    // Fit: full panels, then small buttons without labels, then panels collapsed from the right.
    // Panels give up their small buttons' labels one at a time from the right, as on Windows.
    let mut compacts = vec![false; tab.panels.len()];
    let mut widths: Vec<f32> = tab.panels.iter().map(|panel| panel_width(p, panel, false)).collect();
    for i in (0..tab.panels.len()).rev() {
        if widths.iter().sum::<f32>() <= available {
            break;
        }
        if let (Some(c), Some(w), Some(panel)) = (compacts.get_mut(i), widths.get_mut(i), tab.panels.get(i)) {
            *c = true;
            *w = panel_width(p, panel, true);
        }
    }
    let small: Vec<f32> = tab.panels.iter().map(|panel| collapsed_width(p, panel)).collect();
    let mut collapsed = vec![false; tab.panels.len()];
    let total = |collapsed: &[bool]| -> f32 { widths.iter().zip(&small).zip(collapsed).map(|((w, s), c)| if *c { *s } else { *w }).sum() };
    for i in (0..tab.panels.len()).rev() {
        if total(&collapsed) <= available {
            break;
        }
        if let Some(c) = collapsed.get_mut(i) {
            *c = true;
        }
    }

    (compacts, widths, small, collapsed)
}

/// The tabs the ribbon shows now: the standard ones, then the contextual tab the app's state
/// calls for ([`context`]).
fn shown_tabs(app: &CadApp) -> Vec<&'static RibbonTab> {
    TABS.iter().filter(|tab| !app.ui.hidden_tabs.iter().any(|h| h == tab.name)).chain(context::current(app)).collect()
}

/// The standard ribbon tabs' names (CUI shows or hides them).
pub(crate) fn tab_names() -> Vec<&'static str> {
    TABS.iter().map(|tab| tab.name).collect()
}

/// The Quick Access Toolbar's tools: the customized ones (CUI), else the standard ones.
fn qat_tools(app: &CadApp) -> Vec<Tool> {
    match &app.ui.qat {
        None => QAT.to_vec(),
        Some(ids) => ids.iter().filter_map(|id| qat_tool(id)).take(crate::workspaces::MAX_QAT).collect(),
    }
}

/// The Quick Access Toolbar's command ids.
pub(crate) fn qat_ids(app: &CadApp) -> Vec<String> {
    qat_tools(app).iter().map(|t| t.1.to_string()).collect()
}

/// A command as a Quick Access Toolbar tool: its ribbon icon, else a generic one.
fn qat_tool(id: &str) -> Option<Tool> {
    let id = id.trim();
    if let Some(t) = QAT.iter().chain(APP_MENU).find(|t| t.1.eq_ignore_ascii_case(id)) {
        return Some(*t);
    }
    if let Some(t) = TABS.iter().flat_map(|t| t.panels.iter()).flat_map(all_tools).find(|t| t.1.eq_ignore_ascii_case(id)) {
        return Some(t);
    }
    if let Some(c) = draftcraft_engine::find_command(id) {
        return Some((Icon::Customize, c.id, c.label));
    }
    crate::menus::UI_COMMANDS.iter().find(|c| c.0.eq_ignore_ascii_case(id)).map(|c| (Icon::Customize, c.0, c.1))
}

/// The ribbon tab names and the minimize switch after them.
fn ribbon_tabs(app: &mut CadApp, ui: &mut egui::Ui, area: Rect, tabs: &[&'static RibbonTab], active: usize) {
    let t = Tokens::get();
    let p = ui.painter().clone();
    let tabs_top = area.top();
    // Tabs.
    let mut tx = area.left() + 6.0;
    for (i, tab) in tabs.iter().enumerate() {
        let name = crate::i18n::t(tab.name);
        let w = text_w(&p, name) + 22.0;
        let tr = Rect::from_min_size(pos2(tx, tabs_top + 2.0), vec2(w, TAB_ROW - 2.0));
        let resp = ui.interact(tr, ui.scope_id().with(("ribbon_tab", i)), Sense::click());
        let selected = i == active;
        let contextual = context::is_contextual(tab.name);
        if contextual {
            // A contextual tab stands out in the accent colour, as on Windows.
            p.rect_filled(tr, egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, t.accent.gamma_multiply(0.22));
            p.hline(tr.x_range(), tr.top() + 1.0, Stroke::new(2.0, t.accent));
        }
        if selected && !app.ui.ribbon_minimized {
            p.rect_filled(tr, egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, t.chrome);
            if contextual {
                p.hline(tr.x_range(), tr.top() + 1.0, Stroke::new(2.0, t.accent));
            }
        } else if selected || resp.hovered() {
            p.rect_filled(tr, 2.0, t.control_hover.gamma_multiply(0.45));
        }
        let color = if contextual {
            t.accent
        } else if selected || resp.hovered() {
            t.text
        } else {
            t.text_dim
        };
        p.text(tr.center(), Align2::CENTER_CENTER, name, FontId::proportional(12.5), color);
        icons::describe_control(ui, &resp, tab.name, Some(selected), true);
        if resp.clicked() {
            if app.ui.ribbon_minimized {
                // A minimized ribbon opens again from any of its tabs, on that tab.
                app.ui.ribbon_tab = tab.name.to_string();
                app.ui.ribbon_minimized = false;
            } else if selected {
                app.ui.ribbon_minimized = true;
            } else {
                app.ui.ribbon_tab = tab.name.to_string();
            }
        }
        // A double-click's two clicks toggle twice; the double-click itself makes it one toggle.
        if resp.double_clicked() {
            app.ui.ribbon_minimized = !app.ui.ribbon_minimized;
        }
        tx += w + 1.0;
    }
    // The ribbon's minimize switch after the tabs.
    let mr = Rect::from_min_size(pos2(tx + 8.0, tabs_top + 5.0), vec2(30.0, 17.0));
    let mresp = ui.interact(mr, ui.scope_id().with("ribbon_min"), Sense::click());
    if mresp.hovered() {
        p.rect_filled(mr, 2.0, t.control_hover.gamma_multiply(0.5));
    }
    let box_r = Rect::from_min_size(pos2(mr.left() + 3.0, mr.center().y - 5.0), vec2(14.0, 10.0));
    p.rect_filled(box_r, 1.0, t.text);
    let tri = Rect::from_center_size(box_r.center(), vec2(6.0, 4.0));
    let (a, b2, c) = if app.ui.ribbon_minimized {
        (tri.left_top(), tri.right_top(), tri.center_bottom())
    } else {
        (tri.left_bottom(), tri.right_bottom(), tri.center_top())
    };
    p.add(egui::Shape::convex_polygon(vec![a, b2, c], t.chrome_dark, Stroke::NONE));
    arrow(&p, pos2(mr.right() - 6.0, mr.center().y), t.text_dim);
    icons::describe_control(ui, &mresp, "Minimize the ribbon", None, false);
    if mresp.on_hover_text(crate::tl!("Minimize the ribbon")).clicked() {
        app.ui.ribbon_minimized = !app.ui.ribbon_minimized;
    }
}

fn ribbon(app: &mut CadApp, ui: &mut egui::Ui, area: Rect, out: &mut Out) {
    let t = Tokens::get();
    let p = ui.painter().clone();
    let tabs_top = area.top();
    let body_top = tabs_top + TAB_ROW;
    let tabs = shown_tabs(app);
    let active = tabs.iter().position(|tab| tab.name == app.ui.ribbon_tab).unwrap_or(0);
    if !app.ui.ribbon_minimized {
        p.rect_filled(Rect::from_min_max(pos2(area.left(), body_top), area.max), 0.0, t.chrome);
        p.hline(area.x_range(), area.bottom() - 0.5, Stroke::new(1.0, t.border));
    }
    ribbon_tabs(app, ui, area, &tabs, active);
    if app.ui.ribbon_minimized {
        return;
    }
    let Some(tab) = tabs.get(active).copied() else { return };

    let (compacts, widths, small, collapsed) = fit_panels(&p, tab, area.width() - 8.0);
    let body = Rect::from_min_max(pos2(area.left(), body_top + 2.0), pos2(area.right(), area.bottom() - 2.0));
    let clip = p.with_clip_rect(body);
    let mut px = area.left() + 4.0;
    for (pi, panel) in tab.panels.iter().enumerate() {
        let is_collapsed = collapsed.get(pi).copied().unwrap_or(false);
        let pw = if is_collapsed { small.get(pi).copied().unwrap_or(60.0) } else { widths.get(pi).copied().unwrap_or(60.0) };
        if px > area.right() {
            break;
        }
        let prect = Rect::from_min_size(pos2(px, body.top()), vec2(pw, body.height()));
        // Panel separator on the right edge.
        clip.vline(prect.right() - 0.5, prect.y_range().shrink(2.0), Stroke::new(1.0, t.border));
        clip.vline(prect.right() + 0.5, prect.y_range().shrink(2.0), Stroke::new(1.0, t.tab_active.gamma_multiply(0.6)));
        let title_r = Rect::from_min_max(pos2(prect.left(), prect.bottom() - PANEL_TITLE), pos2(prect.right() - 1.0, prect.bottom()));
        let title_id = ui.scope_id().with(("ribbon_title", tab.name, pi));
        if is_collapsed {
            let br = Rect::from_min_max(prect.min + vec2(3.0, 2.0), pos2(prect.right() - 3.0, prect.bottom() - 2.0));
            let resp = ui.interact(br, title_id, Sense::click());
            if resp.hovered() {
                clip.rect_filled(br, 2.0, t.control_hover.gamma_multiply(0.55));
            }
            icons::paint(&clip, Rect::from_center_size(pos2(br.center().x, br.top() + 24.0), vec2(LARGE_ICON, LARGE_ICON)), first_icon(panel), false);
            clip.text(pos2(br.center().x, br.top() + 50.0), Align2::CENTER_TOP, crate::i18n::t(panel.title), label_font(), t.text);
            arrow(&clip, pos2(br.center().x, br.top() + 72.0), t.text);
            icons::describe_control(ui, &resp, &panel_name(panel.title), None, false);
            let mut tools = all_tools(panel);
            let more = slide_out(tab.name, panel.title);
            if !more.is_empty() {
                tools.push(SEP);
                tools.extend_from_slice(more);
            }
            egui::Popup::menu(&resp).show(|ui| {
                if let Some(c) = tool_menu(ui, &tools) {
                    out.command = Some(c);
                }
            });
            px += pw;
            continue;
        }
        // Panel title with its expand arrow; the title opens the panel's full command list.
        let tresp = ui.interact(title_r, title_id, Sense::click());
        if tresp.hovered() {
            clip.rect_filled(title_r.shrink2(vec2(2.0, 1.0)), 2.0, t.control_hover.gamma_multiply(0.45));
        }
        let title = crate::i18n::t(panel.title);
        let tw = text_w(&p, title);
        clip.text(pos2(title_r.center().x - 6.0, title_r.center().y), Align2::CENTER_CENTER, title, label_font(), t.text_dim);
        arrow(&clip, pos2(title_r.center().x - 6.0 + tw / 2.0 + 9.0, title_r.center().y), t.text_dim);
        icons::describe_control(ui, &tresp, &panel_name(panel.title), None, false);
        // The title opens the panel's slide-out, or for a panel without one, its commands.
        let more = slide_out(tab.name, panel.title);
        egui::Popup::menu(&tresp).show(|ui| {
            let tools = if more.is_empty() { all_tools(panel) } else { more.to_vec() };
            if let Some(c) = tool_menu(ui, &tools) {
                out.command = Some(c);
            }
        });

        let compact = compacts.get(pi).copied().unwrap_or(false);
        let mut cx = prect.left() + 5.0;
        let top = prect.top() + 3.0;
        let body_h = title_r.top() - top - 1.0;
        for (ci, col) in panel.cols.iter().enumerate() {
            let cw = col_width(&p, col, compact);
            let id = ui.scope_id().with(("ribbon", tab.name, pi, ci));
            match col {
                Large(btn) => large_button(app, ui, &clip, Rect::from_min_size(pos2(cx, top), vec2(cw - 3.0, body_h)), btn, id, out),
                Small(btns) => {
                    for (ri, btn) in btns.iter().enumerate().take(3) {
                        let r = Rect::from_min_size(pos2(cx, top + ri as f32 * (ROW + 1.0)), vec2(cw - 3.0, ROW));
                        small_button(ui, &clip, r, btn, !compact, id.with(ri), out);
                    }
                }
                Icons(btns) => {
                    for (ri, btn) in btns.iter().enumerate().take(3) {
                        let w = MINI + if btn.more.is_empty() { 0.0 } else { ARROW };
                        let r = Rect::from_min_size(pos2(cx, top + ri as f32 * (ROW + 1.0)), vec2(w, ROW));
                        small_button(ui, &clip, r, btn, false, id.with(ri), out);
                    }
                }
                Layers(rows, labelled) => {
                    let lr = Rect::from_min_size(pos2(cx, top + 1.0), vec2(cw - 8.0, ROW - 1.0));
                    match layer_list(app, ui, lr, id.with("list")) {
                        Some(LayerPick::Layer(name)) => {
                            out.prop = Some(Prop { key: "layer", value: json!(name), current: ("layer.current", json!({ "name": name })) });
                        }
                        Some(LayerPick::Toggle(params)) => out.run = Some(("layer.set", params)),
                        None => {}
                    }
                    for (ri, (row, last)) in rows.iter().zip(labelled.iter()).enumerate() {
                        let y = top + (ri as f32 + 1.0) * (ROW + 1.0);
                        let mut bx = cx;
                        for (k, btn) in row.iter().enumerate() {
                            let r = Rect::from_min_size(pos2(bx, y), vec2(MINI, ROW));
                            small_button(ui, &clip, r, btn, false, id.with((ri, k)), out);
                            bx += MINI;
                        }
                        let r = if compact {
                            Rect::from_min_size(pos2(bx, y), vec2(MINI, ROW))
                        } else {
                            Rect::from_min_max(pos2(bx + 2.0, y), pos2(cx + cw - 8.0, y + ROW))
                        };
                        small_button(ui, &clip, r, last, !compact, id.with((ri, "last")), out);
                    }
                }
                Props => {
                    props_lists(app, ui, Rect::from_min_size(pos2(cx, top), vec2(cw - 6.0, body_h)), id, out);
                }
            }
            cx += cw;
        }
        px += pw;
    }
}

fn large_button(app: &CadApp, ui: &mut egui::Ui, p: &egui::Painter, br: Rect, btn: &Btn, id: egui::Id, out: &mut Out) {
    let t = Tokens::get();
    let _ = app;
    let enabled = known(btn.cmd);
    let (a, b2) = large_lines(crate::i18n::t(btn.label));
    let color = if enabled { t.text } else { t.text_faint };
    let split = !btn.more.is_empty();
    // A split button runs its command from the icon and opens its list from the label and arrow.
    let icon_r = if split { Rect::from_min_max(br.min, pos2(br.right(), br.top() + LARGE_ICON + 8.0)) } else { br };
    let resp = hit(ui, icon_r, id, btn.label, btn.cmd, enabled);
    if resp.clicked() {
        out.command = Some(btn.cmd);
    }
    if split {
        let menu_r = Rect::from_min_max(pos2(br.left(), icon_r.bottom()), br.max);
        let mresp = hit(ui, menu_r, id.with("more"), btn.label, btn.cmd, enabled);
        if resp.hovered() || mresp.hovered() {
            p.rect_stroke(br, 2.0, Stroke::new(1.0, t.control_hover), egui::StrokeKind::Inside);
        }
        egui::Popup::menu(&mresp).show(|ui| {
            if let Some(c) = tool_menu(ui, btn.more) {
                out.command = Some(c);
            }
        });
    }
    icons::paint(p, Rect::from_center_size(pos2(br.center().x, br.top() + 4.0 + LARGE_ICON / 2.0), vec2(LARGE_ICON, LARGE_ICON)), btn.icon, !enabled);
    let y1 = br.top() + LARGE_ICON + 9.0;
    p.text(pos2(br.center().x, y1), Align2::CENTER_TOP, &a, label_font(), color);
    match (b2, split) {
        (Some(b2), false) => {
            p.text(pos2(br.center().x, y1 + 14.0), Align2::CENTER_TOP, b2, label_font(), color);
        }
        (Some(b2), true) => {
            let w = text_w(p, &b2);
            p.text(pos2(br.center().x - ARROW / 2.0, y1 + 14.0), Align2::CENTER_TOP, b2, label_font(), color);
            arrow(p, pos2(br.center().x - ARROW / 2.0 + w / 2.0 + 7.0, y1 + 21.0), color);
        }
        (None, true) => arrow(p, pos2(br.center().x, y1 + 20.0), color),
        (None, false) => {}
    }
}

fn small_button(ui: &mut egui::Ui, p: &egui::Painter, r: Rect, btn: &Btn, labelled: bool, id: egui::Id, out: &mut Out) {
    let t = Tokens::get();
    let enabled = known(btn.cmd);
    let split = !btn.more.is_empty();
    let main_r = if split { Rect::from_min_max(r.min, pos2(r.right() - ARROW, r.bottom())) } else { r };
    let resp = hit(ui, main_r, id, btn.label, btn.cmd, enabled);
    if resp.clicked() {
        out.command = Some(btn.cmd);
    }
    let icon_r = if labelled {
        Rect::from_min_size(pos2(r.left() + 4.0, r.center().y - SMALL_ICON / 2.0), vec2(SMALL_ICON, SMALL_ICON))
    } else {
        Rect::from_center_size(main_r.center(), vec2(SMALL_ICON + 2.0, SMALL_ICON + 2.0))
    };
    icons::paint(p, icon_r, btn.icon, !enabled);
    let color = if enabled { t.text } else { t.text_faint };
    if labelled {
        p.text(pos2(icon_r.right() + 6.0, r.center().y), Align2::LEFT_CENTER, crate::i18n::t(btn.label), label_font(), color);
    }
    if split {
        let ar = Rect::from_min_max(pos2(main_r.right(), r.top()), r.max);
        let aresp = hit(ui, ar, id.with("more"), btn.label, btn.cmd, enabled);
        arrow(p, ar.center(), color);
        egui::Popup::menu(&aresp).show(|ui| {
            if let Some(c) = tool_menu(ui, btn.more) {
                out.command = Some(c);
            }
        });
    }
}

/// A drop-down list box like the Windows ribbon's: value on the left, arrow on the right.
fn list_box(ui: &egui::Ui, r: Rect, id: egui::Id, tip: &str) -> egui::Response {
    let t = Tokens::get();
    let resp = ui.interact(r, id, Sense::click());
    ui.painter().rect_filled(r, 2.0, t.chrome_dark);
    ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, if resp.hovered() { t.accent } else { t.border }), egui::StrokeKind::Inside);
    arrow(ui.painter(), pos2(r.right() - 9.0, r.center().y), t.text);
    icons::describe_control(ui, &resp, tip, None, false);
    resp.on_hover_text(crate::i18n::t(tip))
}

/// The colour wheel beside the colour list.
fn paint_color_wheel(p: &egui::Painter, c: egui::Pos2, radius: f32) {
    let colors = [
        Color32::from_rgb(0xe8, 0x3a, 0x3a),
        Color32::from_rgb(0xf0, 0xa0, 0x30),
        Color32::from_rgb(0xf0, 0xe0, 0x40),
        Color32::from_rgb(0x50, 0xc0, 0x50),
        Color32::from_rgb(0x40, 0x90, 0xe8),
        Color32::from_rgb(0x90, 0x50, 0xd0),
    ];
    for (i, col) in colors.iter().enumerate() {
        let a0 = i as f32 / 6.0 * std::f32::consts::TAU;
        let a1 = (i as f32 + 1.0) / 6.0 * std::f32::consts::TAU;
        let pts = vec![
            c,
            c + radius * vec2(a0.cos(), a0.sin()),
            c + radius * vec2(((a0 + a1) / 2.0).cos(), ((a0 + a1) / 2.0).sin()),
            c + radius * vec2(a1.cos(), a1.sin()),
        ];
        p.add(egui::Shape::convex_polygon(pts, *col, Stroke::NONE));
    }
}

const COLORS: &[(&str, i64)] =
    &[("ByLayer", 256), ("ByBlock", 0), ("Red", 1), ("Yellow", 2), ("Green", 3), ("Cyan", 4), ("Blue", 5), ("Magenta", 6), ("White", 7)];
const LWEIGHTS: &[i64] = &[-1, -2, -3, 0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];

fn lweight_name(code: i64) -> String {
    match code {
        -1 => "ByLayer".into(),
        -2 => "ByBlock".into(),
        -3 => "Default".into(),
        c => format!("{:.2} mm", c as f64 / 100.0),
    }
}

/// Current colour, linetype and lineweight, each with its list (Properties panel).
fn props_lists(app: &CadApp, ui: &mut egui::Ui, r: Rect, id: egui::Id, out: &mut Out) {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else { return };
    // The selected objects' properties (none shown where they differ), else the current ones.
    let color = match selection_shares(app, |e| e.common.color) {
        Some(shared) => shared,
        None => Some(draftcraft_color::Color::from_aci(d.header.i64("CECOLOR", 256).clamp(0, 256) as i16)),
    };
    let cecolor = color.map(|c| i64::from(c.to_aci()));
    let celtype = match selection_shares(app, |e| e.common.linetype.clone()) {
        Some(shared) => shared,
        None => Some(d.header.str("CELTYPE", "ByLayer").to_string()),
    };
    let celweight = match selection_shares(app, |e| i64::from(e.common.lineweight.to_dxf())) {
        Some(shared) => shared,
        None => Some(d.header.i64("CELWEIGHT", -1)),
    };
    let linetypes: Vec<String> = std::iter::once("ByLayer".to_string())
        .chain(std::iter::once("ByBlock".to_string()))
        .chain(d.linetypes.iter().map(|l| l.name.clone()).filter(|n| !n.eq_ignore_ascii_case("bylayer") && !n.eq_ignore_ascii_case("byblock")))
        .collect();
    let p = ui.painter().clone();
    let list_x = r.left() + 26.0;
    let w = r.right() - list_x;
    for row in 0..3 {
        let y = r.top() + 1.0 + row as f32 * (ROW + 1.0);
        let lr = Rect::from_min_size(pos2(list_x, y), vec2(w, ROW - 1.0));
        let icon_c = pos2(r.left() + 11.0, lr.center().y);
        match row {
            0 => {
                paint_color_wheel(&p, icon_c, 8.0);
                let resp = list_box(ui, lr, id.with("color"), "Object Color");
                if let Some(color) = color {
                    let rgb = color.resolve(draftcraft_color::Color::Index(7), draftcraft_color::Color::Index(7));
                    let sw = Rect::from_min_size(pos2(lr.left() + 5.0, lr.center().y - 6.0), vec2(14.0, 12.0));
                    p.rect_filled(sw, 1.0, Color32::from_rgb(rgb.0, rgb.1, rgb.2));
                    let named = matches!(color, draftcraft_color::Color::True(_)).then(|| color.name());
                    let name = named
                        .or_else(|| COLORS.iter().find(|c| Some(c.1) == cecolor).map(|c| crate::i18n::t(c.0).to_string()))
                        .unwrap_or_else(|| color.name());
                    p.with_clip_rect(lr.shrink(2.0)).text(pos2(sw.right() + 6.0, lr.center().y), Align2::LEFT_CENTER, name, label_font(), t.text);
                }
                egui::Popup::menu(&resp).width(w).show(|ui| {
                    for (name, aci) in COLORS {
                        if ui.selectable_label(Some(*aci) == cecolor, crate::i18n::t(name)).clicked() {
                            out.prop = Some(Prop { key: "color", value: json!(name), current: ("color", json!({ "color": name })) });
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button(crate::tl!("More Colors...")).clicked() {
                        out.command = Some("color");
                        ui.close();
                    }
                });
            }
            1 => {
                for k in 0..3 {
                    p.hline(icon_c.x - 8.0..=icon_c.x + 8.0, icon_c.y - 5.0 + k as f32 * 5.0, Stroke::new(1.5, t.icon));
                }
                let resp = list_box(ui, lr, id.with("ltype"), "Linetype");
                if let Some(celtype) = &celtype {
                    p.hline(lr.left() + 6.0..=lr.left() + 50.0, lr.center().y, Stroke::new(1.0, t.text));
                    p.with_clip_rect(lr.shrink(2.0)).text(
                        pos2(lr.right() - 18.0, lr.center().y),
                        Align2::RIGHT_CENTER,
                        crate::i18n::t(celtype),
                        label_font(),
                        t.text,
                    );
                }
                egui::Popup::menu(&resp).width(w).show(|ui| {
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for name in &linetypes {
                            if ui.selectable_label(celtype.as_ref().is_some_and(|c| name.eq_ignore_ascii_case(c)), crate::i18n::t(name)).clicked() {
                                out.prop = Some(Prop { key: "linetype", value: json!(name), current: ("linetype", json!({ "current": name })) });
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button(crate::tl!("Other...")).clicked() {
                        out.command = Some("linetype");
                        ui.close();
                    }
                });
            }
            _ => {
                for k in 0..3 {
                    p.hline(icon_c.x - 8.0..=icon_c.x + 8.0, icon_c.y - 5.0 + k as f32 * 5.0, Stroke::new(0.8 + k as f32, t.icon));
                }
                let resp = list_box(ui, lr, id.with("lweight"), "Lineweight");
                if let Some(celweight) = celweight {
                    p.hline(lr.left() + 6.0..=lr.left() + 50.0, lr.center().y, Stroke::new(1.0, t.text));
                    p.with_clip_rect(lr.shrink(2.0)).text(
                        pos2(lr.right() - 18.0, lr.center().y),
                        Align2::RIGHT_CENTER,
                        crate::i18n::t(&lweight_name(celweight)),
                        label_font(),
                        t.text,
                    );
                }
                egui::Popup::menu(&resp).width(w).show(|ui| {
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for code in LWEIGHTS {
                            if ui.selectable_label(Some(*code) == celweight, crate::i18n::t(&lweight_name(*code))).clicked() {
                                let v = match code {
                                    -1 => json!("ByLayer"),
                                    -2 => json!("ByBlock"),
                                    -3 => json!("Default"),
                                    c => json!(*c as f64 / 100.0),
                                };
                                out.prop = Some(Prop { key: "lineweight", value: v.clone(), current: ("lweight", json!({ "lineweight": v })) });
                                ui.close();
                            }
                        }
                    });
                });
            }
        }
    }
}

/// What the layer list's menu was clicked for.
#[derive(Debug, PartialEq)]
enum LayerPick {
    /// A layer's name: the selection moves to it, or it becomes the current layer.
    Layer(String),
    /// A layer's on, frozen or locked state switched: `layer.set` parameters.
    Toggle(serde_json::Value),
}

/// The left edges of a layer row's on, freeze and lock icons, 16 points wide each.
fn layer_icon_xs(r: Rect) -> [f32; 3] {
    let x = r.left() + 4.0;
    [x, x + 18.0, x + 36.0]
}

/// The state a click at `x` in a layer row switches, as `layer.set` parameters, or `None` when it
/// is not on one of the icons (the click then picks the layer).
fn layer_toggle_at(r: Rect, x: f32, (name, on, frozen, locked, _): &(String, bool, bool, bool, Color32)) -> Option<serde_json::Value> {
    let hit = layer_icon_xs(r).iter().position(|&left| (left - 1.0..left + 17.0).contains(&x))?;
    Some(match hit {
        0 => json!({ "name": name, "on": !on }),
        1 => json!({ "name": name, "frozen": !frozen }),
        _ => json!({ "name": name, "locked": !locked }),
    })
}

/// The current-layer list: the layer's state and colour, its name, and a menu of all layers.
/// In the menu, the bulb, sun and lock switch the layer's state, as in AutoCAD's layer control,
/// and the rest of a row picks the layer.
fn layer_list(app: &CadApp, ui: &mut egui::Ui, rect: Rect, id: egui::Id) -> Option<LayerPick> {
    let t = Tokens::get();
    let p = ui.painter().clone();
    let Ok(d) = app.session.doc() else {
        p.rect_filled(rect, 2.0, t.chrome_dark);
        return None;
    };
    // The selected objects' layer (none when they differ), else the current one.
    let cur = match selection_shares(app, |e| e.common.layer.clone()) {
        Some(shared) => shared.unwrap_or_default(),
        None => d.header.str("CLAYER", "0").to_string(),
    };
    let layers: Vec<(String, bool, bool, bool, Color32)> = d
        .layers
        .iter()
        .map(|l| {
            let rgb = l.color.resolve(draftcraft_color::Color::Index(7), draftcraft_color::Color::Index(7));
            (l.name.clone(), l.on, l.frozen, l.locked, Color32::from_rgb(rgb.0, rgb.1, rgb.2))
        })
        .collect();
    let resp = list_box(ui, rect, id, "Layer");
    let row = |p: &egui::Painter, r: Rect, (name, on, frozen, locked, color): &(String, bool, bool, bool, Color32)| {
        let icons_at = [
            if *on { Icon::Bulb } else { Icon::BulbOff },
            if *frozen { Icon::Snowflake } else { Icon::Sun },
            if *locked { Icon::Lock } else { Icon::Unlock },
        ];
        for (icon, x) in icons_at.into_iter().zip(layer_icon_xs(r)) {
            icons::paint(p, Rect::from_min_size(pos2(x, r.center().y - 8.0), vec2(16.0, 16.0)), icon, false);
        }
        let x = layer_icon_xs(r)[2] + 18.0;
        let sw = Rect::from_min_size(pos2(x + 2.0, r.center().y - 6.0), vec2(12.0, 12.0));
        p.rect_filled(sw, 1.0, *color);
        p.rect_stroke(sw, 1.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        p.text(pos2(sw.right() + 6.0, r.center().y), Align2::LEFT_CENTER, name, label_font(), t.text);
    };
    if let Some(info) = layers.iter().find(|l| l.0.eq_ignore_ascii_case(&cur)) {
        row(&p.with_clip_rect(rect.shrink(1.0).with_max_x(rect.right() - 18.0)), rect, info);
    }
    let mut picked = None;
    egui::Popup::menu(&resp).width(rect.width()).show(|ui| {
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for l in &layers {
                let (r, item) = ui.allocate_exact_size(vec2(rect.width(), 22.0), Sense::click());
                if item.hovered() {
                    ui.painter().rect_filled(r, 2.0, t.control_hover.gamma_multiply(0.7));
                } else if l.0.eq_ignore_ascii_case(&cur) {
                    ui.painter().rect_filled(r, 2.0, t.select_bg.gamma_multiply(0.6));
                }
                row(ui.painter(), r, l);
                crate::control::record_widget(ui, &item, &l.0, "button", None);
                if item.clicked() {
                    // On an icon the menu stays open, so several layers can be switched in a row.
                    match item.interact_pointer_pos().and_then(|pos| layer_toggle_at(r, pos.x, l)) {
                        Some(params) => picked = Some(LayerPick::Toggle(params)),
                        None => {
                            picked = Some(LayerPick::Layer(l.0.clone()));
                            ui.close();
                        }
                    }
                }
            }
        });
    });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer_of(app: &CadApp, h: draftcraft_engine::doc::Handle) -> String {
        app.session.doc().unwrap().entity(h).unwrap().common.layer.clone()
    }

    /// The ribbon's layer list moves the selected objects to the layer picked, leaving the
    /// current layer alone; with nothing selected it sets the current layer. Objects on locked
    /// layers stay where they are, and the command line says so.
    #[test]
    fn the_layer_list_moves_the_selection_to_the_layer() {
        let mut app = CadApp::new(draftcraft_engine::Session::new(), crate::Services::default());
        app.session.cmdline("line 0,0 10,0").unwrap();
        app.session.cmdline("").unwrap();
        app.session.cmdline("circle 50,50 5").unwrap();
        let d = app.session.doc_mut().unwrap();
        d.ensure_layer("Walls");
        d.ensure_layer("Doors");
        let hs: Vec<_> = d.model.iter().map(|e| e.handle).collect();
        let (line, circle) = (hs[0], hs[1]);
        let pick = |name: &str| Prop { key: "layer", value: json!(name), current: ("layer.current", json!({ "name": name })) };
        // Both selected: both move, CLAYER stays 0, the selection stays.
        app.session.set_selection(vec![line, circle]);
        assert_eq!(selection_shares(&app, |e| e.common.layer.clone()), Some(Some("0".to_string())));
        set_property(&mut app, pick("Walls"));
        assert_eq!((layer_of(&app, line), layer_of(&app, circle)), ("Walls".into(), "Walls".into()));
        assert_eq!(app.session.doc().unwrap().header.str("CLAYER", "0"), "0");
        assert_eq!(app.session.selection(), vec![line, circle]);
        // One of them on another layer: the list shows none.
        app.session.set_selection(vec![line]);
        set_property(&mut app, pick("Doors"));
        app.session.set_selection(vec![line, circle]);
        assert_eq!(selection_shares(&app, |e| e.common.layer.clone()), Some(None));
        // Nothing selected: the current layer changes, no object does.
        app.session.set_selection(Vec::new());
        assert_eq!(selection_shares(&app, |e| e.common.layer.clone()), None);
        set_property(&mut app, pick("Walls"));
        assert_eq!(app.session.doc().unwrap().header.str("CLAYER", "0"), "Walls");
        assert_eq!(layer_of(&app, line), "Doors");
        // On a locked layer: left alone, and said.
        if let Some(l) = app.session.doc_mut().unwrap().layer_mut("Doors") {
            l.locked = true;
        }
        app.session.set_selection(vec![line, circle]);
        set_property(&mut app, pick("0"));
        assert_eq!((layer_of(&app, line), layer_of(&app, circle)), ("Doors".into(), "0".into()));
        assert!(app.session.log.iter().any(|m| m == "1 object was on a locked layer."), "{:?}", app.session.log);
    }

    /// In the layer list's menu, the bulb, sun and lock switch that layer's state; the rest of the
    /// row picks the layer.
    #[test]
    fn the_layer_menu_icons_switch_the_layer_state() {
        let r = Rect::from_min_size(pos2(100.0, 50.0), vec2(220.0, 22.0));
        let layer = ("Walls".to_string(), true, false, false, Color32::WHITE);
        let [on, freeze, lock] = layer_icon_xs(r);
        assert_eq!(layer_toggle_at(r, on + 8.0, &layer), Some(json!({ "name": "Walls", "on": false })));
        assert_eq!(layer_toggle_at(r, freeze + 8.0, &layer), Some(json!({ "name": "Walls", "frozen": true })));
        assert_eq!(layer_toggle_at(r, lock + 8.0, &layer), Some(json!({ "name": "Walls", "locked": true })));
        let off = ("Walls".to_string(), false, true, true, Color32::WHITE);
        assert_eq!(layer_toggle_at(r, on + 8.0, &off), Some(json!({ "name": "Walls", "on": true })));
        assert_eq!(layer_toggle_at(r, freeze + 8.0, &off), Some(json!({ "name": "Walls", "frozen": false })));
        assert_eq!(layer_toggle_at(r, lock + 8.0, &off), Some(json!({ "name": "Walls", "locked": false })));
        // The swatch and the name pick the layer.
        assert_eq!(layer_toggle_at(r, lock + 30.0, &layer), None);
        assert_eq!(layer_toggle_at(r, r.right() - 10.0, &layer), None);
        // And layer.set does switch it.
        let mut app = CadApp::new(draftcraft_engine::Session::new(), crate::Services::default());
        app.session.doc_mut().unwrap().ensure_layer("Walls");
        for params in
            [json!({ "name": "Walls", "on": false }), json!({ "name": "Walls", "frozen": true }), json!({ "name": "Walls", "locked": true })]
        {
            app.run("layer.set", params).unwrap();
        }
        let d = app.session.doc().unwrap();
        let l = d.layers.iter().find(|l| l.name == "Walls").unwrap();
        assert_eq!((l.on, l.frozen, l.locked), (false, true, true));
    }

    /// The colour, linetype and lineweight lists change the selected objects too.
    #[test]
    fn the_property_lists_change_the_selection() {
        let mut app = CadApp::new(draftcraft_engine::Session::new(), crate::Services::default());
        app.session.cmdline("circle 0,0 5").unwrap();
        let h = app.session.doc().unwrap().model.last().unwrap().handle;
        app.session.set_selection(vec![h]);
        set_property(&mut app, Prop { key: "color", value: json!("Red"), current: ("color", json!({ "color": "Red" })) });
        set_property(&mut app, Prop { key: "lineweight", value: json!(0.5), current: ("lweight", json!({ "lineweight": 0.5 })) });
        set_property(&mut app, Prop { key: "linetype", value: json!("ByBlock"), current: ("linetype", json!({ "current": "ByBlock" })) });
        let d = app.session.doc().unwrap();
        let e = d.entity(h).unwrap();
        assert_eq!(e.common.color, draftcraft_color::Color::Index(1));
        assert_eq!(e.common.lineweight.to_dxf(), 50);
        assert!(e.common.linetype.eq_ignore_ascii_case("ByBlock"));
        // The current properties stay as they were.
        assert_eq!(d.header.i64("CECOLOR", 256), 256);
        assert_eq!(d.header.i64("CELWEIGHT", -1), -1);
        assert_eq!(selection_shares(&app, |e| e.common.color), Some(Some(draftcraft_color::Color::Index(1))));
    }

    #[test]
    fn every_ribbon_command_exists() {
        let mut missing = Vec::new();
        for tab in TABS.iter().chain(context::CONTEXT_TABS) {
            for panel in tab.panels {
                for (_, cmd, _) in all_tools(panel) {
                    if !known(cmd) {
                        missing.push(format!("{}/{}: {cmd}", tab.name, panel.title));
                    }
                }
            }
        }
        for (tab, panel, tools) in SLIDE_OUTS {
            assert!(TABS.iter().any(|t| t.name == *tab && t.panels.iter().any(|p| p.title == *panel)), "no panel {tab}/{panel}");
            for (_, cmd, _) in tools.iter().filter(|t| t.1 != SEP.1) {
                if !known(cmd) {
                    missing.push(format!("{tab}/{panel} slide-out: {cmd}"));
                }
            }
        }
        for (_, cmd, _) in QAT.iter().chain(APP_MENU) {
            if !known(cmd) {
                missing.push(format!("QAT/app menu: {cmd}"));
            }
        }
        assert!(missing.is_empty(), "unknown ribbon commands: {missing:?}");
    }

    #[test]
    fn long_large_labels_split_in_two_lines() {
        assert_eq!(large_lines("Line"), ("Line".to_string(), None));
        assert_eq!(large_lines("Layer Properties"), ("Layer".to_string(), Some("Properties".to_string())));
        assert_eq!(large_lines("Match Properties"), ("Match".to_string(), Some("Properties".to_string())));
    }

    #[test]
    fn the_windows_layout_is_the_default() {
        let app = crate::CadApp::new(draftcraft_engine::Session::default(), Default::default());
        assert!(app.ui.show_toolbar, "the ribbon is shown");
        assert!(!app.ui.show_toolsets && !app.ui.show_palettes, "no Tool Sets or docked palettes");
        assert_eq!(app.ui.ribbon_tab, "Home");
    }

    #[test]
    fn lineweight_names() {
        assert_eq!(lweight_name(-1), "ByLayer");
        assert_eq!(lweight_name(25), "0.25 mm");
    }
}
