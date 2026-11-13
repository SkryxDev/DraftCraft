//! ATTOUT / ATTIN: attribute values out to a tab-delimited file and back in by handle.

use std::path::PathBuf;

use draftcraft_doc::{EntityKind, Handle};
use draftcraft_engine::{Input, Session};
use serde_json::{Value, json};

/// Blocks DOOR (tags NUMBER, TYPE) and WINDOW (tags NUMBER, WIDTH), two doors, a window, a line.
fn drawing() -> (Session, [String; 4]) {
    let mut s = Session::new();
    block(&mut s, "DOOR", &["NUMBER", "TYPE"]);
    block(&mut s, "WINDOW", &["Number", "WIDTH"]);
    let d1 = insert(&mut s, "DOOR", 0.0, json!({"NUMBER": "D1", "TYPE": "Single"}));
    let d2 = insert(&mut s, "DOOR", 10.0, json!({"NUMBER": "D2", "TYPE": "Double"}));
    let w1 = insert(&mut s, "WINDOW", 20.0, json!({"NUMBER": "W1", "WIDTH": "900"}));
    let r = s.execute("line", &json!({"points": [[0, -5], [10, -5]]})).unwrap();
    let line = r["handle"].as_str().or_else(|| r["handles"][0].as_str()).unwrap().to_string();
    (s, [d1, d2, w1, line])
}

fn block(s: &mut Session, name: &str, tags: &[&str]) {
    for (i, t) in tags.iter().enumerate() {
        s.execute("attdef", &json!({"tag": t, "default": "", "at": [0, i as f64], "height": 0.5})).unwrap();
    }
    s.execute("circle", &json!({"center": [0, 0], "radius": 1})).unwrap();
    let hs: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    s.execute("block", &json!({"name": name, "base": [0, 0], "handles": hs, "keep": "delete"})).unwrap();
}

fn insert(s: &mut Session, name: &str, x: f64, attribs: Value) -> String {
    s.execute("insert", &json!({"name": name, "at": [x, 5], "attribs": attribs})).unwrap()["handle"].as_str().unwrap().to_string()
}

/// The attribute values of block reference `h`, by tag.
fn values(s: &Session, h: &str) -> Vec<(String, String)> {
    let d = s.doc().unwrap();
    let EntityKind::Insert(i) = &d.entity(Handle::parse_hex(h).unwrap()).unwrap().kind else { panic!("not a block reference") };
    i.attribs.iter().map(|a| (a.tag.clone(), a.text.value.clone())).collect()
}

fn value(s: &Session, h: &str, tag: &str) -> String {
    values(s, h).into_iter().find(|(t, _)| t.eq_ignore_ascii_case(tag)).unwrap().1
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("draftcraft-attio-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn attin_text(s: &mut Session, text: &str) -> Value {
    s.execute("attin", &json!({ "text": text })).unwrap()
}

#[test]
fn attout_writes_handles_names_and_the_union_of_tags() {
    let (mut s, [d1, d2, w1, line]) = drawing();
    let path = scratch("out.txt");
    let r = s.execute("attout", &json!({"path": path.to_string_lossy(), "handles": [d1, w1, line, d2]})).unwrap();
    assert_eq!(r["references"], 3, "{r}");
    assert_eq!(r["tags"], json!(["NUMBER", "TYPE", "WIDTH"]));
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"\xEF\xBB\xBF"), "UTF-8 with a byte order mark");
    let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
    let expected =
        format!("HANDLE\tBLOCKNAME\tNUMBER\tTYPE\tWIDTH\r\n'{d1}\tDOOR\tD1\tSingle\t\r\n'{w1}\tWINDOW\tW1\t\t900\r\n'{d2}\tDOOR\tD2\tDouble\t\r\n");
    assert_eq!(text, expected);
    // Without a path the text comes back; without handles, every reference in the space.
    let r = s.execute("attout", &json!({})).unwrap();
    assert_eq!(r["references"], 3);
    assert!(r["text"].as_str().unwrap().contains(&format!("'{w1}\tWINDOW")));
    // Nothing with attributes is an error, not an empty file.
    assert!(s.execute("attout", &json!({"handles": [line]})).is_err());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn round_trip_attout_edit_attin_and_undo_in_one_step() {
    let (mut s, [d1, d2, w1, _]) = drawing();
    let path = scratch("roundtrip.txt");
    let ps = path.to_string_lossy().to_string();
    s.execute("attout", &json!({"path": ps})).unwrap();
    let before: Vec<_> = [&d1, &d2, &w1].iter().map(|h| values(&s, h)).collect();

    // Unchanged, it changes nothing.
    let r = s.execute("attin", &json!({"path": ps})).unwrap();
    assert_eq!((r["references"].as_u64(), r["values"].as_u64(), r["skipped"].as_u64()), (Some(0), Some(0), Some(0)), "{r}");

    // Edited as a spreadsheet would: CRLF kept, a value with a tab and quotes quoted.
    let text = String::from_utf8(std::fs::read(&path).unwrap()).unwrap();
    let edited = text
        .replace("\tSingle\t", "\t\"Fire \"\"rated\"\"\tA\"\t")
        .replace("\t900\r\n", "\t1200\r\n")
        .replace(&format!("'{d2}\tDOOR\tD2"), &format!("'{d2}\tDOOR\tD2-bis"));
    std::fs::write(&path, edited).unwrap();
    let r = s.execute("attin", &json!({"path": ps})).unwrap();
    assert_eq!(r["references"], 3, "{r}");
    assert_eq!(r["values"], 3);
    assert_eq!(r["skipped"], 0);
    assert!(r["message"].as_str().unwrap().contains("3 block references updated (3 attribute values)"), "{r}");
    assert_eq!(value(&s, &d1, "TYPE"), "Fire \"rated\"\tA");
    assert_eq!(value(&s, &d2, "NUMBER"), "D2-bis");
    assert_eq!(value(&s, &w1, "WIDTH"), "1200");
    // Its own output reads back the same.
    let again = s.execute("attout", &json!({})).unwrap()["text"].as_str().unwrap().to_string();
    let r = attin_text(&mut s, &again);
    assert_eq!(r["values"], 0, "{r}");

    // One undo step takes the whole import back; redo brings it again.
    s.undo().unwrap();
    let after: Vec<_> = [&d1, &d2, &w1].iter().map(|h| values(&s, h)).collect();
    assert_eq!(after, before);
    s.redo().unwrap();
    assert_eq!(value(&s, &w1, "WIDTH"), "1200");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn unknown_handles_and_other_objects_are_skipped() {
    let (mut s, [d1, _, _, line]) = drawing();
    let text = format!("HANDLE\tBLOCKNAME\tNUMBER\n'FFFFF\tDOOR\tX\n\tDOOR\tY\n'{line}\tLINE\tZ\nnot-a-handle\tDOOR\tQ\n'{d1}\tDOOR\tD1-new\n");
    let r = attin_text(&mut s, &text);
    assert_eq!(r["skipped"], 4, "{r}");
    assert_eq!(r["references"], 1);
    assert_eq!(value(&s, &d1, "NUMBER"), "D1-new");
    // Rows outside `handles` are skipped too.
    let (mut s, [d1, d2, _, _]) = drawing();
    let text = format!("HANDLE\tNUMBER\n'{d1}\tA\n'{d2}\tB\n");
    let r = s.execute("attin", &json!({"text": text, "handles": [d2]})).unwrap();
    assert_eq!((r["references"].as_u64(), r["skipped"].as_u64()), (Some(1), Some(1)), "{r}");
    assert_eq!(value(&s, &d1, "NUMBER"), "D1");
    assert_eq!(value(&s, &d2, "NUMBER"), "B");
}

#[test]
fn tags_missing_from_the_file_or_the_block_are_left_alone() {
    let (mut s, [d1, d2, w1, _]) = drawing();
    // No TYPE column; a COLOR column no block has; `<>` and a short row leave values alone; tags
    // match in any case; the columns may come in any order.
    let text = format!("number\tHandle\tCOLOR\tWIDTH\nN1\t'{d1}\tred\t<>\n<>\t{w1}\tblue\t600\nN2\t'{d2}\n");
    let r = attin_text(&mut s, &text);
    assert_eq!(r["unknownTags"], json!(["COLOR"]), "{r}");
    assert_eq!(value(&s, &d1, "NUMBER"), "N1");
    assert_eq!(value(&s, &d1, "TYPE"), "Single");
    assert_eq!(value(&s, &w1, "NUMBER"), "W1");
    assert_eq!(value(&s, &w1, "WIDTH"), "600");
    assert_eq!(value(&s, &d2, "NUMBER"), "N2");
    assert_eq!(value(&s, &d2, "TYPE"), "Double");
    // An empty cell clears a value the block has.
    attin_text(&mut s, &format!("HANDLE\tTYPE\n'{d1}\t\n"));
    assert_eq!(value(&s, &d1, "TYPE"), "");
}

#[test]
fn references_on_locked_layers_are_left_alone_and_reported() {
    let (mut s, [d1, d2, _, _]) = drawing();
    s.execute("layer.new", &json!({"name": "Locked"})).unwrap();
    s.execute("layer.set", &json!({"name": "Locked", "locked": true})).unwrap();
    s.doc_mut().unwrap().modify_entity(Handle::parse_hex(&d1).unwrap(), |e| e.common.layer = "Locked".into()).unwrap();
    let r = attin_text(&mut s, &format!("HANDLE\tNUMBER\n'{d1}\tX\n'{d2}\tY\n"));
    assert_eq!((r["references"].as_u64(), r["locked"].as_u64()), (Some(1), Some(1)), "{r}");
    assert!(r["message"].as_str().unwrap().contains("locked"), "{r}");
    assert_eq!(value(&s, &d1, "NUMBER"), "D1");
    assert_eq!(value(&s, &d2, "NUMBER"), "Y");
}

#[test]
fn hostile_files_are_refused_or_skipped_without_panicking() {
    let (mut s, [d1, ..]) = drawing();
    let snapshot = values(&s, &d1);
    for bad in [
        String::new(),
        "\u{feff}".to_string(),
        "no handle column\n'1\tx\n".to_string(),
        format!("HANDLE\tNUMBER\n'{d1}\t\"never closed\n"),
        format!("HANDLE\tNUMBER\n'{d1}\t{}\n", "x".repeat(70_000)),
        format!("HANDLE{}\n", "\tT".repeat(5_000)),
    ] {
        assert!(s.execute("attin", &json!({"text": bad})).is_err(), "{:.80}", bad);
        assert_eq!(values(&s, &d1), snapshot);
    }
    // Garbage that parses is skipped row by row.
    for junk in ["HANDLE\n\0\0\0\n", "HANDLE\tNUMBER\n'-1\tx\n'18446744073709551616\ty\n", "\t\t\t\nHANDLE\n", "HANDLE\tHANDLE\tNUMBER\n"] {
        let _ = s.execute("attin", &json!({"text": junk}));
    }
    // Control characters are dropped and line breaks become spaces in a single-line attribute.
    attin_text(&mut s, &format!("HANDLE\tNUMBER\n'{d1}\t\"a\r\nb\u{1}c\"\n"));
    assert_eq!(value(&s, &d1, "NUMBER"), "a bc");
    // Bytes that aren't UTF-8 read as Windows-1252; UTF-16 with a byte order mark reads too.
    let path = scratch("cp1252.txt");
    let ps = path.to_string_lossy().to_string();
    let mut bytes = format!("HANDLE\tNUMBER\r\n'{d1}\tcaff").into_bytes();
    bytes.extend([0xE8, b'\r', b'\n']);
    std::fs::write(&path, &bytes).unwrap();
    s.execute("attin", &json!({"path": ps})).unwrap();
    assert_eq!(value(&s, &d1, "NUMBER"), "caffè");
    let mut utf16 = vec![0xFF, 0xFE];
    utf16.extend(format!("HANDLE\tNUMBER\r\n'{d1}\tüber\r\n").encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&path, &utf16).unwrap();
    s.execute("attin", &json!({"path": ps})).unwrap();
    assert_eq!(value(&s, &d1, "NUMBER"), "über");
    // A file too large is refused before it is read in full; a folder or a missing file is an error.
    std::fs::write(&path, vec![b'x'; 32 * 1024 * 1024 + 1]).unwrap();
    assert!(s.execute("attin", &json!({"path": ps})).is_err());
    assert!(s.execute("attin", &json!({"path": std::env::temp_dir().to_string_lossy()})).is_err());
    assert!(s.execute("attin", &json!({"path": scratch("missing.txt").to_string_lossy()})).is_err());
    assert!(s.execute("attin", &json!({})).is_err());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_command_line_forms_ask_for_the_file_then_the_objects() {
    let (mut s, [d1, d2, w1, _]) = drawing();
    let path = scratch("typed.txt");
    let ps = path.to_string_lossy().to_string();
    s.start("attout").unwrap();
    assert!(s.prompt_text().contains("output file name"), "{}", s.prompt_text());
    s.input(Input::Text(format!("\"{ps}\""))).unwrap();
    assert!(s.prompt_text().contains("Select objects"), "{}", s.prompt_text());
    s.input(Input::Pick(vec![Handle::parse_hex(&d1).unwrap(), Handle::parse_hex(&w1).unwrap()])).unwrap();
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none());
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 3, "{text}");
    assert!(!text.contains(&format!("'{d2}")));

    // A pickfirst selection is used straight after the file name.
    std::fs::write(&path, text.replace("\tD1\t", "\tD1-typed\t")).unwrap();
    s.set_selection(vec![Handle::parse_hex(&d2).unwrap()]);
    s.start("attout").unwrap();
    s.input(Input::Text(scratch("pickfirst.txt").to_string_lossy().to_string())).unwrap();
    assert!(s.running.is_none());
    assert_eq!(std::fs::read_to_string(scratch("pickfirst.txt")).unwrap().lines().count(), 2);

    // ATTIN asks for the file and applies it as one undo step.
    s.start("attin").unwrap();
    assert!(s.prompt_text().contains("input file name"), "{}", s.prompt_text());
    s.input(Input::Text(ps.clone())).unwrap();
    assert!(s.running.is_none());
    assert_eq!(value(&s, &d1, "NUMBER"), "D1-typed");
    s.undo().unwrap();
    assert_eq!(value(&s, &d1, "NUMBER"), "D1");
    // A script reaches the same prompts.
    s.script(&format!("attin\n{ps}\n")).unwrap();
    assert_eq!(value(&s, &d1, "NUMBER"), "D1-typed");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(scratch("pickfirst.txt"));
}
