//! One object written field by field, in the order the format stores them: the mirror of
//! [`crate::file::ObjectReader`]. The writer that knows an object's type calls the same field
//! methods the reader calls, with the values; this type keeps the object's three streams
//! (fields, text, references) apart and joins them as the object's version stores them.
//!
//! Writes AutoCAD 2000 to AutoCAD 2018 objects.

use crate::bits::BitWriter;
use crate::crc::crc16;
use crate::file::{ColorValue, EntityLinetype, EntitySpace, XDataBlock, XDataItem};
use crate::read::version::DwgVersion as V;

/// How strings become a file's text: 8-bit text (up to AutoCAD 2004), given the string and the
/// code page number the file is in. Text from AutoCAD 2007 on is UTF-16 and needs no encoder.
/// The mirror of [`crate::file::TextDecoding`].
#[derive(Clone, Copy, Debug)]
pub struct TextEncoding {
    pub ansi: fn(&str, u16) -> Vec<u8>,
}

impl Default for TextEncoding {
    /// Windows-1252's Latin-1 half; every other character as `?`.
    fn default() -> Self {
        TextEncoding { ansi: |s, _| s.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect() }
    }
}

/// How a reference to another object is stored: what the reference means to the object that
/// holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ref {
    /// The object owns the other; the other may be absent.
    SoftOwner = 2,
    /// The object owns the other.
    HardOwner = 3,
    /// The object names the other, which may be absent.
    SoftPointer = 4,
    /// The object needs the other.
    HardPointer = 5,
}

/// What every entity starts with.
#[derive(Clone, Debug)]
pub struct EntityStart<'a> {
    /// The entity's own drawing of itself (a proxy's), when it has one.
    pub graphics: Option<&'a [u8]>,
    pub space: EntitySpace,
    /// The block record that owns the entity. Written when `space` is [`EntitySpace::Owned`].
    pub owner: u64,
    pub reactors: &'a [u64],
    pub xdictionary: u64,
    pub color: ColorValue,
    /// The DBCOLOR object of a colour book colour (AutoCAD 2004 on); 0 for none.
    pub color_book: u64,
    /// The transparency word (DXF group 440).
    pub transparency: Option<u32>,
    pub linetype_scale: f64,
    pub linetype: EntityLinetype,
    pub layer: u64,
    /// The plot style object; 0 for ByLayer.
    pub plotstyle: u64,
    /// The material object (AutoCAD 2007 on); 0 for ByLayer.
    pub material: u64,
    /// Shadow flags (AutoCAD 2007 on): 0 casts and receives, 1 casts, 2 receives, 3 ignores.
    pub shadow: u8,
    pub invisible: bool,
    /// The lineweight, as the index DWG stores (29 ByLayer, 30 ByBlock, 31 Default).
    pub lineweight: u8,
    /// The entities before and after this one in its block (AutoCAD 2000 files, where each
    /// entity names its neighbours): `None` when they are the objects with the handles just
    /// below and above this one's.
    pub links: Option<(u64, u64)>,
}

impl Default for EntityStart<'_> {
    fn default() -> Self {
        EntityStart {
            graphics: None,
            space: EntitySpace::Model,
            owner: 0,
            reactors: &[],
            xdictionary: 0,
            color: ColorValue::ByLayer,
            color_book: 0,
            transparency: None,
            linetype_scale: 1.0,
            linetype: EntityLinetype::ByLayer,
            layer: 0,
            plotstyle: 0,
            material: 0,
            shadow: 0,
            invisible: false,
            lineweight: 29,
            links: None,
        }
    }
}

/// One object being written.
pub struct ObjectWriter {
    ver: V,
    type_code: u16,
    handle: u64,
    codepage: u16,
    text: TextEncoding,
    /// The fields after the object's size: from its handle on.
    dat: BitWriter,
    strs: BitWriter,
    has_strings: bool,
    hdl: BitWriter,
}

impl ObjectWriter {
    /// An object of DWG type `type_code` (0x13 for LINE; a class's number for a class-defined
    /// type) with the handle `handle`, for a file of version `ver` whose 8-bit text is in
    /// `codepage`. Its extended data comes next ([`ObjectWriter::xdata`]), then
    /// [`ObjectWriter::entity`] or [`ObjectWriter::object`], then its own fields.
    pub fn new(ver: V, type_code: u16, handle: u64, codepage: u16, text: TextEncoding) -> Self {
        let mut dat = BitWriter::new();
        dat.handle(0, handle);
        ObjectWriter { ver, type_code, handle, codepage, text, dat, strs: BitWriter::new(), has_strings: false, hdl: BitWriter::new() }
    }

    /// The file's version, which decides what many objects store.
    pub fn version(&self) -> V {
        self.ver
    }
    /// True when the file keeps text apart from the other fields (AutoCAD 2007 and later).
    pub fn has_string_stream(&self) -> bool {
        self.ver.has_string_stream()
    }
    /// The object's own handle.
    pub fn handle(&self) -> u64 {
        self.handle
    }

    /// The object's extended data, one block per application, then the end of it. Called once,
    /// with an empty list for an object that has none. A block too long for its size field
    /// (32 767 bytes) is left out; `false` is returned when one was.
    pub fn xdata(&mut self, blocks: &[XDataBlock]) -> bool {
        let mut whole = true;
        for x in blocks {
            let mut b: Vec<u8> = Vec::new();
            for item in &x.items {
                self.xdata_item(&mut b, item);
            }
            if b.is_empty() {
                continue;
            }
            let Ok(len) = u16::try_from(b.len()).map_err(|_| ()).and_then(|n| if n > 0x7FFF { Err(()) } else { Ok(n) }) else {
                whole = false;
                continue;
            };
            self.dat.bs(len);
            self.dat.handle(5, x.app);
            self.dat.raw_bytes(&b);
        }
        self.dat.bs(0);
        whole
    }

    fn xdata_item(&self, b: &mut Vec<u8>, item: &XDataItem) {
        let point = |b: &mut Vec<u8>, code: u8, p: &[f64; 3]| {
            b.push(code);
            for c in p {
                b.extend_from_slice(&c.to_le_bytes());
            }
        };
        let real = |b: &mut Vec<u8>, code: u8, v: f64| {
            b.push(code);
            b.extend_from_slice(&v.to_le_bytes());
        };
        match item {
            XDataItem::Text(s) => {
                b.push(0);
                if self.ver.has_string_stream() {
                    let units: Vec<u16> = s.encode_utf16().take(usize::from(u16::MAX)).collect();
                    b.extend_from_slice(&u16::try_from(units.len()).unwrap_or(u16::MAX).to_le_bytes());
                    for u in units {
                        b.extend_from_slice(&u.to_le_bytes());
                    }
                } else {
                    let mut bytes = (self.text.ansi)(s, self.codepage);
                    bytes.truncate(255);
                    b.push(u8::try_from(bytes.len()).unwrap_or(u8::MAX));
                    b.extend_from_slice(&self.codepage.to_be_bytes());
                    b.extend_from_slice(&bytes);
                }
            }
            XDataItem::Open => b.extend_from_slice(&[2, 0]),
            XDataItem::Close => b.extend_from_slice(&[2, 1]),
            XDataItem::Layer(h) => {
                b.push(3);
                b.extend_from_slice(&h.to_be_bytes());
            }
            XDataItem::Bytes(bytes) => {
                let bytes = bytes.get(..bytes.len().min(255)).unwrap_or(&[]);
                b.push(4);
                b.push(u8::try_from(bytes.len()).unwrap_or(u8::MAX));
                b.extend_from_slice(bytes);
            }
            XDataItem::Handle(h) => {
                b.push(5);
                b.extend_from_slice(&h.to_be_bytes());
            }
            XDataItem::Point(p) => point(b, 10, p),
            XDataItem::Position(p) => point(b, 11, p),
            XDataItem::Displacement(p) => point(b, 12, p),
            XDataItem::Direction(p) => point(b, 13, p),
            XDataItem::Real(v) => real(b, 40, *v),
            XDataItem::Distance(v) => real(b, 41, *v),
            XDataItem::ScaleFactor(v) => real(b, 42, *v),
            XDataItem::Int16(v) => {
                b.push(70);
                b.extend_from_slice(&v.to_le_bytes());
            }
            XDataItem::Int32(v) => {
                b.push(71);
                b.extend_from_slice(&v.to_le_bytes());
            }
            XDataItem::Undecoded(bytes) => b.extend_from_slice(bytes),
        }
    }

    /// What every entity starts with: its fields, and its first references (owner, reactors,
    /// extension dictionary, layer, linetype, material, plot style).
    pub fn entity(&mut self, e: &EntityStart<'_>) {
        let ver = self.ver;
        match e.graphics {
            Some(g) => {
                self.dat.b(true);
                if ver >= V::R2010 {
                    self.dat.bll(g.len() as u64);
                } else {
                    self.dat.rl(u32::try_from(g.len()).unwrap_or(u32::MAX));
                }
                self.dat.raw_bytes(g);
            }
            None => self.dat.b(false),
        }
        self.dat.bb(match e.space {
            EntitySpace::Owned => 0,
            EntitySpace::Paper => 1,
            EntitySpace::Model => 2,
        });
        self.dat.bl(u32::try_from(e.reactors.len()).unwrap_or(u32::MAX));
        if ver >= V::R2004 {
            self.dat.b(e.xdictionary == 0);
        }
        if ver >= V::R2013 {
            self.dat.b(false);
        }
        if ver <= V::R2000 {
            self.dat.b(e.links.is_none());
        }
        self.entity_color(e);
        self.dat.bd(e.linetype_scale);
        self.dat.bb(match e.linetype {
            EntityLinetype::ByLayer => 0,
            EntityLinetype::ByBlock => 1,
            EntityLinetype::Continuous => 2,
            EntityLinetype::Record(_) => 3,
        });
        self.dat.bb(if e.plotstyle == 0 { 0 } else { 3 });
        if ver >= V::R2007 {
            self.dat.bb(if e.material == 0 { 0 } else { 3 });
            self.dat.rc(e.shadow);
        }
        if ver >= V::R2010 {
            // No visual style of its own (whole, face, edge).
            self.dat.b(false);
            self.dat.b(false);
            self.dat.b(false);
        }
        self.dat.bs(u16::from(e.invisible));
        self.dat.rc(e.lineweight);

        // The references, in the order `read::object::common_handles` reads them.
        if e.space == EntitySpace::Owned {
            self.owner(e.owner);
        }
        for r in e.reactors {
            self.reference(Ref::SoftPointer, *r);
        }
        if ver < V::R2004 || e.xdictionary != 0 {
            self.reference(Ref::HardOwner, e.xdictionary);
        }
        if ver <= V::R2000
            && let Some((prev, next)) = e.links
        {
            self.relative(prev);
            self.relative(next);
        }
        if ver >= V::R2004 && e.color_book != 0 {
            self.reference(Ref::HardPointer, e.color_book);
        }
        self.reference(Ref::HardPointer, e.layer);
        if let EntityLinetype::Record(h) = e.linetype {
            self.reference(Ref::HardPointer, h);
        }
        if ver >= V::R2007 && e.material != 0 {
            self.reference(Ref::HardPointer, e.material);
        }
        if e.plotstyle != 0 {
            self.reference(Ref::HardPointer, e.plotstyle);
        }
    }

    /// The common properties of an entity embedded in another (an AutoCAD 2018 multiline
    /// attribute's text): its own owner (none), no reactors, extension dictionary or binary data,
    /// then `e`'s colour, linetype scale and linetype, material and plot style flags, shadows, no
    /// visual styles, visibility and lineweight. Its references follow, where the handle stream
    /// holds them, by [`ObjectWriter::embedded_entity_refs`].
    pub fn embedded_entity(&mut self, e: &EntityStart<'_>) {
        self.dat.bb(0);
        self.dat.bl(0);
        self.dat.b(true);
        self.dat.b(false);
        self.entity_color(e);
        self.dat.bd(e.linetype_scale);
        self.dat.bb(match e.linetype {
            EntityLinetype::ByLayer => 0,
            EntityLinetype::ByBlock => 1,
            EntityLinetype::Continuous => 2,
            EntityLinetype::Record(_) => 3,
        });
        self.dat.bb(if e.material == 0 { 0 } else { 3 });
        self.dat.bb(if e.plotstyle == 0 { 0 } else { 3 });
        self.dat.rc(e.shadow);
        self.dat.b(false);
        self.dat.b(false);
        self.dat.b(false);
        self.dat.bs(u16::from(e.invisible));
        self.dat.rc(e.lineweight);
    }

    /// The references of an entity [`ObjectWriter::embedded_entity`] wrote: its owner (none),
    /// layer, then the linetype, material and plot style it flagged.
    pub fn embedded_entity_refs(&mut self, e: &EntityStart<'_>) {
        self.reference(Ref::SoftPointer, 0);
        self.reference(Ref::HardPointer, e.layer);
        if let EntityLinetype::Record(h) = e.linetype {
            self.reference(Ref::HardPointer, h);
        }
        if e.material != 0 {
            self.reference(Ref::HardPointer, e.material);
        }
        if e.plotstyle != 0 {
            self.reference(Ref::HardPointer, e.plotstyle);
        }
    }

    /// An entity's colour: an index up to AutoCAD 2000; from 2004 the index with flags for a
    /// true colour, a colour book colour and a transparency, each following when flagged.
    fn entity_color(&mut self, e: &EntityStart<'_>) {
        let index: u16 = match e.color {
            ColorValue::ByLayer => 256,
            ColorValue::ByBlock => 0,
            ColorValue::Index(i) => u16::from(i),
            ColorValue::Rgb(..) => 0,
            ColorValue::None => 257,
        };
        if self.ver < V::R2004 {
            let index = match e.color {
                ColorValue::Rgb(r, g, b) => u16::from(nearest_index(r, g, b)),
                _ => index,
            };
            self.dat.bs(index);
            return;
        }
        let mut flags = index;
        let book = e.color_book != 0;
        let rgb = matches!(e.color, ColorValue::Rgb(..));
        if book {
            // A colour book colour is the DBCOLOR object the references name: no colour here.
            flags |= 0xC000;
        } else if rgb {
            flags |= 0x8000;
        }
        if e.transparency.is_some() {
            flags |= 0x2000;
        }
        self.dat.bs(flags);
        if rgb && !book {
            self.dat.bl(method_word(e.color));
        }
        if let Some(t) = e.transparency {
            self.dat.bl(t);
        }
    }

    /// What every object that is not an entity starts with: its reactor count, and its first
    /// references (owner, reactors, extension dictionary).
    pub fn object(&mut self, owner: u64, reactors: &[u64], xdictionary: u64) {
        let ver = self.ver;
        self.dat.bl(u32::try_from(reactors.len()).unwrap_or(u32::MAX));
        if ver >= V::R2004 {
            self.dat.b(xdictionary == 0);
        }
        if ver >= V::R2013 {
            self.dat.b(false);
        }
        self.owner(owner);
        for r in reactors {
            self.reference(Ref::SoftPointer, *r);
        }
        if ver < V::R2004 || xdictionary != 0 {
            self.reference(Ref::HardOwner, xdictionary);
        }
    }

    /// A reference to the object's owner: a soft pointer in its shortest form, as AutoCAD writes
    /// owners (see [`ObjectWriter::relative`]).
    fn owner(&mut self, target: u64) {
        self.relative(target);
    }

    /// A reference to another object, by its handle (0 for none).
    pub fn reference(&mut self, kind: Ref, target: u64) {
        self.hdl.handle(kind as u8, target);
    }

    /// A soft pointer in its shortest form: the handle itself, the next or previous handle (no
    /// bytes), or the distance from this object when that is the smaller number.
    pub fn relative(&mut self, target: u64) {
        let own = self.handle;
        if target == 0 {
            self.hdl.handle(4, 0);
        } else if target == own.wrapping_add(1) {
            self.hdl.handle(6, 0);
        } else if target.wrapping_add(1) == own {
            self.hdl.handle(8, 0);
        } else if target > own && target - own < target {
            self.hdl.handle(0xA, target - own);
        } else if target < own && own - target < target {
            // Only when the distance is the smaller number (AutoCAD's own files: layer A owned
            // by 8 is "C.2"; application 13 owned by 9 is "4.9", not "C.A").
            self.hdl.handle(0xC, own - target);
        } else {
            self.hdl.handle(4, target);
        }
    }

    /// A handle among the object's fields (not a reference to follow: an extension record's
    /// value, the handle seed).
    pub fn inline_handle(&mut self, code: u8, value: u64) {
        self.dat.handle(code, value);
    }

    /// A bit.
    pub fn b(&mut self, v: bool) {
        self.dat.b(v);
    }
    /// Two bits.
    pub fn bb(&mut self, v: u8) {
        self.dat.bb(v);
    }
    /// A bit-coded 16-bit integer.
    pub fn bs(&mut self, v: u16) {
        self.dat.bs(v);
    }
    /// A bit-coded 32-bit integer.
    pub fn bl(&mut self, v: u32) {
        self.dat.bl(v);
    }
    /// A bit-coded 64-bit integer.
    pub fn bll(&mut self, v: u64) {
        self.dat.bll(v);
    }
    /// A bit-coded double.
    pub fn bd(&mut self, v: f64) {
        self.dat.bd(v);
    }
    /// Two bit-coded doubles.
    pub fn bd2(&mut self, v: [f64; 2]) {
        self.dat.bd2(v);
    }
    /// Three bit-coded doubles.
    pub fn bd3(&mut self, v: [f64; 3]) {
        self.dat.bd3(v);
    }
    /// A double stored against a default (the previous coordinate, or 1).
    pub fn dd(&mut self, v: f64, default: f64) {
        self.dat.dd(v, default);
    }
    /// A thickness.
    pub fn thickness(&mut self, v: f64) {
        self.dat.bt(v, self.ver);
    }
    /// An extrusion direction.
    pub fn extrusion(&mut self, v: [f64; 3]) {
        self.dat.be(v, self.ver);
    }
    /// A byte.
    pub fn rc(&mut self, v: u8) {
        self.dat.rc(v);
    }
    /// A raw 16-bit integer.
    pub fn rs(&mut self, v: u16) {
        self.dat.rs(v);
    }
    /// A raw 32-bit integer.
    pub fn rl(&mut self, v: u32) {
        self.dat.rl(v);
    }
    /// A raw double.
    pub fn rd(&mut self, v: f64) {
        self.dat.rd(v);
    }
    /// Two raw doubles.
    pub fn rd2(&mut self, v: [f64; 2]) {
        self.dat.rd2(v);
    }
    /// Three raw doubles.
    pub fn rd3(&mut self, v: [f64; 3]) {
        self.dat.rd3(v);
    }
    /// Bytes as they are.
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.dat.raw_bytes(bytes);
    }
    /// A text value: in the string stream where the file has one, otherwise among the fields in
    /// the file's code page.
    pub fn text(&mut self, s: &str) {
        if self.ver.has_string_stream() {
            self.strs.text16(clip16(s));
            self.has_strings = true;
        } else {
            self.dat.text8(&terminated((self.text.ansi)(s, self.codepage)));
        }
    }
    /// 8-bit text in the file's code page, as its bytes (for a field that stores its own length).
    pub fn ansi(&self, s: &str) -> Vec<u8> {
        (self.text.ansi)(s, self.codepage)
    }
    /// A colour: an index up to AutoCAD 2000; from 2004 the index field, the colour word and
    /// the flags for names (none).
    pub fn color(&mut self, c: ColorValue) {
        self.named_color(c, None, None);
    }
    /// A colour with the names of a colour book colour (AutoCAD 2004 on), as a layer or a
    /// DBCOLOR object stores it.
    pub fn named_color(&mut self, c: ColorValue, name: Option<&str>, book: Option<&str>) {
        if self.ver < V::R2004 {
            self.dat.bs(match c {
                ColorValue::ByLayer => 256,
                ColorValue::ByBlock => 0,
                ColorValue::Index(i) => u16::from(i),
                ColorValue::Rgb(r, g, b) => u16::from(nearest_index(r, g, b)),
                ColorValue::None => 257,
            });
            return;
        }
        // The index field stays 0: the colour is in the word, as AutoCAD writes every colour
        // from 2004 on.
        self.dat.bs(0);
        self.dat.bl(method_word(c));
        self.dat.rc(u8::from(name.is_some()) | u8::from(book.is_some()) << 1);
        if let Some(n) = name {
            self.text(n);
        }
        if let Some(n) = book {
            self.text(n);
        }
    }
    /// A colour as an index with its sign (a layer that is off, before AutoCAD 2004).
    pub fn color_index(&mut self, index: i16) {
        self.dat.bs(index.cast_unsigned());
    }
    /// The fields and references of an object the codec keeps as its DXF groups (table and
    /// multileader styles, materials, visual styles, section and detail view styles, dimension
    /// associations, image definitions): `items` are its groups from the first subclass marker
    /// on, in DXF order, a point as one item; `text_styles` names the text styles a table style
    /// refers to. An error for a type or a version the codec has no layout for, or groups that
    /// don't fit the layout: nothing usable has been written then, and the object is left out.
    pub fn grouped(&mut self, kind: &str, items: Vec<(u16, crate::file::XrValue)>, text_styles: &[(String, u64)]) -> crate::records::Result<()> {
        use crate::read::model::{Groups, Handle};
        let lookup = crate::read::styles::Lookup::from_text_styles(text_styles.iter().map(|(n, h)| (n.clone(), Handle(*h))).collect());
        let mut w = crate::write::objects::ObjW::new(self.ver, Handle(self.handle));
        crate::read::styles::encode_with(&mut w, &Groups { kind: kind.to_string(), items }, &lookup)?;
        let (dat, strs, has_strings, hdl) = w.into_streams();
        self.dat.append(&dat);
        self.strs.append(&strs);
        self.has_strings |= has_strings;
        self.hdl.append(&hdl);
        Ok(())
    }
    /// How many bits of fields the object holds so far.
    pub fn position(&self) -> usize {
        self.dat.position()
    }

    /// The finished object as it sits among the file's objects: its size, the size of its
    /// references (AutoCAD 2010 on), its data and the checksum.
    pub fn finish(self) -> Vec<u8> {
        let ver = self.ver;
        let mut w = BitWriter::new();
        if ver >= V::R2010 {
            w.ot(self.type_code);
        } else {
            w.bs(self.type_code);
        }
        // AutoCAD 2000 to 2007: where the references start, in bits from the object's start.
        let strings = if ver.has_string_stream() { if self.has_strings { self.strs.position() + if self.strs.position() >= 0x8000 { 33 } else { 17 } } else { 1 } } else { 0 };
        if ver < V::R2010 {
            let end = w.position() + 32 + self.dat.position() + strings;
            w.rl(u32::try_from(end).unwrap_or(u32::MAX));
        }
        w.append(&self.dat);
        if ver.has_string_stream() {
            if self.has_strings {
                let n = self.strs.position();
                w.append(&self.strs);
                if n >= 0x8000 {
                    w.rs(u16::try_from(n >> 15).unwrap_or(u16::MAX));
                    w.rs(u16::try_from(n & 0x7FFF).unwrap_or(0) | 0x8000);
                } else {
                    w.rs(u16::try_from(n).unwrap_or(0));
                }
                w.b(true);
            } else {
                w.b(false);
            }
        }
        let end = w.position();
        w.append(&self.hdl);
        while !w.position().is_multiple_of(8) {
            // AutoCAD fills the last byte with 1 bits.
            w.b(true);
        }
        let total = w.position();
        frame(ver, &w.into_bytes(), (total - end) as u64)
    }
}

/// 8-bit text as a field stores it: with the 0 that ends it, which the length counts; an empty
/// text is no bytes at all. As AutoCAD writes it.
pub(super) fn terminated(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(end) = bytes.iter().position(|b| *b == 0) {
        bytes.truncate(end);
    }
    if bytes.is_empty() {
        return bytes;
    }
    bytes.truncate(usize::from(u16::MAX) - 1);
    bytes.push(0);
    bytes
}

/// The start of `s` that fits a text field's 16-bit length.
fn clip16(s: &str) -> &str {
    let mut units = 0usize;
    for (i, c) in s.char_indices() {
        units += c.len_utf16();
        if units > usize::from(u16::MAX) {
            return s.get(..i).unwrap_or("");
        }
    }
    s
}

/// The colour word AutoCAD 2004 and later store: the method in the top byte (0xC0 ByLayer,
/// 0xC1 ByBlock, 0xC2 a true colour, 0xC3 an index, 0xC8 none), the value below.
pub fn method_word(c: ColorValue) -> u32 {
    match c {
        ColorValue::ByLayer => 0xC000_0000,
        ColorValue::ByBlock => 0xC100_0000,
        ColorValue::Rgb(r, g, b) => 0xC200_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b),
        ColorValue::Index(i) => 0xC300_0000 | u32::from(i),
        ColorValue::None => 0xC800_0000,
    }
}

/// The colour a colour word ([`method_word`]) stores; ByLayer for a method it doesn't name.
pub fn color_of_word(word: u32) -> ColorValue {
    let [method, r, g, b] = word.to_be_bytes();
    match method {
        0xC1 => ColorValue::ByBlock,
        0xC2 => ColorValue::Rgb(r, g, b),
        0xC3 if b == 0 => ColorValue::ByBlock,
        0xC3 if b == 0xFF => ColorValue::ByLayer,
        0xC3 => ColorValue::Index(b),
        0xC8 => ColorValue::None,
        _ => ColorValue::ByLayer,
    }
}

/// The index colour nearest a true colour, for the versions that store none (AutoCAD 2000):
/// among the nine named colours and the greys, which every program draws the same.
fn nearest_index(r: u8, g: u8, b: u8) -> u8 {
    const BASIC: [(u8, [u8; 3]); 15] = [
        (1, [255, 0, 0]),
        (2, [255, 255, 0]),
        (3, [0, 255, 0]),
        (4, [0, 255, 255]),
        (5, [0, 0, 255]),
        (6, [255, 0, 255]),
        (7, [255, 255, 255]),
        (8, [128, 128, 128]),
        (9, [192, 192, 192]),
        (250, [51, 51, 51]),
        (251, [91, 91, 91]),
        (252, [132, 132, 132]),
        (253, [173, 173, 173]),
        (254, [214, 214, 214]),
        (255, [255, 255, 255]),
    ];
    let d = |c: &[u8; 3]| {
        let s = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        s(c[0], r) + s(c[1], g) + s(c[2], b)
    };
    BASIC.iter().min_by_key(|(_, c)| d(c)).map_or(7, |(i, _)| *i)
}

/// Object data in its size fields and checksum, as it sits among the file's objects.
pub fn frame(ver: V, data: &[u8], handle_bits: u64) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.ms(u32::try_from(data.len()).unwrap_or(u32::MAX));
    if ver >= V::R2010 {
        w.mc_unsigned(handle_bits);
    }
    let mut out = w.into_bytes();
    out.extend_from_slice(data);
    let crc = crc16(0xC0C1, &out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}
