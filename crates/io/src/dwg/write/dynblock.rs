//! A dynamic block's objects: its evaluation graph and the parameters, grips and actions in it.
//!
//! Each is an evaluation expression (`AcDbEvalExpr`), most a block element after it, then the
//! fields of its kind. The DXF record lists them subclass by subclass; the DWG object holds the
//! same values in a fixed order, with a few lists of fixed length (a two-point parameter's four
//! properties, an action's connections) and none of the counts DXF adds. The layouts follow the
//! one acadrust (MPL-2.0) writes, whose files AutoCAD opens.

use draftcraft_dwg::compose::{ObjectWriter, Ref};
use draftcraft_dxf::Tag;

use super::Cx;
use super::objects::begin;
use super::rec::{Groups, Rec, hex, text};

/// Most entries of any list in one object (a stretch's points, a lookup's cells, a graph's
/// nodes): more is a damaged record.
const MAX_ITEMS: usize = 1 << 16;

/// An expression's "no value" type: nothing follows it.
const NO_VALUE: i64 = -9999;

/// The groups of one subclass, read in order: each value is the next group of its code after the
/// last one read; a group that isn't there reads as its default and moves nothing.
struct Cur<'a> {
    t: &'a [Tag],
    i: usize,
}

impl<'a> Cur<'a> {
    fn new(g: Groups<'a>) -> Cur<'a> {
        Cur { t: g.0, i: 0 }
    }
    fn next(&mut self, code: i32) -> Option<&'a Tag> {
        let rest = self.t.get(self.i..)?;
        let j = rest.iter().position(|t| t.code == code)?;
        self.i = self.i.saturating_add(j).saturating_add(1);
        rest.get(j)
    }
    /// The code of the group after the last one read.
    fn peek(&self) -> Option<i32> {
        self.t.get(self.i).map(|t| t.code)
    }
    fn skip(&mut self) {
        self.i = self.i.saturating_add(1);
    }
    fn i(&mut self, code: i32) -> i64 {
        self.next(code).map_or(0, Tag::i64)
    }
    fn f(&mut self, code: i32) -> f64 {
        self.next(code).map_or(0.0, Tag::f64).finite_or_zero()
    }
    fn s(&mut self, code: i32) -> &'a str {
        self.next(code).map_or("", text)
    }
    fn h(&mut self, code: i32) -> u64 {
        self.next(code).map_or(0, |t| hex(text(t)))
    }
    /// A point: the group `code` and the groups 10 and 20 after it.
    fn p3(&mut self, code: i32) -> [f64; 3] {
        let x = self.f(code);
        let mut p = [x, 0.0, 0.0];
        for (k, c) in [(1, code + 10), (2, code + 20)] {
            if self.peek() == Some(c) {
                p[k] = self.f(c);
            }
        }
        p
    }
    fn p2(&mut self, code: i32) -> [f64; 2] {
        let x = self.f(code);
        let y = if self.peek() == Some(code + 10) { self.f(code + 10) } else { 0.0 };
        [x, y]
    }
}

trait FiniteOrZero {
    fn finite_or_zero(self) -> f64;
}
impl FiniteOrZero for f64 {
    fn finite_or_zero(self) -> f64 {
        if self.is_finite() { self } else { 0.0 }
    }
}

fn bl(w: &mut ObjectWriter, v: i64) {
    w.bl(i32::try_from(v).unwrap_or(0) as u32);
}
fn bs(w: &mut ObjectWriter, v: i64) {
    w.bs(i16::try_from(v).unwrap_or(0) as u16);
}
fn count(n: usize) -> u32 {
    u32::try_from(n.min(MAX_ITEMS)).unwrap_or(0)
}
fn clamp(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0).min(MAX_ITEMS)
}

/// The dynamic block object `r`. `None` for a kind with no layout here.
pub(super) fn object(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let kind = r.kind.to_ascii_uppercase();
    let code = cx.class(&kind)?;
    let mut w = begin(cx, code, r);
    let mut x = Ctx { cx, r, w: &mut w };
    match kind.as_str() {
        "ACDB_BLOCKREPRESENTATION_DATA" | "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION" => {
            let g = r.own();
            bs(x.w, g.i(70));
            x.w.reference(Ref::HardPointer, cx.known(g.h(340)));
        }
        "ACDB_DYNAMICBLOCKPROXYNODE" | "DYNAMICBLOCKPROXYNODE" => x.eval(),
        "BLOCKGRIPLOCATIONCOMPONENT" => {
            x.eval();
            let mut g = Cur::new(r.sub("AcDbBlockGripExpr"));
            bl(x.w, g.i(91));
            x.w.text(g.s(300));
        }
        "BLOCKALIGNMENTGRIP" | "BLOCKLINEARGRIP" => {
            x.grip();
            x.orientation();
        }
        "BLOCKFLIPGRIP" => {
            x.grip();
            bl(x.w, r.sub("AcDbBlockFlipGrip").i(93));
            x.orientation();
        }
        "BLOCKLOOKUPGRIP" | "BLOCKPOLARGRIP" | "BLOCKROTATIONGRIP" | "BLOCKVISIBILITYGRIP" | "BLOCKXYGRIP" | "BLOCKPROPERTIESTABLEGRIP" => x.grip(),
        "BLOCKALIGNMENTPARAMETER" => {
            x.two_point();
            x.w.b(r.sub("AcDbBlockAlignmentParameter").i(280) != 0);
        }
        "BLOCKBASEPOINTPARAMETER" => {
            x.one_point();
            let mut g = Cur::new(r.sub("AcDbBlockBasepointParameter"));
            x.w.bd3(g.p3(1011));
            x.w.bd3(g.p3(1012));
        }
        "BLOCKFLIPPARAMETER" => {
            x.two_point();
            let mut g = Cur::new(r.sub("AcDbBlockFlipParameter"));
            for c in [305, 306, 307, 308] {
                x.w.text(g.s(c));
            }
            x.w.bd3(Cur::new(r.sub("AcDbBlockFlipParameter")).p3(1012));
            bl(x.w, r.sub("AcDbBlockFlipParameter").i(96));
            x.w.text(r.sub("AcDbBlockFlipParameter").s(309));
        }
        "BLOCKLINEARPARAMETER" => {
            x.two_point();
            let g = r.sub("AcDbBlockLinearParameter");
            x.w.text(g.s(305));
            x.w.text(g.s(306));
            x.w.bd(g.f(140));
            value_set(x.w, g, 96, 141, 175);
        }
        "BLOCKLOOKUPPARAMETER" => {
            x.one_point();
            let g = r.sub("AcDbBlockLookupParameter");
            bl(x.w, g.i(94));
            x.w.text(g.s(303));
            x.w.text(g.s(304));
        }
        "BLOCKPOINTPARAMETER" => {
            x.one_point();
            let g = r.sub("AcDbBlockPointParameter");
            x.w.text(g.s(303));
            x.w.text(g.s(304));
            x.w.bd3(Cur::new(g).p3(1011));
        }
        "BLOCKPOLARPARAMETER" => {
            x.two_point();
            let g = r.sub("AcDbBlockPolarParameter");
            for c in [305, 306, 307, 308] {
                x.w.text(g.s(c));
            }
            x.w.bd(g.f(140));
            value_set(x.w, g, 96, 141, 175);
            value_set(x.w, g, 97, 145, 176);
        }
        "BLOCKROTATIONPARAMETER" => {
            x.two_point();
            let g = r.sub("AcDbBlockRotationParameter");
            x.w.bd3(Cur::new(g).p3(1011));
            x.w.text(g.s(305));
            x.w.text(g.s(306));
            x.w.bd(g.f(140));
            value_set(x.w, g, 96, 141, 175);
        }
        "BLOCKXYPARAMETER" => {
            x.two_point();
            let g = r.sub("AcDbBlockXYParameter");
            for c in [305, 306, 307, 308] {
                x.w.text(g.s(c));
            }
            x.w.bd(g.f(141));
            x.w.bd(g.f(140));
            value_set(x.w, g, 96, 142, 175);
            value_set(x.w, g, 97, 146, 176);
        }
        "BLOCKUSERPARAMETER" => {
            x.one_point();
            let mut g = Cur::new(r.sub("AcDbBlockUserParameter"));
            bs(x.w, g.i(90));
            let var = g.h(330);
            x.w.reference(Ref::HardPointer, cx.known(var));
            x.w.text(g.s(301));
            let code = g.next(70).map_or(0, Tag::i64);
            bs(x.w, code);
            x.value(&mut g, code, 40);
            bs(x.w, g.i(170));
        }
        "BLOCKANGULARCONSTRAINTPARAMETER" => {
            x.constraint();
            let g = r.sub("AcDbBlockAngularConstraintParameter");
            x.w.bd3(Cur::new(g).p3(1011));
            x.w.bd3(Cur::new(g).p3(1012));
            x.w.text(g.s(305));
            x.w.text(g.s(306));
            x.w.bd(g.f(140));
            x.w.b(g.i(280) != 0);
            value_set(x.w, g, 96, 128, 175);
        }
        "BLOCKDIAMETRICCONSTRAINTPARAMETER" | "BLOCKRADIALCONSTRAINTPARAMETER" => {
            x.constraint();
            let marker =
                if kind.starts_with("BLOCKDIAMETRIC") { "AcDbBlockDiametricConstraintParameter" } else { "AcDbBlockRadialConstraintParameter" };
            let g = r.sub(marker);
            x.w.text(g.s(305));
            x.w.text(g.s(306));
            x.w.bd(g.f(140));
            value_set(x.w, g, 96, 128, 175);
        }
        "BLOCKALIGNEDCONSTRAINTPARAMETER"
        | "BLOCKLINEARCONSTRAINTPARAMETER"
        | "BLOCKHORIZONTALCONSTRAINTPARAMETER"
        | "BLOCKVERTICALCONSTRAINTPARAMETER" => {
            x.constraint();
            let g = r.sub("AcDbBlockLinearConstraintParameter");
            x.w.text(g.s(305));
            x.w.text(g.s(306));
            x.w.bd(g.f(140));
            value_set(x.w, g, 96, 128, 175);
        }
        "ACDBBLOCKPARAMDEPENDENCYBODY" | "BLOCKPARAMDEPENDENCYBODY" => {
            bs(x.w, r.sub("AcDbAssocDependencyBody").i(90));
            let base = r.sub("AcDbImpAssocDimDependencyBodyBase");
            bs(x.w, base.i(90));
            x.w.text(base.s(1));
            bs(x.w, r.sub("AcDbBlockParameterDependencyBody").i(90));
        }
        "BLOCKMOVEACTION" => {
            x.action();
            let g = r.sub("AcDbBlockMoveAction");
            connections(x.w, g, 92, 301, 2);
            offsets(x.w, g);
        }
        "BLOCKFLIPACTION" => {
            x.action();
            connections(x.w, r.sub("AcDbBlockFlipAction"), 92, 301, 4);
        }
        "BLOCKROTATEACTION" => {
            x.action_with_base();
            connections(x.w, r.sub("AcDbBlockRotationAction"), 94, 303, 1);
        }
        "BLOCKSCALEACTION" => {
            x.action_with_base();
            connections(x.w, r.sub("AcDbBlockScaleAction"), 94, 303, 3);
            // The byte AutoCAD reads after the connections.
            x.w.rc(0);
        }
        "BLOCKARRAYACTION" => {
            x.action();
            let g = r.sub("AcDbBlockArrayAction");
            connections(x.w, g, 92, 301, 4);
            x.w.bd(g.f(140));
            x.w.bd(g.f(141));
        }
        "BLOCKLOOKUPACTION" => {
            x.action();
            x.lookup_action();
        }
        "BLOCKSTRETCHACTION" => {
            x.action();
            x.stretch_action();
        }
        "BLOCKPOLARSTRETCHACTION" => {
            x.action();
            x.polar_stretch_action();
        }
        "BLOCKVISIBILITYPARAMETER" => {
            x.one_point();
            x.visibility();
        }
        "ACAD_EVALUATION_GRAPH" => x.graph(),
        _ => return None,
    }
    Some(w)
}

/// What every encoding here reads from and writes to.
struct Ctx<'c, 'a, 'w> {
    cx: &'c Cx<'a>,
    r: &'c Rec<'a>,
    w: &'w mut ObjectWriter,
}

impl Ctx<'_, '_, '_> {
    /// The evaluation expression: its parent (none in a DXF file), versions, value and node.
    fn eval(&mut self) {
        let mut g = Cur::new(self.r.sub("AcDbEvalExpr"));
        let node = g.i(90);
        bl(self.w, -1);
        bl(self.w, g.i(98));
        bl(self.w, g.i(99));
        let code = g.next(70).map_or(NO_VALUE, Tag::i64);
        bs(self.w, code);
        self.value(&mut g, code, 140);
        bl(self.w, node);
    }

    /// An expression's value of type `code`, a real in the group `real`.
    fn value(&mut self, g: &mut Cur<'_>, code: i64, real: i32) {
        match code {
            40 => self.w.bd(g.f(real)),
            10 | 11 => {
                let c = i32::try_from(code).unwrap_or(10);
                self.w.rd2(g.p2(c));
            }
            1 => self.w.text(g.s(1)),
            90 => bl(self.w, g.i(90)),
            91 => {
                let h = g.h(91);
                self.w.reference(Ref::HardPointer, self.cx.known(h));
            }
            70 => bs(self.w, g.i(70)),
            _ => {}
        }
    }

    fn element(&mut self) {
        self.eval();
        let g = self.r.sub("AcDbBlockElement");
        self.w.text(g.s(300));
        bl(self.w, g.i(98));
        bl(self.w, g.i(99));
        bl(self.w, g.i(1071));
    }

    fn parameter(&mut self) {
        self.element();
        let g = self.r.sub("AcDbBlockParameter");
        self.w.b(g.i(280) != 0);
        self.w.b(g.i(281) != 0);
    }

    fn one_point(&mut self) {
        self.parameter();
        let g = self.r.sub("AcDbBlock1PtParameter");
        self.w.bd3(Cur::new(g).p3(1010));
        for k in 0..2 {
            property(self.w, g, 170 + k, 91 + k, 301 + k);
        }
        bl(self.w, g.i(93));
    }

    fn two_point(&mut self) {
        self.parameter();
        let g = self.r.sub("AcDbBlock2PtParameter");
        self.w.bd3(Cur::new(g).p3(1010));
        self.w.bd3(Cur::new(g).p3(1011));
        for k in 0..4 {
            property(self.w, g, 171 + k, 92 + k, 301 + k);
        }
        // The four states follow their count (group 170).
        let mut c = Cur::new(g);
        let at_states = c.next(170).is_some();
        for _ in 0..4 {
            let state = if at_states && c.peek() == Some(91) { c.i(91) } else { 0 };
            bl(self.w, state);
        }
        bs(self.w, g.i(177));
    }

    fn constraint(&mut self) {
        self.two_point();
        let dep = self.r.sub("AcDbBlockConstraintParameter").h(330);
        self.w.reference(Ref::HardPointer, self.cx.known(dep));
    }

    fn grip(&mut self) {
        self.element();
        let g = self.r.sub("AcDbBlockGrip");
        bl(self.w, g.i(91));
        bl(self.w, g.i(92));
        self.w.bd3(Cur::new(g).p3(1010));
        self.w.b(g.i(280) != 0);
        bl(self.w, g.i(93));
    }

    /// An oriented grip's direction: groups 140, 141 and 142 of its last subclass.
    fn orientation(&mut self) {
        let last = self.r.tags.iter().rposition(|t| t.code == 100).map_or(self.r.tags, |i| self.r.tags.get(i + 1..).unwrap_or(&[]));
        let g = Groups(last);
        self.w.bd3([g.f(140), g.f(141), g.f(142)]);
    }

    fn action(&mut self) {
        self.element();
        let g = self.r.sub("AcDbBlockAction");
        self.w.bd3(Cur::new(g).p3(1010));
        let deps: Vec<u64> = g.all(330).take(MAX_ITEMS).map(|t| hex(text(t))).collect();
        self.w.bl(count(deps.len()));
        for h in deps {
            self.w.reference(Ref::SoftPointer, self.cx.known(h));
        }
        let ids: Vec<i64> = g.all(91).take(MAX_ITEMS).map(Tag::i64).collect();
        self.w.bl(count(ids.len()));
        for id in ids {
            bl(self.w, id);
        }
    }

    fn action_with_base(&mut self) {
        self.action();
        let g = self.r.sub("AcDbBlockActionWithBasePt");
        self.w.bd3(Cur::new(g).p3(1011));
        connections(self.w, g, 92, 301, 2);
        self.w.b(g.i(280) != 0);
        self.w.bd3(Cur::new(g).p3(1012));
    }

    fn lookup_action(&mut self) {
        let g = self.r.sub("AcDbBlockLookupAction");
        let (rows, cols) = (g.i(92), g.i(93));
        bl(self.w, rows);
        bl(self.w, cols);
        let cells = clamp(rows).saturating_mul(clamp(cols)).min(MAX_ITEMS);
        let mut exprs = g.all(302);
        for _ in 0..cells {
            self.w.text(exprs.next().map_or("", text));
        }
        // Each column from its group 303 on.
        let mut c = Cur::new(g);
        for _ in 0..clamp(cols) {
            let _ = c.next(303);
            bl(self.w, c.i(94));
            bl(self.w, c.i(95));
            bl(self.w, c.i(96));
            self.w.b(c.i(282) != 0);
            self.w.text(c.s(305));
            self.w.b(c.i(281) != 0);
            self.w.text(c.s(304));
        }
        self.w.b(c.i(280) != 0);
    }

    fn stretch_action(&mut self) {
        let g = self.r.sub("AcDbBlockStretchAction");
        connections(self.w, g, 92, 301, 2);
        let mut c = Cur::new(g);
        let n = clamp(c.i(72));
        self.w.bl(count(n));
        for _ in 0..n {
            self.w.rd2(c.p2(1011));
        }
        let n = clamp(c.i(73));
        self.w.bl(count(n));
        for _ in 0..n {
            let h = c.h(331);
            self.w.reference(Ref::SoftPointer, self.cx.known(h));
            let k = clamp(c.i(74));
            self.w.bs(u16::try_from(k).unwrap_or(0));
            for _ in 0..k {
                bl(self.w, c.i(94));
            }
        }
        let n = clamp(c.i(75));
        self.w.bl(count(n));
        for _ in 0..n {
            bl(self.w, c.i(95));
            let k = clamp(c.i(76));
            self.w.bs(u16::try_from(k).unwrap_or(0));
            for _ in 0..k {
                bl(self.w, c.i(94));
            }
        }
        offsets(self.w, g);
    }

    fn polar_stretch_action(&mut self) {
        let g = self.r.sub("AcDbBlockPolarStretchAction");
        connections(self.w, g, 92, 301, 6);
        let mut c = Cur::new(g);
        let n = clamp(c.i(73));
        self.w.bl(count(n));
        for _ in 0..n {
            self.w.rd2(c.p2(1011));
        }
        let n = clamp(c.i(72));
        self.w.bl(count(n));
        for _ in 0..n {
            let h = c.h(331);
            self.w.reference(Ref::SoftPointer, self.cx.known(h));
        }
        let n = clamp(c.i(74));
        self.w.bl(count(n));
        for _ in 0..n {
            let h = c.h(332);
            self.w.reference(Ref::SoftPointer, self.cx.known(h));
            let k = clamp(c.i(75));
            self.w.bl(count(k));
            for _ in 0..k {
                bl(self.w, c.i(76));
            }
        }
        let n = clamp(c.i(78));
        self.w.bl(count(n));
        for _ in 0..n {
            bl(self.w, c.i(98));
            let k = clamp(c.i(79));
            self.w.bl(count(k));
            for _ in 0..k {
                bl(self.w, c.i(76));
            }
        }
        self.w.bd(g.f(141));
        self.w.bd(g.f(140));
        self.w.bl(0);
    }

    /// A visibility parameter's own fields: whether it is set up, its name and description, the
    /// block's objects it governs, and its states with the objects and parameters each shows.
    fn visibility(&mut self) {
        let mut c = Cur::new(self.r.sub("AcDbBlockVisibilityParameter"));
        self.w.b(c.i(281) != 0);
        self.w.text(c.s(301));
        self.w.text(c.s(302));
        self.w.b(c.i(91) != 0);
        let n = clamp(c.i(93));
        self.w.bl(count(n));
        for _ in 0..n {
            let h = c.h(331);
            self.w.reference(Ref::SoftPointer, self.cx.known(h));
        }
        let states = clamp(c.i(92));
        self.w.bl(count(states));
        for _ in 0..states {
            self.w.text(c.s(303));
            for (n_code, h_code) in [(94, 332), (95, 333)] {
                let n = clamp(c.i(n_code));
                self.w.bl(count(n));
                for _ in 0..n {
                    let h = c.h(h_code);
                    self.w.reference(Ref::SoftPointer, self.cx.known(h));
                }
            }
        }
    }

    /// The evaluation graph: its first node, then its nodes (each with the expression it owns)
    /// and its edges.
    fn graph(&mut self) {
        let g = self.r.sub("AcDbEvalGraph");
        let mut c = Cur::new(g);
        bl(self.w, c.i(96));
        bl(self.w, c.i(97));
        // Nodes start with a group 91, edges with a group 92.
        let mut nodes: Vec<(i64, i64, i64, u64, [i64; 4])> = Vec::new();
        while c.peek() == Some(91) && nodes.len() < MAX_ITEMS {
            let id = c.i(91);
            let flags = if c.peek() == Some(93) { c.i(93) } else { 0 };
            let next = if c.peek() == Some(95) { c.i(95) } else { 0 };
            let expr = if c.peek() == Some(360) { c.h(360) } else { 0 };
            let mut data = [0i64; 4];
            for v in &mut data {
                if c.peek() == Some(92) && !starts_edge(&c) {
                    *v = c.i(92);
                }
            }
            if c.peek() == Some(290) {
                c.skip();
            }
            nodes.push((id, flags, next, expr, data));
        }
        self.w.bl(count(nodes.len()));
        for (id, flags, next, expr, data) in nodes {
            bl(self.w, id);
            bl(self.w, flags);
            bl(self.w, next);
            self.w.reference(Ref::HardOwner, self.cx.known(expr));
            for v in data {
                bl(self.w, v);
            }
        }
        let mut edges: Vec<[i64; 10]> = Vec::new();
        while c.peek() == Some(92) && edges.len() < MAX_ITEMS {
            let mut e = [0i64; 10];
            for (slot, code) in e.iter_mut().zip([92, 93, 94, 91, 91, 92, 92, 92, 92, 92]) {
                if c.peek() == Some(code) {
                    *slot = c.i(code);
                }
            }
            edges.push(e);
        }
        self.w.bl(count(edges.len()));
        for e in edges {
            for v in e {
                bl(self.w, v);
            }
        }
    }
}

/// Whether the group 92 the cursor is at starts an edge (92 93 94) rather than continuing a
/// node's data.
fn starts_edge(c: &Cur<'_>) -> bool {
    let code = |k: usize| c.t.get(c.i.saturating_add(k)).map(|t| t.code);
    code(1) == Some(93) && code(2) == Some(94)
}

/// One property of a parameter: its connections (value, name), from the count `count_code`, the
/// values `value_code` and the names `name_code`.
fn property(w: &mut ObjectWriter, g: Groups<'_>, count_code: i32, value_code: i32, name_code: i32) {
    let n = clamp(g.i(count_code));
    let values: Vec<i64> = g.all(value_code).take(n).map(Tag::i64).collect();
    let names: Vec<&str> = g.all(name_code).take(n).map(text).collect();
    w.bl(count(n));
    for k in 0..n {
        bl(w, values.get(k).copied().unwrap_or(0));
        w.text(names.get(k).copied().unwrap_or(""));
    }
}

/// `n` connections: the values in groups `code`, `code + 1`… and the names in `name_code`,
/// `name_code + 1`…
fn connections(w: &mut ObjectWriter, g: Groups<'_>, code: i32, name_code: i32, n: i32) {
    for k in 0..n {
        bl(w, g.i(code + k));
        w.text(g.s(name_code + k));
    }
}

/// A parameter's allowed values: flags, minimum, maximum, increment and the list.
fn value_set(w: &mut ObjectWriter, g: Groups<'_>, flags: i32, real: i32, count_code: i32) {
    bl(w, g.i(flags));
    w.bd(g.f(real));
    w.bd(g.f(real + 1));
    w.bd(g.f(real + 2));
    let n = clamp(g.i(count_code)).min(usize::from(u16::MAX >> 1));
    let values: Vec<f64> = g.all(real + 3).take(n).map(Tag::f64).collect();
    w.bs(u16::try_from(n).unwrap_or(0));
    for k in 0..n {
        w.bd(values.get(k).copied().unwrap_or(0.0));
    }
}

/// An action's distance multiplier, angle offset and flags.
fn offsets(w: &mut ObjectWriter, g: Groups<'_>) {
    w.bd(g.f(140));
    w.bd(g.f(141));
    w.rc(u8::try_from(g.i(280)).unwrap_or(0));
}
