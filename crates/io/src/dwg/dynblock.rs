//! A dynamic block's objects (its evaluation graph and the parameters, grips and actions in it),
//! read into the DXF groups AutoCAD writes of them, which the drawing keeps and
//! `draftcraft_doc::dynblock` reads. The DWG object holds the values in a fixed order; this puts
//! them in DXF's order, adds the counts DXF has and leaves out what only DWG stores (an
//! expression's parent). `super::write::dynblock` writes them back the same way.

use draftcraft_dwg::file::{CommonRefs, Groups, Handle, ObjectReader, XrValue};
use draftcraft_dwg::records::{Error, Result};

/// Most entries of any list in one object: more is a damaged object.
const MAX_ITEMS: u32 = 1 << 16;

/// An expression's "no value" type: nothing follows it.
const NO_VALUE: i16 = -9999;

/// The object of type `kind` as its DXF groups, `None` for a type that isn't a dynamic block's.
pub(super) fn read(r: &mut ObjectReader<'_>, kind: &str) -> Option<Result<(Groups, CommonRefs)>> {
    let kind = kind.to_ascii_uppercase();
    if !is_dynamic_block_type(&kind) {
        return None;
    }
    Some(decode(r, &kind))
}

fn is_dynamic_block_type(kind: &str) -> bool {
    matches!(
        kind,
        "ACAD_EVALUATION_GRAPH"
            | "ACDB_BLOCKREPRESENTATION_DATA"
            | "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION"
            | "ACDB_DYNAMICBLOCKPROXYNODE"
            | "ACDBBLOCKPARAMDEPENDENCYBODY"
            | "BLOCKPARAMDEPENDENCYBODY"
            | "BLOCKGRIPLOCATIONCOMPONENT"
    ) || (kind.starts_with("BLOCK")
        && (kind.ends_with("GRIP") || kind.ends_with("PARAMETER") || kind.ends_with("ACTION"))
        && kind != "BLOCKPROPERTIESTABLE")
}

fn decode(r: &mut ObjectReader<'_>, kind: &str) -> Result<(Groups, CommonRefs)> {
    let refs = r.common_refs()?;
    let mut o = Out { r, t: Vec::new() };
    match kind {
        "ACDB_BLOCKREPRESENTATION_DATA" | "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION" => {
            o.marker(if kind == "ACDB_BLOCKREPRESENTATION_DATA" { "AcDbBlockRepresentationData" } else { "AcDbDynamicBlockPurgePreventer" });
            o.bs(70)?;
            o.href(340)?;
        }
        "ACDB_DYNAMICBLOCKPROXYNODE" => {
            o.eval()?;
            o.marker("AcDbDynamicBlockProxyNode");
        }
        "BLOCKGRIPLOCATIONCOMPONENT" => {
            o.eval()?;
            o.marker("AcDbBlockGripExpr");
            o.bl(91)?;
            o.text(300)?;
        }
        "BLOCKALIGNMENTGRIP" | "BLOCKLINEARGRIP" => {
            o.grip()?;
            o.marker(if kind == "BLOCKALIGNMENTGRIP" { "AcDbBlockAlignmentGrip" } else { "AcDbBlockLinearGrip" });
            o.vector(140)?;
        }
        "BLOCKFLIPGRIP" => {
            o.grip()?;
            o.marker("AcDbBlockFlipGrip");
            let state = o.r.bl()?;
            o.vector(140)?;
            o.int(93, i64::from(state as i32));
        }
        "BLOCKLOOKUPGRIP" | "BLOCKPOLARGRIP" | "BLOCKROTATIONGRIP" | "BLOCKVISIBILITYGRIP" | "BLOCKXYGRIP" | "BLOCKPROPERTIESTABLEGRIP" => {
            o.grip()?;
            o.marker(match kind {
                "BLOCKLOOKUPGRIP" => "AcDbBlockLookupGrip",
                "BLOCKPOLARGRIP" => "AcDbBlockPolarGrip",
                "BLOCKROTATIONGRIP" => "AcDbBlockRotationGrip",
                "BLOCKVISIBILITYGRIP" => "AcDbBlockVisibilityGrip",
                "BLOCKXYGRIP" => "AcDbBlockXYGrip",
                _ => "AcDbBlockPropertiesTableGrip",
            });
        }
        "BLOCKALIGNMENTPARAMETER" => {
            o.two_point()?;
            o.marker("AcDbBlockAlignmentParameter");
            o.b(280)?;
        }
        "BLOCKBASEPOINTPARAMETER" => {
            o.one_point()?;
            o.marker("AcDbBlockBasepointParameter");
            o.p3(1011)?;
            o.p3(1012)?;
        }
        "BLOCKFLIPPARAMETER" => {
            o.two_point()?;
            o.marker("AcDbBlockFlipParameter");
            for c in [305, 306, 307, 308] {
                o.text(c)?;
            }
            o.p3(1012)?;
            let flags = o.r.bl()?;
            o.text(309)?;
            o.int(96, i64::from(flags as i32));
        }
        "BLOCKLINEARPARAMETER" => {
            o.two_point()?;
            o.marker("AcDbBlockLinearParameter");
            o.text(305)?;
            o.text(306)?;
            o.bd(140)?;
            o.value_set(96, 141, 175, 307)?;
        }
        "BLOCKLOOKUPPARAMETER" => {
            o.one_point()?;
            // AutoCAD's marker, which differs from the class name in one letter's case.
            o.marker("AcDbBlockLookUpParameter");
            o.bl(94)?;
            o.text(303)?;
            o.text(304)?;
        }
        "BLOCKPOINTPARAMETER" => {
            o.one_point()?;
            o.marker("AcDbBlockPointParameter");
            o.text(303)?;
            o.text(304)?;
            o.p3(1011)?;
        }
        "BLOCKPOLARPARAMETER" => {
            o.two_point()?;
            o.marker("AcDbBlockPolarParameter");
            for c in [305, 306, 307, 308] {
                o.text(c)?;
            }
            o.bd(140)?;
            o.value_set(96, 141, 175, 309)?;
            o.value_set(97, 145, 176, 410)?;
        }
        "BLOCKROTATIONPARAMETER" => {
            o.two_point()?;
            o.marker("AcDbBlockRotationParameter");
            let point = o.r.bd3()?;
            o.text(305)?;
            o.text(306)?;
            o.point(1011, point);
            o.bd(140)?;
            o.value_set(96, 141, 175, 307)?;
        }
        "BLOCKXYPARAMETER" => {
            o.two_point()?;
            o.marker("AcDbBlockXYParameter");
            for c in [305, 306, 307, 308] {
                o.text(c)?;
            }
            o.bd(141)?;
            o.bd(140)?;
            // DWG holds the horizontal values first, DXF the vertical ones.
            let mut x = Out { r: &mut *o.r, t: Vec::new() };
            x.value_set(96, 142, 175, 410)?;
            let x = x.t;
            o.value_set(97, 146, 176, 309)?;
            o.t.extend(x);
        }
        "BLOCKUSERPARAMETER" => {
            o.one_point()?;
            o.marker("AcDbBlockUserParameter");
            o.bs(90)?;
            o.href(330)?;
            o.text(301)?;
            let code = o.r.bs()? as i16;
            o.int(70, i64::from(code));
            o.value(code, 40)?;
            o.bs(170)?;
        }
        "BLOCKANGULARCONSTRAINTPARAMETER" => {
            o.constraint()?;
            o.marker("AcDbBlockAngularConstraintParameter");
            o.p3(1011)?;
            o.p3(1012)?;
            o.text(305)?;
            o.text(306)?;
            o.bd(140)?;
            o.b(280)?;
            o.value_set(96, 128, 175, 307)?;
        }
        "BLOCKDIAMETRICCONSTRAINTPARAMETER" | "BLOCKRADIALCONSTRAINTPARAMETER" => {
            o.constraint()?;
            o.marker(if kind == "BLOCKRADIALCONSTRAINTPARAMETER" {
                "AcDbBlockRadialConstraintParameter"
            } else {
                "AcDbBlockDiametricConstraintParameter"
            });
            o.text(305)?;
            o.text(306)?;
            o.bd(140)?;
            o.value_set(96, 128, 175, 307)?;
        }
        "BLOCKALIGNEDCONSTRAINTPARAMETER"
        | "BLOCKLINEARCONSTRAINTPARAMETER"
        | "BLOCKHORIZONTALCONSTRAINTPARAMETER"
        | "BLOCKVERTICALCONSTRAINTPARAMETER" => {
            o.constraint()?;
            o.marker("AcDbBlockLinearConstraintParameter");
            o.text(305)?;
            o.text(306)?;
            o.bd(140)?;
            o.value_set(96, 128, 175, 307)?;
            match kind {
                "BLOCKALIGNEDCONSTRAINTPARAMETER" => o.marker("AcDbBlockAlignedConstraintParameter"),
                "BLOCKHORIZONTALCONSTRAINTPARAMETER" => o.marker("AcDbBlockHorizontalConstraintParameter"),
                "BLOCKVERTICALCONSTRAINTPARAMETER" => o.marker("AcDbBlockVerticalConstraintParameter"),
                _ => {}
            }
        }
        "ACDBBLOCKPARAMDEPENDENCYBODY" | "BLOCKPARAMDEPENDENCYBODY" => {
            o.marker("AcDbAssocDependencyBody");
            o.bs(90)?;
            o.marker("AcDbImpAssocDimDependencyBodyBase");
            o.bs(90)?;
            o.text(1)?;
            o.marker("AcDbBlockParameterDependencyBody");
            o.bs(90)?;
        }
        "BLOCKMOVEACTION" => {
            o.action()?;
            o.marker("AcDbBlockMoveAction");
            o.connections(92, 301, 2)?;
            o.offsets()?;
        }
        "BLOCKFLIPACTION" => {
            o.action()?;
            o.marker("AcDbBlockFlipAction");
            o.connections(92, 301, 4)?;
        }
        "BLOCKROTATEACTION" | "BLOCKSCALEACTION" => {
            o.action()?;
            o.action_with_base()?;
            // The rotate action's marker is not its class name.
            if kind == "BLOCKROTATEACTION" {
                o.marker("AcDbBlockRotationAction");
                o.connections(94, 303, 1)?;
            } else {
                o.marker("AcDbBlockScaleAction");
                o.connections(94, 303, 3)?;
                // A byte after the connections, which DXF doesn't hold. Older writers left
                // it out: read it only when it is there.
                if o.r.remaining_bits() >= 8 {
                    let _ = o.r.rc();
                }
            }
        }
        "BLOCKARRAYACTION" => {
            o.action()?;
            o.marker("AcDbBlockArrayAction");
            o.connections(92, 301, 4)?;
            o.bd(140)?;
            o.bd(141)?;
        }
        "BLOCKLOOKUPACTION" => {
            o.action()?;
            o.marker("AcDbBlockLookupAction");
            o.lookup_action()?;
        }
        "BLOCKSTRETCHACTION" => {
            o.action()?;
            o.marker("AcDbBlockStretchAction");
            o.stretch_action()?;
        }
        "BLOCKPOLARSTRETCHACTION" => {
            o.action()?;
            o.marker("AcDbBlockPolarStretchAction");
            o.polar_stretch_action()?;
        }
        "BLOCKVISIBILITYPARAMETER" => {
            o.one_point()?;
            o.marker("AcDbBlockVisibilityParameter");
            o.visibility()?;
        }
        "ACAD_EVALUATION_GRAPH" => {
            o.marker("AcDbEvalGraph");
            o.graph()?;
        }
        _ => return Err(Error::format(format!("{kind}: not a dynamic block object this reader decodes"))),
    }
    let items = o.t;
    Ok((Groups { kind: kind.to_string(), items }, refs))
}

/// The groups read so far, and the object they come from.
struct Out<'r, 'a> {
    r: &'r mut ObjectReader<'a>,
    t: Vec<(u16, XrValue)>,
}

/// A group code as the items hold it.
fn code(c: i32) -> u16 {
    u16::try_from(c).unwrap_or(0)
}

impl Out<'_, '_> {
    fn marker(&mut self, name: &str) {
        self.t.push((100, XrValue::Str(name.to_string())));
    }
    fn int(&mut self, c: i32, v: i64) {
        self.t.push((code(c), XrValue::Int64(v)));
    }
    fn real(&mut self, c: i32, v: f64) {
        self.t.push((code(c), XrValue::Real(v)));
    }
    fn string(&mut self, c: i32, s: String) {
        self.t.push((code(c), XrValue::Str(s)));
    }
    fn handle(&mut self, c: i32, h: u64) {
        self.t.push((code(c), XrValue::Handle(Handle(h))));
    }
    fn point(&mut self, c: i32, p: [f64; 3]) {
        self.real(c, p[0]);
        self.real(c + 10, p[1]);
        self.real(c + 20, p[2]);
    }
    /// A long, kept with its sign.
    fn bl(&mut self, c: i32) -> Result<i64> {
        let v = i64::from(self.r.bl()? as i32);
        self.int(c, v);
        Ok(v)
    }
    /// A short, kept with its sign.
    fn bs(&mut self, c: i32) -> Result<i64> {
        let v = i64::from(self.r.bs()? as i16);
        self.int(c, v);
        Ok(v)
    }
    fn b(&mut self, c: i32) -> Result<()> {
        let v = self.r.b()?;
        self.int(c, i64::from(v));
        Ok(())
    }
    fn bd(&mut self, c: i32) -> Result<()> {
        let v = self.r.bd()?;
        self.real(c, v);
        Ok(())
    }
    fn p3(&mut self, c: i32) -> Result<()> {
        let p = self.r.bd3()?;
        self.point(c, p);
        Ok(())
    }
    fn text(&mut self, c: i32) -> Result<()> {
        let s = self.r.text()?;
        self.string(c, s);
        Ok(())
    }
    fn href(&mut self, c: i32) -> Result<()> {
        let h = self.r.reference()?;
        self.handle(c, h);
        Ok(())
    }
    /// A list's length, refused past the object's data (each entry at least `min_bits`) or the
    /// cap.
    fn len(&mut self, n: u32, min_bits: usize, what: &str) -> Result<u32> {
        if n > MAX_ITEMS {
            return Err(Error::format(format!("{what}: {n} entries")));
        }
        let n = self.r.count(n, min_bits, what)?;
        Ok(u32::try_from(n).unwrap_or(0))
    }
    /// A list's length read as a long.
    fn bl_len(&mut self, min_bits: usize, what: &str) -> Result<u32> {
        let n = self.r.bl()?;
        self.len(n, min_bits, what)
    }

    /// The evaluation expression: its node, versions and value. DXF has nothing after the
    /// versions when there is no value; otherwise an empty expression, the value's type and the
    /// value.
    fn eval(&mut self) -> Result<()> {
        let _parent = self.r.bl()?;
        let major = i64::from(self.r.bl()? as i32);
        let minor = i64::from(self.r.bl()? as i32);
        let value_code = self.r.bs()? as i16;
        let mut value = Out { r: &mut *self.r, t: Vec::new() };
        value.value(value_code, 140)?;
        let value = value.t;
        let node = i64::from(self.r.bl()? as i32);
        self.marker("AcDbEvalExpr");
        self.int(90, node);
        self.int(98, major);
        self.int(99, minor);
        if value_code != NO_VALUE {
            self.string(1, String::new());
            self.int(70, i64::from(value_code));
            self.t.extend(value);
        }
        Ok(())
    }

    /// An expression's value of type `code`, a real in the group `real`.
    fn value(&mut self, code: i16, real: i32) -> Result<()> {
        match code {
            40 => self.bd(real)?,
            10 | 11 => {
                let p = self.r.rd2()?;
                let c = i32::from(code);
                self.real(c, p[0]);
                self.real(c + 10, p[1]);
            }
            1 => self.text(1)?,
            90 => {
                self.bl(90)?;
            }
            91 => self.href(91)?,
            70 => {
                self.bs(70)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn element(&mut self) -> Result<()> {
        self.eval()?;
        self.marker("AcDbBlockElement");
        self.text(300)?;
        self.bl(98)?;
        self.bl(99)?;
        self.bl(1071)?;
        Ok(())
    }

    fn parameter(&mut self) -> Result<()> {
        self.element()?;
        self.marker("AcDbBlockParameter");
        self.b(280)?;
        self.b(281)
    }

    /// A parameter property: its connections (value, name) after their count.
    fn property(&mut self, count_code: i32, value_code: i32, name_code: i32) -> Result<()> {
        let n = self.bl_len(4, "parameter property")?;
        self.int(count_code, i64::from(n));
        for _ in 0..n {
            self.bl(value_code)?;
            self.text(name_code)?;
        }
        Ok(())
    }

    fn one_point(&mut self) -> Result<()> {
        self.parameter()?;
        self.marker("AcDbBlock1PtParameter");
        self.p3(1010)?;
        // DXF has the count after the point, before the properties.
        let mut props = Out { r: &mut *self.r, t: Vec::new() };
        for k in 0..2 {
            props.property(170 + k, 91 + k, 301 + k)?;
        }
        let props = props.t;
        self.bl(93)?;
        self.t.extend(props);
        Ok(())
    }

    fn two_point(&mut self) -> Result<()> {
        self.parameter()?;
        self.marker("AcDbBlock2PtParameter");
        self.p3(1010)?;
        self.p3(1011)?;
        let mut props = Out { r: &mut *self.r, t: Vec::new() };
        for k in 0..4 {
            props.property(171 + k, 92 + k, 301 + k)?;
        }
        let props = props.t;
        self.int(170, 4);
        for _ in 0..4 {
            self.bl(91)?;
        }
        self.t.extend(props);
        self.bs(177)?;
        Ok(())
    }

    fn constraint(&mut self) -> Result<()> {
        self.two_point()?;
        self.marker("AcDbBlockConstraintParameter");
        self.href(330)
    }

    fn grip(&mut self) -> Result<()> {
        self.element()?;
        self.marker("AcDbBlockGrip");
        self.bl(91)?;
        self.bl(92)?;
        self.p3(1010)?;
        self.b(280)?;
        self.bl(93)?;
        Ok(())
    }

    /// An oriented grip's direction, in groups 140, 141 and 142.
    fn vector(&mut self, c: i32) -> Result<()> {
        let v = self.r.bd3()?;
        self.real(c, v[0]);
        self.real(c + 1, v[1]);
        self.real(c + 2, v[2]);
        Ok(())
    }

    fn action(&mut self) -> Result<()> {
        self.element()?;
        self.marker("AcDbBlockAction");
        let location = self.r.bd3()?;
        let n = self.bl_len(0, "action dependencies")?;
        let mut deps = Vec::new();
        for _ in 0..n {
            deps.push(self.r.reference()?);
        }
        let n = self.bl_len(2, "action ids")?;
        self.int(70, i64::from(n));
        for _ in 0..n {
            self.bl(91)?;
        }
        self.int(71, i64::try_from(deps.len()).unwrap_or(0));
        for h in deps {
            self.handle(330, h);
        }
        self.point(1010, location);
        Ok(())
    }

    fn action_with_base(&mut self) -> Result<()> {
        self.marker("AcDbBlockActionWithBasePt");
        let offset = self.r.bd3()?;
        self.connections(92, 301, 2)?;
        self.point(1011, offset);
        self.b(280)?;
        self.p3(1012)
    }

    /// `n` connections: DXF has their values (groups `code`, `code + 1`…), then their names.
    fn connections(&mut self, code: i32, name_code: i32, n: i32) -> Result<()> {
        let mut names = Vec::new();
        for k in 0..n {
            self.bl(code + k)?;
            names.push(self.r.text()?);
        }
        for (k, name) in (0..n).zip(names) {
            self.string(name_code + k, name);
        }
        Ok(())
    }

    /// A parameter's allowed values: its description (only in DXF), flags, minimum, maximum,
    /// increment and list.
    fn value_set(&mut self, flags: i32, real: i32, count_code: i32, description: i32) -> Result<()> {
        self.string(description, String::new());
        self.bl(flags)?;
        self.bd(real)?;
        self.bd(real + 1)?;
        self.bd(real + 2)?;
        let n = self.r.bs()?;
        let n = self.len(u32::from(n), 2, "value list")?;
        self.int(count_code, i64::from(n));
        for _ in 0..n {
            self.bd(real + 3)?;
        }
        Ok(())
    }

    /// An action's distance multiplier, angle offset and flags.
    fn offsets(&mut self) -> Result<()> {
        self.bd(140)?;
        self.bd(141)?;
        let flags = self.r.rc()?;
        self.int(280, i64::from(flags));
        Ok(())
    }

    fn lookup_action(&mut self) -> Result<()> {
        let rows = self.r.bl()?;
        let cols = self.r.bl()?;
        if rows > MAX_ITEMS || cols > MAX_ITEMS {
            return Err(Error::format(format!("lookup table of {rows} by {cols}")));
        }
        let cells = rows.checked_mul(cols).filter(|&n| n <= MAX_ITEMS).ok_or_else(|| Error::format(format!("lookup table of {rows} by {cols}")))?;
        let cells = self.len(cells, 2, "lookup cells")?;
        let cols = self.len(cols, 12, "lookup columns")?;
        self.int(92, i64::from(rows));
        self.int(93, i64::from(cols));
        self.string(301, String::new());
        for _ in 0..cells {
            self.text(302)?;
        }
        for _ in 0..cols {
            self.string(303, String::new());
            self.bl(94)?;
            self.bl(95)?;
            self.bl(96)?;
            self.b(282)?;
            self.text(305)?;
            self.b(281)?;
            self.text(304)?;
        }
        self.b(280)
    }

    fn stretch_action(&mut self) -> Result<()> {
        self.connections(92, 301, 2)?;
        let n = self.bl_len(128, "stretch points")?;
        self.int(72, i64::from(n));
        for _ in 0..n {
            let p = self.r.rd2()?;
            self.real(1011, p[0]);
            self.real(1021, p[1]);
        }
        let n = self.bl_len(2, "stretch objects")?;
        self.int(73, i64::from(n));
        for _ in 0..n {
            self.href(331)?;
            let k = self.r.bs()?;
            let k = self.len(u32::from(k), 2, "stretch object vertices")?;
            self.int(74, i64::from(k));
            for _ in 0..k {
                self.bl(94)?;
            }
        }
        let n = self.bl_len(4, "stretch codes")?;
        self.int(75, i64::from(n));
        for _ in 0..n {
            self.bl(95)?;
            let k = self.r.bs()?;
            let k = self.len(u32::from(k), 2, "stretch code vertices")?;
            self.int(76, i64::from(k));
            for _ in 0..k {
                self.bl(94)?;
            }
        }
        self.offsets()
    }

    fn polar_stretch_action(&mut self) -> Result<()> {
        self.connections(92, 301, 6)?;
        let n = self.bl_len(128, "polar stretch points")?;
        self.int(73, i64::from(n));
        for _ in 0..n {
            let p = self.r.rd2()?;
            self.real(1011, p[0]);
            self.real(1021, p[1]);
        }
        let n = self.bl_len(0, "polar stretch objects")?;
        self.int(72, i64::from(n));
        for _ in 0..n {
            self.href(331)?;
        }
        let n = self.bl_len(2, "polar stretch bindings")?;
        self.int(74, i64::from(n));
        for _ in 0..n {
            self.href(332)?;
            let k = self.bl_len(2, "polar stretch binding vertices")?;
            self.int(75, i64::from(k));
            for _ in 0..k {
                self.bl(76)?;
            }
        }
        let n = self.bl_len(4, "polar stretch codes")?;
        self.int(78, i64::from(n));
        for _ in 0..n {
            self.bl(98)?;
            let k = self.bl_len(2, "polar stretch code vertices")?;
            self.int(79, i64::from(k));
            for _ in 0..k {
                self.bl(76)?;
            }
        }
        self.bd(141)?;
        self.bd(140)?;
        // Ids DXF has no groups for: read past, not kept.
        let n = self.bl_len(2, "polar stretch ids")?;
        for _ in 0..n {
            let _ = self.r.bl()?;
        }
        self.int(77, 0);
        Ok(())
    }

    /// A visibility parameter's own fields: whether it is set up, its name and description, the
    /// block's objects it governs, and its states with the objects and parameters each shows.
    fn visibility(&mut self) -> Result<()> {
        self.b(281)?;
        self.text(301)?;
        self.text(302)?;
        self.b(91)?;
        let n = self.bl_len(0, "visibility objects")?;
        self.int(93, i64::from(n));
        for _ in 0..n {
            self.href(331)?;
        }
        let states = self.bl_len(6, "visibility states")?;
        self.int(92, i64::from(states));
        for _ in 0..states {
            self.text(303)?;
            for (n_code, h_code) in [(94, 332), (95, 333)] {
                let n = self.bl_len(0, "visibility state members")?;
                self.int(n_code, i64::from(n));
                for _ in 0..n {
                    self.href(h_code)?;
                }
            }
        }
        Ok(())
    }

    /// The evaluation graph: its first node, its nodes (each with the expression it owns) and
    /// its edges.
    fn graph(&mut self) -> Result<()> {
        self.bl(96)?;
        self.bl(97)?;
        let nodes = self.bl_len(14, "graph nodes")?;
        for _ in 0..nodes {
            self.bl(91)?;
            self.bl(93)?;
            self.bl(95)?;
            self.href(360)?;
            for _ in 0..4 {
                self.bl(92)?;
            }
        }
        let edges = self.bl_len(20, "graph edges")?;
        for _ in 0..edges {
            for c in [92, 93, 94, 91, 91, 92, 92, 92, 92, 92] {
                self.bl(c)?;
            }
        }
        Ok(())
    }
}
