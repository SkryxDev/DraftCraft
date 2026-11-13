use super::*;
use crate::file::{ColorValue, EntityLinetype, EntitySpace, XDataBlock, XDataItem};
use crate::read::object::{self, DecodeCtx};
use crate::read::version::DwgVersion as V;

const VERSIONS: [V; 6] = [V::R2000, V::R2004, V::R2007, V::R2010, V::R2013, V::R2018];

/// An entity written in every version reads back field for field with the reader.
#[test]
fn an_entity_reads_back_in_every_version() {
    for ver in VERSIONS {
        let mut w = ObjectWriter::new(ver, 0x13, 0x2A5, 30, TextEncoding::default());
        w.xdata(&[XDataBlock { app: 0x12, items: vec![XDataItem::Text("héllo".into()), XDataItem::Int16(7), XDataItem::Real(2.5)] }]);
        let reactors = [0x77u64];
        w.entity(&EntityStart {
            space: EntitySpace::Owned,
            owner: 0x1F,
            reactors: &reactors,
            xdictionary: 0x300,
            color: ColorValue::Index(3),
            linetype: EntityLinetype::Record(0x16),
            linetype_scale: 2.0,
            layer: 0x10,
            lineweight: 5,
            invisible: true,
            ..EntityStart::default()
        });
        w.bd(1.5);
        w.text("name");
        w.bs(9);
        w.reference(Ref::HardPointer, 0x55);
        let bytes = w.finish();
        let ctx = DecodeCtx { ver, codepage: 30, classes: &[], text: Default::default() };
        let mut o = object::open(&ctx, &bytes, 0).unwrap_or_else(|e| panic!("{ver:?}: {e}"));
        assert_eq!((o.handle, o.type_code), (0x2A5, 0x13), "{ver:?}");
        assert_eq!(o.xdata.len(), 1, "{ver:?}");
        assert_eq!(o.s.dat.bd().unwrap(), 1.5, "{ver:?}");
        assert_eq!(o.s.tv().unwrap(), "name", "{ver:?}");
        assert_eq!(o.s.dat.bs().unwrap(), 9, "{ver:?}");
        let (owner, r, xdic) = object::common_handles(&mut o).unwrap();
        assert_eq!((owner, r.len(), xdic), (0x1F, 1, 0x300), "{ver:?}");
        let c = o.common.clone().unwrap();
        assert_eq!(c.layer.0, 0x10, "{ver:?}");
        assert!(c.invisible && c.lineweight == 5 && c.linetype_scale == 2.0, "{ver:?}");
        assert_eq!(o.s.h_abs(0x2A5).unwrap(), 0x55, "{ver:?}");
        assert_eq!(o.s.handle_bits_left() / 8, 0, "{ver:?}: nothing left but padding");
    }
}

/// An object that is not an entity, with no text, in every version.
#[test]
fn an_object_reads_back_in_every_version() {
    for ver in VERSIONS {
        let mut w = ObjectWriter::new(ver, 0x2A, 0xC, 30, TextEncoding::default());
        w.xdata(&[]);
        w.object(0, &[], 0);
        w.bl(3);
        w.reference(Ref::SoftOwner, 0xD);
        let bytes = w.finish();
        let ctx = DecodeCtx { ver, codepage: 30, classes: &[], text: Default::default() };
        let mut o = object::open(&ctx, &bytes, 0).unwrap_or_else(|e| panic!("{ver:?}: {e}"));
        assert_eq!(o.s.dat.bl().unwrap(), 3, "{ver:?}");
        let (owner, r, xdic) = object::common_handles(&mut o).unwrap();
        assert_eq!((owner, r.len(), xdic), (0, 0, 0), "{ver:?}");
        assert_eq!(o.s.h_abs(0xC).unwrap(), 0xD, "{ver:?}");
    }
}
