//! The small sections every DWG file carries whatever the drawing: the application information
//! (AcDb:AppInfo and AcDb:AppInfoHistory), the revision history, and the header variables' default
//! values, in the forms AutoCAD 2027 writes them.

/// Default values of the header variables for a drawing that has none
/// (AutoCAD's own defaults, as an imperial drawing carries them).
pub(crate) fn default_text(name: &str) -> &'static str {
    match name {
        "UNKNOWN_BD_412148564080" => "412148564080",
        "UNKNOWN_BD_1A" | "UNKNOWN_BD_1B" | "UNKNOWN_BD_1C" => "1",
        "UNKNOWN_TV_1" => "\"m\"",
        // The spec gives 24; AutoCAD writes 0 in every file of the corpus.
        "UNKNOWN_BL_24" => "0",
        // Undocumented longs AutoCAD writes with the same values in every
        // file of the corpus, 2004 to 2018, whatever the drawing.
        "UNKNOWN_BL_R2004_1" | "UNKNOWN_BL_R2004_6" => "808464432",
        "UNKNOWN_BL_R2004_2" | "UNKNOWN_BL_R2004_5" => "486539264",
        "UNKNOWN_BL_R2004_3" | "UNKNOWN_BL_R2004_4" => "1295003696",
        "MENUNAME" => "\".\"",
        "DIMASO" | "DIMSHO" | "REGENMODE" | "FILLMODE" | "PSLTSCALE" | "USRTIMER" | "WORLDVIEW" | "TILEMODE" | "VISRETAIN" | "DIMTIH" | "DIMTOH"
        | "CAMERADISPLAY" => "",
        "PROXYGRAPHICS" | "ATTMODE" | "SHOWHIST" | "LOFTNORMALS" | "LIGHTGLYPHDISPLAY" | "TILEMODELIGHTSYNCH" | "HIDETEXT" | "DIMTOLJ" => "1",
        "TREEDEPTH" => "3020",
        "LUNITS" | "DIMALTD" | "DIMALTU" | "DIMALTTD" | "DIMLUNIT" | "XCLIPFRAME" | "DIMASSOC" | "DWFFRAME" => "2",
        "LUPREC" | "DIMDEC" | "DIMTDEC" => "4",
        "SPLINESEGS" | "ISOLINES" => "8",
        "SURFU" | "SURFV" | "SURFTYPE" | "SURFTAB1" | "SURFTAB2" | "SPLINETYPE" => "6",
        "SHADEDGE" | "DIMATFIT" => "3",
        "SHADEDIF" | "TSTACKSIZE" => "70",
        "MAXACTVP" => "64",
        "TEXTQLTY" => "50",
        "LTSCALE" | "CMLSCALE" | "CELTSCALE" | "DIMSCALE" | "DIMLFAC" | "DIMTFAC" | "DIMFXL" | "UNKNOWN_BD_R2007_1" => "1",
        "TEXTSIZE" => "0.2",
        "TRACEWID" => "0.05",
        "SKETCHINC" => "0.1",
        "FACETRES" => "0.5",
        "PEXTMIN" | "EXTMIN" => "(100000000000000000000, 100000000000000000000, 100000000000000000000)",
        "PEXTMAX" | "EXTMAX" => "(-100000000000000000000, -100000000000000000000, -100000000000000000000)",
        "PLIMMAX" | "LIMMAX" => "(12, 9)",
        "PUCSXDIR" | "UCSXDIR" => "(1, 0, 0)",
        "PUCSYDIR" | "UCSYDIR" => "(0, 1, 0)",
        "DIMASZ" | "DIMEXE" | "DIMTXT" => "0.18",
        "DIMEXO" => "0.0625",
        "DIMDLI" => "0.38",
        "DIMCEN" | "DIMGAP" => "0.09",
        "DIMALTF" => "25.4",
        "DIMJOGANG" => "0.7853981633974483",
        "DIMDSEP" => "46",
        "DIMLWD" | "DIMLWE" => "-2",
        "DIMALTMZF" | "DIMMZF" => "100",
        "FLAGS" => "10781",
        "SORTENTS" => "127",
        "OBSCUREDCOLOR" | "INTERSECTIONCOLOR" => "257",
        "TSTACKALIGN" => "1",
        "UNKNOWN_BL_R2007_2" => "10",
        "STEPSPERSEC" | "3DDWFPREC" => "2",
        "STEPSIZE" => "6",
        "LENSLENGTH" => "50",
        "PSOLWIDTH" => "0.25",
        "PSOLHEIGHT" => "4",
        "LOFTANG1" | "LOFTANG2" => "1.5707963267948966",
        "LOFTPARAM" => "7",
        "LATITUDE" => "37.795",
        "LONGITUDE" => "-122.394",
        "TIMEZONE" => "-8000",
        "UNKNOWN_B_R2007" => "true",
        _ => "",
    }
}

/// The maintenance release AutoCAD 2027 writes for AC1032 (file header,
/// auxiliary header, class records), whose object layout the writer
/// follows bit for bit.
pub(crate) const ACAD_MAINTENANCE: u32 = 427;

/// AcDb:RevHistory (chapter 18), as AutoCAD 2027 writes it for a drawing
/// with no revision records.
pub(crate) fn rev_history() -> Vec<u8> {
    [0u32, 0, 1, 0].iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// The kinds of entry in an application information list, each written
/// before its text. The spec calls these 16 bytes a checksum that may be
/// zero; in AutoCAD's files they are fixed identifiers, the same in every
/// file of the corpus whatever the text (the property set's is the format
/// id its XML names).
const APPINFO_COMMENT: [u8; 16] = [0xB8, 0xD0, 0xF0, 0x25, 0xA1, 0xD7, 0x93, 0x49, 0xB2, 0xFA, 0x9B, 0xF9, 0x28, 0x6F, 0xA1, 0xFD];

const APPINFO_PROPERTIES: [u8; 16] = [0xE0, 0x85, 0x9F, 0xF2, 0xF9, 0x4F, 0x68, 0x10, 0xAB, 0x91, 0x08, 0x00, 0x2B, 0x27, 0xB3, 0xD9];

const APPINFO_PRODUCT: [u8; 16] = [0xE8, 0xE0, 0x96, 0x51, 0xC5, 0xCE, 0xB2, 0x44, 0xA8, 0xBF, 0xF6, 0xE8, 0x3B, 0x85, 0x9D, 0x44];

/// The 32 bytes AcDb:AppInfoHistory starts with, the same in every file of
/// the corpus (2004 to 2018, every save).
const APPINFO_HISTORY_START: [u8; 32] = [
    0x53, 0xDE, 0x38, 0x1D, 0xEC, 0x43, 0x21, 0xCA, 0x96, 0x19, 0xE1, 0xE2, 0x17, 0x1A, 0x2A, 0x67, 0x3B, 0xD9, 0x7F, 0xF7, 0x3C, 0xBB, 0xCE, 0x08,
    0xA0, 0x53, 0xD8, 0xED, 0xD2, 0x8D, 0xC5, 0xC7,
];

/// TU as the R2007+ small sections store it: RS length counting the
/// terminating 0, then UTF-16 units with that 0.
fn tu(out: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().filter(|&u| u != 0).chain([0]).collect();
    out.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
}

fn appinfo_comment(name: &str, version: &str) -> String {
    format!("Saved by {name} {version}.")
}

fn appinfo_product(name: &str, version: &str) -> String {
    let registry: String = version.split('.').take(2).collect::<Vec<_>>().join(".");
    format!(
        "<ProductInformation name =\"{name}\" build_version=\"{version}\" registry_version=\"{registry}\" install_id_string=\"{name}\" registry_localeID=\"1033\"/>"
    )
}

/// AcDb:AppInfo (chapter 16) in the layout AutoCAD 2027 writes: format 3,
/// "AppInfoDataList", two entries (a comment and the product
/// information), each after the identifier of its kind. It names the
/// program writing the file, never claims to be an Autodesk application.
pub(crate) fn app_info(name: &str, version: &str) -> Vec<u8> {
    let mut o = Vec::new();
    o.extend_from_slice(&3u32.to_le_bytes());
    tu(&mut o, "AppInfoDataList");
    o.extend_from_slice(&2u32.to_le_bytes());
    o.extend_from_slice(&APPINFO_COMMENT);
    tu(&mut o, &appinfo_comment(name, version));
    o.extend_from_slice(&APPINFO_PRODUCT);
    tu(&mut o, &appinfo_product(name, version));
    o
}

/// AcDb:AppInfoHistory (not in the spec) from a ready property set: the fixed 32 bytes, a
/// zero long, then an application information list of three entries (the comment, the property
/// set and the product information).
pub(crate) fn app_info_history_with(props: &str, name: &str, version: &str) -> Vec<u8> {
    let mut o = APPINFO_HISTORY_START.to_vec();
    o.extend_from_slice(&0u32.to_le_bytes());
    tu(&mut o, "AppInfoDataList");
    o.extend_from_slice(&3u32.to_le_bytes());
    o.extend_from_slice(&APPINFO_COMMENT);
    tu(&mut o, &appinfo_comment(name, version));
    o.extend_from_slice(&APPINFO_PROPERTIES);
    tu(&mut o, props);
    o.extend_from_slice(&APPINFO_PRODUCT);
    tu(&mut o, &appinfo_product(name, version));
    o
}
