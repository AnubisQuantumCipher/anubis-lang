//! The abstract values of the information-flow interpreter.
//!
//! One abstract value [`V`] stands for every runtime `AnubisValue` a program point may hold
//! (`backends/run.rs`, `enum AnubisValue`). The runtime has pure value semantics: every read
//! clones, heap kinds are copy-on-write, and there is no aliasing, so an abstract value can be a
//! tree mirroring the runtime value, with a label at each node:
//!
//! - `lab` is the node's own information: a scalar's content, a list's length, a map's key set, a
//!   struct's or enum's type and tag, which function a function value is.
//! - the children carry their own labels (list elements, map values, struct fields, enum
//!   payloads). The element at a list position carries what decides it, so a list's order is in
//!   its elements' labels.
//!
//! A value may be of several kinds at once (a join of a struct and a closure, `Some` and `None`),
//! so every kind has its own component; the empty value (every component empty) is bottom: no
//! runtime value reaches the point.
//!
//! Closures capture a snapshot of their free names when they are created (`run.rs`, `Expr::Lambda`),
//! so a closure is its lambda and the captured values. Closures nested more deeply than
//! [`CLOSURE_DEPTH`] in each other's captures are summarized per lambda and reach label (see
//! `Clo::Summary`), which bounds a chain such as `c = |z| c(z) + 1` in a loop; `compose` pairs
//! nested that deeply become a chain (`FnSet::chain`).

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// A security label: confidentiality (`SEC`) and integrity (`TNT`) bits. Joining is bitwise or.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct Lab(pub u8);

impl Lab {
    pub const PUB: Lab = Lab(0);
    pub const SEC: Lab = Lab(1);
    pub const TNT: Lab = Lab(2);

    pub fn join(self, o: Lab) -> Lab {
        Lab(self.0 | o.0)
    }
    pub fn secret(self) -> bool {
        self.0 & Self::SEC.0 != 0
    }
    pub fn tainted(self) -> bool {
        self.0 & Self::TNT.0 != 0
    }
    /// The part of a label that decides control: only confidentiality. Integrity is checked for
    /// explicit flows only (the existing lanes' policy), so a tainted condition does not taint
    /// what it controls.
    pub fn control(self) -> Lab {
        Lab(self.0 & Self::SEC.0)
    }
    pub fn without(self, o: Lab) -> Lab {
        Lab(self.0 & !o.0)
    }
}

/// Deepest nesting of data an abstract value keeps; below it a value becomes `top` with the
/// join of what it held.
pub(crate) const DATA_DEPTH: usize = 32;
/// Closures nested in each other's captures deeper than this are summarized per lambda.
pub(crate) const CLOSURE_DEPTH: usize = 3;
/// Most nodes an abstract value keeps; a wider one first forgets list positions, then is folded
/// at a shallower depth.
pub(crate) const MAX_NODES: usize = 256;

/// A lambda of the program, by the index of its node in the interpreter's table.
pub(crate) type LamId = u32;

/// The captured values of a closure (its free names at creation).
pub(crate) type Env = BTreeMap<String, V>;

thread_local! {
    /// Work done on abstract values (nodes joined, compared, widened, raised, chosen): the
    /// interpreter's budget counts it, so a value that grows expensive fails closed instead of
    /// running on.
    static WORK: Cell<u64> = const { Cell::new(0) };
    /// Closure snapshots that joined into, or were compared against, a per-lambda summary: the
    /// interpreter adds them to its summaries (a join has no access to them).
    static PENDING: RefCell<Vec<(LamId, Lab, Rc<Env>)>> = const { RefCell::new(Vec::new()) };
}

fn charge(n: u64) {
    WORK.with(|w| w.set(w.get().saturating_add(n)));
}

/// The work counted so far (see `WORK`).
pub(crate) fn work() -> u64 {
    WORK.with(|w| w.get())
}

/// Start counting afresh (one analysis).
pub(crate) fn reset() {
    WORK.with(|w| w.set(0));
    PENDING.with(|p| p.borrow_mut().clear());
}

/// The snapshots summarized since the last call.
pub(crate) fn take_pending() -> Vec<(LamId, Lab, Rc<Env>)> {
    PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

fn pend(id: LamId, sig: Lab, env: &Rc<Env>) {
    PENDING.with(|p| p.borrow_mut().push((id, sig, env.clone())));
}

/// What a closure captured.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) enum Clo {
    /// The snapshot, as captured.
    Env(Rc<Env>),
    /// Snapshots of this lambda that were summarized, by their reach label ([`env_reach`]): the
    /// interpreter keeps one joined environment per (lambda, reach label), so closures that touch
    /// no secret are not summarized together with ones that do. Used past `CLOSURE_DEPTH`.
    Summary(BTreeSet<Lab>),
}

/// A function value: which functions it may be.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct FnSet {
    /// Free user functions named as values (`var_as_value`: a closure calling `anb_f`).
    pub user: BTreeSet<Rc<str>>,
    /// Builtins named as values (a closure dispatching on the argument count).
    pub builtins: BTreeSet<Rc<str>>,
    /// Closures, by lambda.
    pub clos: BTreeMap<LamId, Clo>,
    /// `compose(f, g)`: a closure computing `f(g(x))`.
    pub composed: BTreeSet<(Rc<V>, Rc<V>)>,
    /// A composition of any length (at least one) of the functions this value holds: what
    /// `compose` pairs nested past `CLOSURE_DEPTH` become.
    pub chain: Option<Rc<V>>,
    /// Some function the analysis cannot name (applying it is havoc: fail closed).
    pub any: bool,
}

impl FnSet {
    pub fn is_empty(&self) -> bool {
        self.user.is_empty()
            && self.builtins.is_empty()
            && self.clos.is_empty()
            && self.composed.is_empty()
            && self.chain.is_none()
            && !self.any
    }
}

/// A list: the element of every position (`all`), and each position when the exact sequence is
/// known (a literal nothing has resized since). Invariant: when `items` is known, `all` is exactly
/// their join (build one with [`ListV::of_items`]); equality, ordering and hashing then look at
/// `items` only, so they cost the size of the value, not twice that per level of nesting.
#[derive(Clone, Debug)]
pub(crate) struct ListV {
    pub items: Option<Vec<V>>,
    pub all: V,
}

impl ListV {
    pub fn of_items(items: Vec<V>) -> ListV {
        let all = items.iter().fold(V::bottom(), |a, i| a.join(i));
        ListV {
            items: Some(items),
            all,
        }
    }
}

impl PartialEq for ListV {
    fn eq(&self, o: &Self) -> bool {
        match (&self.items, &o.items) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.all == o.all,
            _ => false,
        }
    }
}

impl Eq for ListV {}

impl Hash for ListV {
    fn hash<H: Hasher>(&self, h: &mut H) {
        match &self.items {
            Some(items) => {
                1u8.hash(h);
                items.hash(h);
            }
            None => {
                0u8.hash(h);
                self.all.hash(h);
            }
        }
    }
}

impl PartialOrd for ListV {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for ListV {
    fn cmp(&self, o: &Self) -> Ordering {
        match (&self.items, &o.items) {
            (Some(a), Some(b)) => a.cmp(b),
            (None, None) => self.all.cmp(&o.all),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
        }
    }
}

/// A map: the value at each key known as text, and at any other key (`other`). `klab` is the
/// label of the keys that were computed (not literal). A computed key may equal a known one, so a
/// value stored under a computed key is joined into every known slot as well as `other`; a read
/// of a known key is then `known[k]`, raised by `klab` (which of them it is, is decided by the
/// computed keys).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct MapV {
    pub known: BTreeMap<String, V>,
    pub other: V,
    pub klab: Lab,
    /// Known keys that may be absent (a join of maps with different keys); every other known key
    /// is present.
    pub maybe: BTreeSet<String>,
}

/// An enum value's payload: positional values and, for a struct variant, their names (in the
/// order the literal wrote them, which is the runtime's).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct EnumV {
    pub fields: Vec<V>,
    pub names: Vec<Rc<str>>,
}

/// An abstract value (see the module documentation).
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct V {
    pub lab: Lab,
    pub scalar: bool,
    pub list: Option<Rc<ListV>>,
    pub map: Option<Rc<MapV>>,
    /// Struct type → fields.
    pub structs: BTreeMap<Rc<str>, Rc<BTreeMap<String, V>>>,
    /// Struct type → its fields that may be absent (a join of values with different fields); every
    /// other field is present.
    pub maybe_fields: BTreeMap<Rc<str>, BTreeSet<String>>,
    /// (enum type, tag) → payload.
    pub enums: BTreeMap<(Rc<str>, Rc<str>), Rc<EnumV>>,
    pub fns: FnSet,
    /// May be any value of unknown shape. Its shape (lengths, key sets, tags, and the shapes nested
    /// in it) is labelled `lab`; its content (what printing it may reveal) `tlab` as well.
    pub top: bool,
    /// The content label of the `top` part (`Lab::PUB` when the value is not `top`).
    pub tlab: Lab,
    /// For a `top` value folded from known data, per field name (a struct field, a struct-variant
    /// field, a literal map key; `""` for what any name may hold, a computed map key's value):
    /// the content label of what that name holds anywhere in it, and of the scalars it holds
    /// directly. `None`: unknown, any part may hold any of `tlab`.
    pub tfields: Option<Rc<BTreeMap<String, (Lab, Lab)>>>,
    /// For a `top` value: the content of the scalar it may itself be (a string's length, a
    /// number's value), which its `lab` does not carry. What a field name holds directly as a
    /// scalar is the second label of its `tfields` entry.
    pub tscal: Lab,
    /// Only on the result of a call: the call may end the program (`exit`, `panic`) under this
    /// label, so the caller's continuation runs only when it did not. The caller clears it.
    pub exit: Lab,
}

impl V {
    pub fn bottom() -> V {
        V::default()
    }
    pub fn scalar(lab: Lab) -> V {
        V {
            lab,
            scalar: true,
            ..V::default()
        }
    }
    pub fn top(lab: Lab) -> V {
        V {
            lab,
            top: true,
            tlab: lab,
            tscal: lab,
            ..V::default()
        }
    }

    /// What deciding this value's own scalar content reveals (a string's length, a number): a
    /// scalar's `lab`, a `top`'s `tscal`.
    pub fn direct_scalar(&self) -> Lab {
        let mut l = Lab::PUB;
        if self.scalar {
            l = l.join(self.lab);
        }
        if self.top {
            l = l.join(self.tscal);
        }
        l
    }

    /// A part of this value's `top` (an element, a field, a key): of unknown shape, its shape and
    /// content labelled as the whole's, and any of the functions the value holds.
    pub fn part_of_top(&self) -> V {
        V {
            lab: self.lab,
            top: true,
            tlab: self.tlab.join(self.lab),
            tscal: self.tlab.join(self.lab),
            tfields: self.tfields.clone(),
            fns: self.fns.clone(),
            ..V::default()
        }
    }

    /// The value of field `f` of this value's `top`: what that name holds anywhere in it, when
    /// the folded value knows it.
    pub fn field_of_top(&self, f: &str) -> V {
        let mut v = self.part_of_top();
        if let Some(t) = &self.tfields {
            let at = t.get(f).copied().unwrap_or_default();
            let any = t.get("").copied().unwrap_or_default();
            v.tlab = at.0.join(any.0).join(self.lab);
            v.tscal = at.1.join(any.1).join(self.lab);
        }
        v
    }

    /// The content label of every field name in the value's data (see `tfields`).
    fn collect_fields(&self, out: &mut BTreeMap<String, (Lab, Lab)>) {
        fn add(out: &mut BTreeMap<String, (Lab, Lab)>, k: &str, v: &V, by: Lab) {
            let e = out.entry(k.to_string()).or_default();
            e.0 = e.0.join(v.deep()).join(by);
            e.1 = e.1.join(v.direct_scalar()).join(by);
        }
        if let Some(li) = &self.list {
            match &li.items {
                Some(items) => {
                    for i in items {
                        i.collect_fields(out);
                    }
                }
                None => li.all.collect_fields(out),
            }
        }
        if let Some(m) = &self.map {
            for (k, v) in &m.known {
                add(out, k, v, m.klab);
                v.collect_fields(out);
            }
            add(out, "", &m.other, m.klab);
            m.other.collect_fields(out);
        }
        for f in self.structs.values() {
            for (k, v) in f.iter() {
                add(out, k, v, Lab::PUB);
                v.collect_fields(out);
            }
        }
        for e in self.enums.values() {
            for (i, v) in e.fields.iter().enumerate() {
                if let Some(n) = e.names.get(i) {
                    add(out, n, v, Lab::PUB);
                }
                v.collect_fields(out);
            }
        }
        if self.top {
            match &self.tfields {
                Some(t) => {
                    for (k, (c, d)) in t.iter() {
                        let e = out.entry(k.clone()).or_default();
                        e.0 = e.0.join(*c);
                        e.1 = e.1.join(*d);
                    }
                }
                None => {
                    let e = out.entry(String::new()).or_default();
                    e.0 = e.0.join(self.tlab).join(self.lab);
                    e.1 = e.1.join(self.tlab).join(self.lab);
                }
            }
        }
    }

    fn fields_of(&self) -> BTreeMap<String, (Lab, Lab)> {
        let mut out = BTreeMap::new();
        self.collect_fields(&mut out);
        out
    }

    /// The value folded into `top`: every shape label, its content, every function it holds.
    pub fn fold(&self) -> V {
        let mut t = V::top(self.shape_all());
        t.tlab = self.deep();
        t.tscal = self.direct_scalar();
        t.tfields = Some(Rc::new(self.fields_of()));
        t.fns = self.all_fns();
        t
    }

    /// Record that `v` was stored somewhere in this value's `top` (under field name `f`, or at an
    /// unnamed position).
    pub fn store_in_top(&mut self, f: Option<&str>, v: &V) {
        self.tlab = self.tlab.join(v.deep());
        let fields = v.fields_of();
        if let Some(t) = &mut self.tfields {
            let t = Rc::make_mut(t);
            let mut add = |k: &str, c: Lab, d: Lab| {
                let e = t.entry(k.to_string()).or_default();
                e.0 = e.0.join(c);
                e.1 = e.1.join(d);
            };
            // An unnamed position of a folded value reads as its whole content (`part_of_top`).
            if let Some(f) = f {
                add(f, v.deep(), v.direct_scalar());
            }
            for (k, (c, d)) in fields {
                add(&k, c, d);
            }
        }
    }

    /// Whether the value can only be a scalar (its `lab` is then its content).
    fn pure_scalar(&self) -> bool {
        self.scalar
            && self.list.is_none()
            && self.map.is_none()
            && self.structs.is_empty()
            && self.enums.is_empty()
            && self.fns.is_empty()
            && !self.top
    }

    /// The join of every shape label in the value (not the content of its scalars).
    pub fn shape_all(&self) -> Lab {
        let mut l = if self.pure_scalar() {
            Lab::PUB
        } else {
            self.lab
        };
        if let Some(li) = &self.list {
            match &li.items {
                Some(items) => {
                    for i in items {
                        l = l.join(i.shape_all());
                    }
                }
                None => l = l.join(li.all.shape_all()),
            }
        }
        if let Some(m) = &self.map {
            l = l.join(m.klab).join(m.other.shape_all());
            for v in m.known.values() {
                l = l.join(v.shape_all());
            }
        }
        for f in self.structs.values() {
            for v in f.values() {
                l = l.join(v.shape_all());
            }
        }
        for e in self.enums.values() {
            for v in &e.fields {
                l = l.join(v.shape_all());
            }
        }
        l
    }
    pub fn is_bottom(&self) -> bool {
        !self.scalar
            && self.list.is_none()
            && self.map.is_none()
            && self.structs.is_empty()
            && self.enums.is_empty()
            && self.fns.is_empty()
            && !self.top
    }
    pub fn list(items: Vec<V>, lab: Lab) -> V {
        V {
            lab,
            list: Some(Rc::new(ListV::of_items(items))),
            ..V::default()
        }
    }
    /// A list whose positions are not known individually.
    pub fn list_of(all: V, lab: Lab) -> V {
        V {
            lab,
            list: Some(Rc::new(ListV { items: None, all })),
            ..V::default()
        }
    }
    pub fn func(fns: FnSet, lab: Lab) -> V {
        V {
            lab,
            fns,
            ..V::default()
        }
    }

    /// The join of every label in the value's data: what printing it, converting it to text,
    /// comparing it or hashing it may reveal. A closure displays as `<closure>`, so its captured
    /// values are not part of it (calling it is analyzed where it is called).
    pub fn deep(&self) -> Lab {
        let mut l = self.lab;
        if self.top {
            l = l.join(self.tlab);
        }
        if let Some(li) = &self.list {
            match &li.items {
                Some(items) => {
                    for i in items {
                        l = l.join(i.deep());
                    }
                }
                None => l = l.join(li.all.deep()),
            }
        }
        if let Some(m) = &self.map {
            l = l.join(m.klab).join(m.other.deep());
            for v in m.known.values() {
                l = l.join(v.deep());
            }
        }
        for f in self.structs.values() {
            for v in f.values() {
                l = l.join(v.deep());
            }
        }
        for e in self.enums.values() {
            for v in &e.fields {
                l = l.join(v.deep());
            }
        }
        for (f, g) in &self.fns.composed {
            l = l.join(f.lab).join(g.lab);
        }
        if let Some(c) = &self.fns.chain {
            l = l.join(c.lab);
        }
        l
    }

    /// Every label the value can reach: its data and, through every function it holds, what
    /// those closures captured (a summary by its reach labels).
    pub fn reach(&self) -> Lab {
        let mut l = self.deep();
        let f = self.all_fns();
        l = l.join(fns_reach(&f));
        l
    }

    /// What deciding the value's truth reveals (`as_bool`): a scalar's content, a list's, map's or
    /// string's emptiness; a struct, an enum and a closure are always true.
    pub fn truth(&self) -> Lab {
        let mut l = Lab::PUB;
        if self.scalar || self.list.is_some() {
            l = l.join(self.lab);
        }
        if let Some(m) = &self.map {
            l = l.join(self.lab).join(m.klab);
        }
        if self.top {
            l = l.join(self.lab).join(self.tscal);
        }
        l
    }

    /// What the value's runtime kind (`type`) reveals: which scalar kind it is is its content;
    /// among the other kinds, which one it is is its `lab` when it may be several.
    pub fn kind_label(&self) -> Lab {
        if self.top {
            return self.lab.join(self.tscal);
        }
        let kinds = [
            self.scalar,
            self.list.is_some(),
            self.map.is_some(),
            !self.structs.is_empty(),
            !self.enums.is_empty(),
            !self.fns.is_empty(),
        ]
        .iter()
        .filter(|k| **k)
        .count();
        if self.scalar || kinds > 1 {
            self.lab
        } else {
            Lab::PUB
        }
    }

    /// Every label in the value's data raised by `by` (a value computed or chosen under `by`).
    pub fn raise(&self, by: Lab) -> V {
        if by == Lab::PUB {
            return self.clone();
        }
        let mut v = self.clone();
        v.raise_in_place(by);
        v
    }
    fn raise_in_place(&mut self, by: Lab) {
        charge(1);
        self.lab = self.lab.join(by);
        if self.top {
            self.tlab = self.tlab.join(by);
            self.tscal = self.tscal.join(by);
            if let Some(t) = &mut self.tfields {
                for (c, d) in Rc::make_mut(t).values_mut() {
                    *c = c.join(by);
                    *d = d.join(by);
                }
            }
        }
        if let Some(li) = &mut self.list {
            let li = Rc::make_mut(li);
            match &mut li.items {
                Some(items) => {
                    for i in items.iter_mut() {
                        i.raise_in_place(by);
                    }
                    li.all = items.iter().fold(V::bottom(), |a, i| a.join(i));
                }
                None => li.all.raise_in_place(by),
            }
        }
        if let Some(m) = &mut self.map {
            let m = Rc::make_mut(m);
            m.klab = m.klab.join(by);
            m.other.raise_in_place(by);
            for v in m.known.values_mut() {
                v.raise_in_place(by);
            }
        }
        for f in self.structs.values_mut() {
            for v in Rc::make_mut(f).values_mut() {
                v.raise_in_place(by);
            }
        }
        for e in self.enums.values_mut() {
            for v in &mut Rc::make_mut(e).fields {
                v.raise_in_place(by);
            }
        }
    }

    /// Every label cleared of `bits` (a well-formed `declassify`).
    pub fn release(&self, bits: Lab) -> V {
        let mut v = self.clone();
        v.release_in_place(bits);
        v
    }
    fn release_in_place(&mut self, bits: Lab) {
        charge(1);
        self.lab = self.lab.without(bits);
        self.tlab = self.tlab.without(bits);
        self.tscal = self.tscal.without(bits);
        if let Some(t) = &mut self.tfields {
            for (c, d) in Rc::make_mut(t).values_mut() {
                *c = c.without(bits);
                *d = d.without(bits);
            }
        }
        if let Some(li) = &mut self.list {
            let li = Rc::make_mut(li);
            match &mut li.items {
                Some(items) => {
                    for i in items.iter_mut() {
                        i.release_in_place(bits);
                    }
                    li.all = items.iter().fold(V::bottom(), |a, i| a.join(i));
                }
                None => li.all.release_in_place(bits),
            }
        }
        if let Some(m) = &mut self.map {
            let m = Rc::make_mut(m);
            m.klab = m.klab.without(bits);
            m.other.release_in_place(bits);
            for v in m.known.values_mut() {
                v.release_in_place(bits);
            }
        }
        for f in self.structs.values_mut() {
            for v in Rc::make_mut(f).values_mut() {
                v.release_in_place(bits);
            }
        }
        for e in self.enums.values_mut() {
            for v in &mut Rc::make_mut(e).fields {
                v.release_in_place(bits);
            }
        }
    }

    /// Add every function `o` holds (at any depth) to this value's own (a closure stored into a
    /// folded value).
    pub fn absorb_fns(&mut self, o: &V) {
        let f = o.all_fns();
        self.fns = join_fns(&self.fns, &f);
    }

    /// The least value above both.
    pub fn join(&self, o: &V) -> V {
        if o.is_bottom() {
            let mut s = self.clone();
            s.exit = s.exit.join(o.exit);
            return s;
        }
        if self.is_bottom() {
            let mut s = o.clone();
            s.exit = s.exit.join(self.exit);
            return s;
        }
        if self == o {
            return self.clone();
        }
        charge(1);
        let list = match (&self.list, &o.list) {
            (None, x) | (x, None) => x.clone(),
            (Some(a), Some(b)) if Rc::ptr_eq(a, b) => Some(a.clone()),
            (Some(a), Some(b)) => Some(Rc::new(match (&a.items, &b.items) {
                (Some(x), Some(y)) if x.len() == y.len() => {
                    ListV::of_items(x.iter().zip(y).map(|(p, q)| p.join(q)).collect())
                }
                _ => ListV {
                    items: None,
                    all: a.all.join(&b.all),
                },
            })),
        };
        let map = match (&self.map, &o.map) {
            (None, x) | (x, None) => x.clone(),
            (Some(a), Some(b)) if Rc::ptr_eq(a, b) => Some(a.clone()),
            (Some(a), Some(b)) => {
                // A key known on one side only: the other side's value there, if it has that key,
                // is one of its computed keys' (`other`).
                let mut known = BTreeMap::new();
                for (k, v) in &a.known {
                    let j = match b.known.get(k) {
                        Some(w) => v.join(w),
                        None => v.join(&b.other),
                    };
                    known.insert(k.clone(), j);
                }
                for (k, w) in &b.known {
                    if !a.known.contains_key(k) {
                        known.insert(k.clone(), w.join(&a.other));
                    }
                }
                // A key known on one side only may be absent from the join.
                let mut maybe: BTreeSet<String> = a.maybe.union(&b.maybe).cloned().collect();
                for k in a.known.keys().filter(|k| !b.known.contains_key(*k)) {
                    maybe.insert(k.clone());
                }
                for k in b.known.keys().filter(|k| !a.known.contains_key(*k)) {
                    maybe.insert(k.clone());
                }
                Some(Rc::new(MapV {
                    known,
                    other: a.other.join(&b.other),
                    klab: a.klab.join(b.klab),
                    maybe,
                }))
            }
        };
        let mut structs = self.structs.clone();
        let mut maybe_fields = self.maybe_fields.clone();
        for (t, f) in &o.structs {
            let j = match structs.get(t) {
                Some(x) if Rc::ptr_eq(x, f) => x.clone(),
                Some(x) => {
                    // A field of one side only may be absent from the join.
                    let m = maybe_fields.entry(t.clone()).or_default();
                    for k in x.keys().filter(|k| !f.contains_key(*k)) {
                        m.insert(k.clone());
                    }
                    for k in f.keys().filter(|k| !x.contains_key(*k)) {
                        m.insert(k.clone());
                    }
                    Rc::new(join_fields(x, f))
                }
                None => f.clone(),
            };
            structs.insert(t.clone(), j);
        }
        for (t, m) in &o.maybe_fields {
            maybe_fields
                .entry(t.clone())
                .or_default()
                .extend(m.iter().cloned());
        }
        maybe_fields.retain(|_, m| !m.is_empty());
        let mut enums = self.enums.clone();
        for (t, e) in &o.enums {
            let j = match enums.get(t) {
                Some(x) if Rc::ptr_eq(x, e) => x.clone(),
                Some(x) => Rc::new(join_enum(x, e)),
                None => e.clone(),
            };
            enums.insert(t.clone(), j);
        }
        V {
            lab: self.lab.join(o.lab),
            scalar: self.scalar || o.scalar,
            list,
            map,
            structs,
            maybe_fields,
            enums,
            fns: join_fns(&self.fns, &o.fns),
            top: self.top || o.top,
            tlab: self.tlab.join(o.tlab),
            tfields: join_tfields(self, o),
            tscal: self.tscal.join(o.tscal),
            exit: self.exit.join(o.exit),
        }
    }

    /// The value chosen between `a` and `b` by a condition labelled `by`: every node raised by
    /// `by`, except the shape of a node that is the same in both (one kind, one struct type with the
    /// same fields, one enum variant, lists of one known length, maps of the same literal keys,
    /// the same function), whose children are then chosen the same way.
    pub fn select(a: &V, b: &V, by: Lab) -> V {
        if by == Lab::PUB {
            return a.join(b);
        }
        charge(1);
        if a.is_bottom() || b.is_bottom() || a.top || b.top || a.scalar || b.scalar {
            return a.join(b).raise(by);
        }
        let kinds = |v: &V| {
            (
                v.list.is_some(),
                v.map.is_some(),
                !v.structs.is_empty(),
                !v.enums.is_empty(),
                !v.fns.is_empty(),
            )
        };
        let ka = kinds(a);
        if ka != kinds(b)
            || [ka.0, ka.1, ka.2, ka.3, ka.4]
                .iter()
                .filter(|k| **k)
                .count()
                != 1
        {
            return a.join(b).raise(by);
        }
        let lab = a.lab.join(b.lab);
        let exit = a.exit.join(b.exit);
        if let (Some(la), Some(lb)) = (&a.list, &b.list) {
            if let (Some(x), Some(y)) = (&la.items, &lb.items) {
                if x.len() == y.len() {
                    let items = x.iter().zip(y).map(|(p, q)| V::select(p, q, by)).collect();
                    return V {
                        lab,
                        list: Some(Rc::new(ListV::of_items(items))),
                        exit,
                        ..V::default()
                    };
                }
            }
            return a.join(b).raise(by);
        }
        if let (Some(ma), Some(mb)) = (&a.map, &b.map) {
            let same_keys = ma.known.keys().eq(mb.known.keys())
                && ma.maybe == mb.maybe
                && ma.other.is_bottom()
                && mb.other.is_bottom()
                && ma.klab == Lab::PUB
                && mb.klab == Lab::PUB;
            if !same_keys {
                return a.join(b).raise(by);
            }
            let known = ma
                .known
                .iter()
                .zip(mb.known.values())
                .map(|((k, x), y)| (k.clone(), V::select(x, y, by)))
                .collect();
            return V {
                lab,
                map: Some(Rc::new(MapV {
                    known,
                    other: V::bottom(),
                    klab: Lab::PUB,
                    maybe: ma.maybe.clone(),
                })),
                exit,
                ..V::default()
            };
        }
        if !a.structs.is_empty() {
            if a.structs.len() == 1 && b.structs.len() == 1 {
                let (ta, fa) = a.structs.iter().next().expect("one struct type");
                let (tb, fb) = b.structs.iter().next().expect("one struct type");
                if ta == tb && fa.keys().eq(fb.keys()) && a.maybe_fields == b.maybe_fields {
                    let fields = fa
                        .iter()
                        .zip(fb.values())
                        .map(|((k, x), y)| (k.clone(), V::select(x, y, by)))
                        .collect();
                    let mut v = V {
                        lab,
                        exit,
                        ..V::default()
                    };
                    v.structs.insert(ta.clone(), Rc::new(fields));
                    v.maybe_fields = a.maybe_fields.clone();
                    return v;
                }
            }
            return a.join(b).raise(by);
        }
        if !a.enums.is_empty() {
            if a.enums.len() == 1 && b.enums.len() == 1 {
                let (ka, ea) = a.enums.iter().next().expect("one variant");
                let (kb, eb) = b.enums.iter().next().expect("one variant");
                if ka == kb && ea.names == eb.names && ea.fields.len() == eb.fields.len() {
                    let fields = ea
                        .fields
                        .iter()
                        .zip(&eb.fields)
                        .map(|(x, y)| V::select(x, y, by))
                        .collect();
                    let mut v = V {
                        lab,
                        exit,
                        ..V::default()
                    };
                    v.enums.insert(
                        ka.clone(),
                        Rc::new(EnumV {
                            fields,
                            names: ea.names.clone(),
                        }),
                    );
                    return v;
                }
            }
            return a.join(b).raise(by);
        }
        // Function values: the same single named function, or the same single lambda (its
        // captured values chosen name by name).
        let (fa, fb) = (&a.fns, &b.fns);
        let plain = |f: &FnSet| f.composed.is_empty() && f.chain.is_none() && !f.any;
        if plain(fa) && plain(fb) {
            let single_named = fa.clos.is_empty()
                && fb.clos.is_empty()
                && fa.user.len() + fa.builtins.len() == 1
                && fa.user == fb.user
                && fa.builtins == fb.builtins;
            if single_named {
                return V {
                    lab,
                    fns: fa.clone(),
                    exit,
                    ..V::default()
                };
            }
            if fa.user.is_empty()
                && fb.user.is_empty()
                && fa.builtins.is_empty()
                && fb.builtins.is_empty()
                && fa.clos.len() == 1
                && fb.clos.len() == 1
            {
                let (ia, ca) = fa.clos.iter().next().expect("one lambda");
                let (ib, cb) = fb.clos.iter().next().expect("one lambda");
                if let (true, Clo::Env(ea), Clo::Env(eb)) = (ia == ib, ca, cb) {
                    if ea.keys().eq(eb.keys()) {
                        let env: Env = ea
                            .iter()
                            .zip(eb.values())
                            .map(|((k, x), y)| (k.clone(), V::select(x, y, by)))
                            .collect();
                        let mut fns = FnSet::default();
                        fns.clos.insert(*ia, Clo::Env(Rc::new(env)));
                        return V {
                            lab,
                            fns,
                            exit,
                            ..V::default()
                        };
                    }
                }
            }
        }
        a.join(b).raise(by)
    }

    /// The value with data nested deeper than `DATA_DEPTH` folded into `top`, and closures nested
    /// in captures deeper than `CLOSURE_DEPTH` summarized (`summarize` receives each such
    /// closure's reach label and captured values). Applied where values accumulate (loop heads,
    /// call contexts). A value that is still wider than `MAX_NODES` first forgets its lists'
    /// positions, then is folded at halving depths. `widen` starting from a smaller depth limit (a
    /// summary's arguments, which must settle fast).
    pub fn widen_from(&self, depth: usize, summarize: &mut dyn FnMut(LamId, Lab, &Env)) -> V {
        let w = self.widen_at_limit(0, 0, depth, summarize);
        if w.size() <= MAX_NODES {
            return w;
        }
        let w = w.collapse_items();
        if w.size() <= MAX_NODES {
            return w;
        }
        let mut limit = depth / 2;
        loop {
            let x = w.widen_at_limit(0, 0, limit.max(1), summarize);
            if limit <= 1 || x.size() <= MAX_NODES {
                return x;
            }
            limit /= 2;
        }
    }

    /// The value with every list's positions forgotten (each list keeps the join of its
    /// elements) and every map's literal keys merged into its other keys.
    fn collapse_items(&self) -> V {
        charge(1);
        let mut v = self.clone();
        if let Some(li) = &self.list {
            v.list = Some(Rc::new(ListV {
                items: None,
                all: li.all.collapse_items(),
            }));
        }
        if let Some(m) = &self.map {
            let mut other = m.other.collapse_items();
            for x in m.known.values() {
                other = other.join(&x.collapse_items());
            }
            v.map = Some(Rc::new(MapV {
                known: BTreeMap::new(),
                other,
                klab: m.klab,
                maybe: BTreeSet::new(),
            }));
        }
        v.structs = self
            .structs
            .iter()
            .map(|(t, f)| {
                (
                    t.clone(),
                    Rc::new(
                        f.iter()
                            .map(|(k, x)| (k.clone(), x.collapse_items()))
                            .collect(),
                    ),
                )
            })
            .collect();
        v.enums = self
            .enums
            .iter()
            .map(|(t, e)| {
                (
                    t.clone(),
                    Rc::new(EnumV {
                        fields: e.fields.iter().map(V::collapse_items).collect(),
                        names: e.names.clone(),
                    }),
                )
            })
            .collect();
        v
    }

    /// Every function the value holds, at any depth of its data.
    pub fn all_fns(&self) -> FnSet {
        let mut out = self.fns.clone();
        let mut add = |v: &V| {
            if !v.is_bottom() {
                let f = v.all_fns();
                out = join_fns(&out, &f);
            }
        };
        if let Some(li) = &self.list {
            match &li.items {
                Some(items) => {
                    for i in items {
                        add(i);
                    }
                }
                None => add(&li.all),
            }
        }
        if let Some(m) = &self.map {
            add(&m.other);
            for v in m.known.values() {
                add(v);
            }
        }
        for f in self.structs.values() {
            for v in f.values() {
                add(v);
            }
        }
        for e in self.enums.values() {
            for v in &e.fields {
                add(v);
            }
        }
        out
    }

    /// Whether `self` ⊑ `o`: everything `self` stands for, `o` stands for too. Each kind of
    /// `self` must be below the same kind of `o`, or covered by `o`'s `top` part.
    pub fn leq(&self, o: &V) -> bool {
        charge(1);
        if !lab_leq(self.exit, o.exit) {
            return false;
        }
        if self.is_bottom() {
            return true;
        }
        if o.top && self.covered_by_top(o) {
            return true;
        }
        if self.top
            && !(o.top
                && lab_leq(self.lab, o.lab)
                && lab_leq(self.tlab, o.tlab.join(o.lab))
                && lab_leq(self.tscal, o.tscal.join(o.lab))
                && tfields_cover(&self.top_fields(), o))
        {
            return false;
        }
        if !lab_leq(self.lab, o.lab) {
            return false;
        }
        let top_covers = |part: V| o.top && part.covered_by_top(o);
        if self.scalar && !o.scalar && !top_covers(V::scalar(self.lab)) {
            return false;
        }
        if let Some(a) = &self.list {
            let ok = match &o.list {
                Some(b) => Rc::ptr_eq(a, b) || list_leq(a, b),
                None => false,
            };
            if !ok
                && !top_covers(V {
                    lab: self.lab,
                    list: Some(a.clone()),
                    ..V::default()
                })
            {
                return false;
            }
        }
        if let Some(a) = &self.map {
            let ok = match &o.map {
                Some(b) => Rc::ptr_eq(a, b) || map_leq(a, b),
                None => false,
            };
            if !ok
                && !top_covers(V {
                    lab: self.lab,
                    map: Some(a.clone()),
                    ..V::default()
                })
            {
                return false;
            }
        }
        for (t, fa) in &self.structs {
            let ok = match o.structs.get(t) {
                Some(fb) => {
                    let (ma, mb) = (self.maybe_fields.get(t), o.maybe_fields.get(t));
                    let in_mb = |k: &String| mb.is_some_and(|m| m.contains(k));
                    !ma.is_some_and(|m| m.iter().any(|k| !in_mb(k)))
                        && !fb.keys().any(|k| !fa.contains_key(k) && !in_mb(k))
                        && (Rc::ptr_eq(fa, fb)
                            || fa.iter().all(|(k, v)| fb.get(k).is_some_and(|w| v.leq(w))))
                }
                None => false,
            };
            if !ok {
                let mut part = V {
                    lab: self.lab,
                    ..V::default()
                };
                part.structs.insert(t.clone(), fa.clone());
                if !top_covers(part) {
                    return false;
                }
            }
        }
        for (t, ea) in &self.enums {
            let ok = match o.enums.get(t) {
                Some(eb) => Rc::ptr_eq(ea, eb) || enum_leq(ea, eb),
                None => false,
            };
            if !ok {
                let mut part = V {
                    lab: self.lab,
                    ..V::default()
                };
                part.enums.insert(t.clone(), ea.clone());
                if !top_covers(part) {
                    return false;
                }
            }
        }
        fns_leq(&self.fns, &o.fns)
    }

    /// Whether a `top` value `o` covers all of `self`: its shapes, its content, its functions.
    fn covered_by_top(&self, o: &V) -> bool {
        lab_leq(self.shape_all(), o.lab)
            && lab_leq(self.deep(), o.lab.join(o.tlab))
            && fns_leq(&self.all_fns(), &o.fns)
            && lab_leq(self.direct_scalar(), o.tscal.join(o.lab))
            && tfields_cover(&self.fields_of(), o)
    }

    /// The field labels of this value's own `top` part.
    fn top_fields(&self) -> BTreeMap<String, (Lab, Lab)> {
        match &self.tfields {
            Some(t) => (**t).clone(),
            None => {
                let l = self.tlab.join(self.lab);
                [(String::new(), (l, l))].into_iter().collect()
            }
        }
    }

    /// The number of nodes in the value's data and captures.
    pub fn size(&self) -> usize {
        let mut n = 1;
        if let Some(li) = &self.list {
            n += match &li.items {
                Some(items) => items.iter().map(V::size).sum::<usize>(),
                None => li.all.size(),
            };
        }
        if let Some(m) = &self.map {
            n += m.other.size() + m.known.values().map(V::size).sum::<usize>();
        }
        for f in self.structs.values() {
            n += f.values().map(V::size).sum::<usize>();
        }
        for e in self.enums.values() {
            n += e.fields.iter().map(V::size).sum::<usize>();
        }
        for c in self.fns.clos.values() {
            if let Clo::Env(env) = c {
                n += env.values().map(V::size).sum::<usize>();
            }
        }
        for (f, g) in &self.fns.composed {
            n += f.size() + g.size();
        }
        if let Some(c) = &self.fns.chain {
            n += c.size();
        }
        n
    }

    fn widen_at(
        &self,
        depth: usize,
        cdepth: usize,
        summarize: &mut dyn FnMut(LamId, Lab, &Env),
    ) -> V {
        self.widen_at_limit(depth, cdepth, DATA_DEPTH, summarize)
    }

    fn widen_at_limit(
        &self,
        depth: usize,
        cdepth: usize,
        limit: usize,
        summarize: &mut dyn FnMut(LamId, Lab, &Env),
    ) -> V {
        charge(1);
        // Leaves (a scalar, an already-folded value) have nothing to fold.
        let leaf = self.list.is_none()
            && self.map.is_none()
            && self.structs.is_empty()
            && self.enums.is_empty()
            && self.fns.is_empty();
        if depth >= limit && !leaf {
            // Folded: the data's labels join into `top`; every function it holds anywhere stays
            // (applying the folded value applies them).
            let mut t = self.fold();
            t.exit = self.exit;
            t.fns = V::func(t.fns, Lab::PUB).fns_widened(cdepth, summarize);
            return t;
        }
        let mut v = self.clone();
        if let Some(li) = &self.list {
            v.list = Some(Rc::new(match &li.items {
                Some(items) => ListV::of_items(
                    items
                        .iter()
                        .map(|i| i.widen_at_limit(depth + 1, cdepth, limit, summarize))
                        .collect(),
                ),
                None => ListV {
                    items: None,
                    all: li.all.widen_at_limit(depth + 1, cdepth, limit, summarize),
                },
            }));
        }
        if let Some(m) = &self.map {
            v.map = Some(Rc::new(MapV {
                known: m
                    .known
                    .iter()
                    .map(|(k, x)| {
                        (
                            k.clone(),
                            x.widen_at_limit(depth + 1, cdepth, limit, summarize),
                        )
                    })
                    .collect(),
                other: m.other.widen_at_limit(depth + 1, cdepth, limit, summarize),
                klab: m.klab,
                maybe: m.maybe.clone(),
            }));
        }
        v.structs = self
            .structs
            .iter()
            .map(|(t, f)| {
                (
                    t.clone(),
                    Rc::new(
                        f.iter()
                            .map(|(k, x)| {
                                (
                                    k.clone(),
                                    x.widen_at_limit(depth + 1, cdepth, limit, summarize),
                                )
                            })
                            .collect(),
                    ),
                )
            })
            .collect();
        v.enums = self
            .enums
            .iter()
            .map(|(t, e)| {
                (
                    t.clone(),
                    Rc::new(EnumV {
                        fields: e
                            .fields
                            .iter()
                            .map(|x| x.widen_at_limit(depth + 1, cdepth, limit, summarize))
                            .collect(),
                        names: e.names.clone(),
                    }),
                )
            })
            .collect();
        v.fns = self.fns_widened(cdepth, summarize);
        v
    }
    fn fns_widened(&self, cdepth: usize, summarize: &mut dyn FnMut(LamId, Lab, &Env)) -> FnSet {
        let mut fns = self.fns.clone();
        fns.clos = BTreeMap::new();
        for (id, c) in &self.fns.clos {
            let c2 = match c {
                Clo::Summary(s) => Clo::Summary(s.clone()),
                Clo::Env(env) if cdepth >= CLOSURE_DEPTH => {
                    let sig = env_reach(env);
                    summarize(*id, sig, env);
                    Clo::Summary([sig].into_iter().collect())
                }
                Clo::Env(env) => Clo::Env(Rc::new(
                    env.iter()
                        .map(|(k, x)| (k.clone(), x.widen_at(0, cdepth + 1, summarize)))
                        .collect(),
                )),
            };
            fns.clos.insert(*id, c2);
        }
        // A composition of compositions (or nested this deeply) becomes a chain: any composition
        // of their functions. What stays a pair composes two plain function values, so a
        // composition accumulated in a loop settles.
        let nested = |v: &V| {
            let f = v.all_fns();
            !f.composed.is_empty() || f.chain.is_some()
        };
        let mut chain = self.fns.chain.as_deref().cloned();
        let mut pairs = BTreeSet::new();
        for (g, h) in &self.fns.composed {
            if cdepth >= CLOSURE_DEPTH || nested(g) || nested(h) {
                let c = chain_value(g, h);
                chain = Some(match chain {
                    Some(x) => x.join(&c),
                    None => c,
                });
            } else {
                pairs.insert((
                    Rc::new(g.widen_at(0, cdepth + 1, summarize)),
                    Rc::new(h.widen_at(0, cdepth + 1, summarize)),
                ));
            }
        }
        fns.composed = pairs;
        fns.chain = chain.map(|c| Rc::new(c.widen_at(0, cdepth + 1, summarize)));
        fns
    }
}

fn lab_leq(a: Lab, b: Lab) -> bool {
    a.0 & !b.0 == 0
}

/// The `tfields` of a join: known only when every `top` side knows it.
fn join_tfields(a: &V, b: &V) -> Option<Rc<BTreeMap<String, (Lab, Lab)>>> {
    match (a.top, b.top) {
        (false, false) => None,
        (true, false) => a.tfields.clone(),
        (false, true) => b.tfields.clone(),
        (true, true) => match (&a.tfields, &b.tfields) {
            (Some(x), Some(y)) if Rc::ptr_eq(x, y) => Some(x.clone()),
            (Some(x), Some(y)) => {
                let mut out = (**x).clone();
                for (k, (c, d)) in y.iter() {
                    let e = out.entry(k.clone()).or_default();
                    e.0 = e.0.join(*c);
                    e.1 = e.1.join(*d);
                }
                Some(Rc::new(out))
            }
            _ => None,
        },
    }
}

/// Whether a `top` value `o` bounds what these field names hold.
fn tfields_cover(fields: &BTreeMap<String, (Lab, Lab)>, o: &V) -> bool {
    let Some(t) = &o.tfields else { return true };
    let any = t.get("").copied().unwrap_or_default();
    let any = (any.0.join(o.lab), any.1.join(o.lab));
    fields.iter().all(|(k, (c, d))| {
        let at = if k.is_empty() {
            any
        } else {
            let x = t.get(k).copied().unwrap_or_default();
            (x.0.join(any.0), x.1.join(any.1))
        };
        lab_leq(*c, at.0) && lab_leq(*d, at.1)
    })
}

fn list_leq(a: &ListV, b: &ListV) -> bool {
    match (&a.items, &b.items) {
        (Some(ai), Some(bi)) => ai.len() == bi.len() && ai.iter().zip(bi).all(|(x, y)| x.leq(y)),
        (Some(ai), None) => ai.iter().all(|x| x.leq(&b.all)),
        (None, Some(_)) => false,
        (None, None) => a.all.leq(&b.all),
    }
}

fn map_leq(a: &MapV, b: &MapV) -> bool {
    if !lab_leq(a.klab, b.klab) || !a.other.leq(&b.other) {
        return false;
    }
    for (k, v) in &a.known {
        let target = b.known.get(k).unwrap_or(&b.other);
        if !v.leq(target) {
            return false;
        }
    }
    // Where `b` knows a key `a` does not, `a`'s value there is its `other`, and `a` may lack the
    // key.
    for (k, w) in &b.known {
        if !a.known.contains_key(k) && (!a.other.leq(w) || !b.maybe.contains(k)) {
            return false;
        }
    }
    !a.maybe
        .iter()
        .any(|k| b.known.contains_key(k) && !b.maybe.contains(k))
}

pub(crate) fn env_leq(a: &Env, b: &Env) -> bool {
    a.iter().all(|(k, v)| b.get(k).is_some_and(|w| v.leq(w)))
}

/// The reach label of a closure's captured values (which summary it belongs to).
pub(crate) fn env_reach(e: &Env) -> Lab {
    e.values().fold(Lab::PUB, |l, v| l.join(v.reach()))
}

fn fns_reach(f: &FnSet) -> Lab {
    let mut l = Lab::PUB;
    for c in f.clos.values() {
        match c {
            Clo::Env(e) => l = l.join(env_reach(e)),
            Clo::Summary(s) => {
                for x in s {
                    l = l.join(*x);
                }
            }
        }
    }
    for (g, h) in &f.composed {
        l = l.join(g.reach()).join(h.reach());
    }
    if let Some(c) = &f.chain {
        l = l.join(c.reach());
    }
    l
}

/// A composition `g ∘ h` as a chain: a function value holding every function of both (their own
/// compositions and chains flattened into it), labelled with both labels.
fn chain_value(g: &V, h: &V) -> V {
    let mut out = FnSet::default();
    let mut lab = g.lab.join(h.lab);
    let mut stack: Vec<V> = vec![g.clone(), h.clone()];
    while let Some(v) = stack.pop() {
        lab = lab.join(v.lab);
        let f = v.all_fns();
        out.user.extend(f.user.iter().cloned());
        out.builtins.extend(f.builtins.iter().cloned());
        out.any |= f.any;
        let only_clos = FnSet {
            clos: f.clos.clone(),
            ..FnSet::default()
        };
        out = join_fns(&out, &only_clos);
        for (a, b) in &f.composed {
            stack.push((**a).clone());
            stack.push((**b).clone());
        }
        if let Some(c) = &f.chain {
            stack.push((**c).clone());
        }
    }
    V::func(out, lab)
}

fn fns_leq(a: &FnSet, b: &FnSet) -> bool {
    if a.any && !b.any {
        return false;
    }
    if !a.user.is_subset(&b.user) || !a.builtins.is_subset(&b.builtins) {
        return false;
    }
    match (&a.chain, &b.chain) {
        (Some(x), Some(y)) => {
            if !x.leq(y) {
                return false;
            }
        }
        (Some(_), None) => return false,
        _ => {}
    }
    let composed_ok = a.composed.iter().all(|p| {
        b.composed.contains(p)
            || b.chain
                .as_ref()
                .is_some_and(|c| chain_value(&p.0, &p.1).leq(c))
    });
    if !composed_ok {
        return false;
    }
    a.clos.iter().all(|(id, c)| match (c, b.clos.get(id)) {
        (_, None) => false,
        (Clo::Env(x), Some(Clo::Env(y))) => env_leq(x, y),
        (Clo::Env(x), Some(Clo::Summary(s))) => {
            let sig = env_reach(x);
            if s.contains(&sig) {
                pend(*id, sig, x);
                true
            } else {
                false
            }
        }
        (Clo::Summary(s), Some(Clo::Summary(t))) => s.is_subset(t),
        (Clo::Summary(_), Some(Clo::Env(_))) => false,
    })
}

fn join_fields(a: &BTreeMap<String, V>, b: &BTreeMap<String, V>) -> BTreeMap<String, V> {
    let mut out = a.clone();
    for (k, v) in b {
        let j = match out.get(k) {
            Some(x) => x.join(v),
            None => v.clone(),
        };
        out.insert(k.clone(), j);
    }
    out
}

/// Two payloads of one variant. Positional when their field names agree; when a struct variant's
/// literals wrote the fields in different orders, a position and a name no longer pick the same
/// field, so every position holds any of them.
fn join_enum(a: &EnumV, b: &EnumV) -> EnumV {
    let n = a.fields.len().max(b.fields.len());
    if a.names == b.names {
        let mut fields = Vec::with_capacity(n);
        for i in 0..n {
            let x = match (a.fields.get(i), b.fields.get(i)) {
                (Some(p), Some(q)) => p.join(q),
                (Some(p), None) | (None, Some(p)) => p.clone(),
                (None, None) => V::bottom(),
            };
            fields.push(x);
        }
        return EnumV {
            fields,
            names: a.names.clone(),
        };
    }
    let any = a
        .fields
        .iter()
        .chain(&b.fields)
        .fold(V::bottom(), |acc, x| acc.join(x));
    let names = if a.names.len() >= b.names.len() {
        a.names.clone()
    } else {
        b.names.clone()
    };
    EnumV {
        fields: vec![any; n],
        names,
    }
}

fn enum_leq(ea: &EnumV, eb: &EnumV) -> bool {
    if ea.fields.len() > eb.fields.len() || !ea.fields.iter().zip(&eb.fields).all(|(x, y)| x.leq(y))
    {
        return false;
    }
    if ea.names != eb.names {
        // A field read by name must find it at least as large where `eb` keeps that name.
        for (i, n) in ea.names.iter().enumerate() {
            let Some(j) = eb.names.iter().position(|m| m == n) else {
                return false;
            };
            match (ea.fields.get(i), eb.fields.get(j)) {
                (Some(x), Some(y)) if x.leq(y) => {}
                (None, _) => {}
                _ => return false,
            }
        }
    }
    true
}

pub(crate) fn join_env(a: &Env, b: &Env) -> Env {
    let mut out = a.clone();
    for (k, v) in b {
        let j = match out.get(k) {
            Some(x) => x.join(v),
            None => v.clone(),
        };
        out.insert(k.clone(), j);
    }
    out
}

fn join_fns(a: &FnSet, b: &FnSet) -> FnSet {
    let mut out = a.clone();
    out.user.extend(b.user.iter().cloned());
    out.builtins.extend(b.builtins.iter().cloned());
    out.composed.extend(b.composed.iter().cloned());
    out.any |= b.any;
    out.chain = match (&a.chain, &b.chain) {
        (None, x) | (x, None) => x.clone(),
        (Some(x), Some(y)) => Some(Rc::new(x.join(y))),
    };
    for (id, c) in &b.clos {
        let j = match (out.clos.get(id), c) {
            (None, c) => c.clone(),
            (Some(Clo::Summary(s)), Clo::Summary(t)) => Clo::Summary(s.union(t).cloned().collect()),
            (Some(Clo::Summary(s)), Clo::Env(e)) | (Some(Clo::Env(e)), Clo::Summary(s)) => {
                // The snapshot joins the summary: the interpreter must add it there.
                let sig = env_reach(e);
                pend(*id, sig, e);
                let mut s = s.clone();
                s.insert(sig);
                Clo::Summary(s)
            }
            (Some(Clo::Env(x)), Clo::Env(y)) => {
                if x == y {
                    Clo::Env(x.clone())
                } else {
                    Clo::Env(Rc::new(join_env(x, y)))
                }
            }
        };
        out.clos.insert(*id, j);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_is_a_least_upper_bound_on_labels_and_kinds() {
        let a = V::scalar(Lab::SEC);
        let b = V::list(vec![V::scalar(Lab::PUB)], Lab::PUB);
        let j = a.join(&b);
        assert!(j.scalar && j.list.is_some());
        assert!(j.lab.secret());
        assert_eq!(j.join(&a), j);
        assert_eq!(j.join(&b), j);
        assert_eq!(a.join(&V::bottom()), a);
        assert_eq!(V::bottom().join(&a), a);
    }

    #[test]
    fn deep_sees_nested_data_but_not_closure_captures() {
        let secret_elem = V::list(vec![V::scalar(Lab::PUB), V::scalar(Lab::SEC)], Lab::PUB);
        assert!(secret_elem.deep().secret());
        assert!(!secret_elem.lab.secret());
        let mut env = Env::new();
        env.insert("k".into(), V::scalar(Lab::SEC));
        let mut fns = FnSet::default();
        fns.clos.insert(0, Clo::Env(Rc::new(env)));
        let f = V::func(fns, Lab::PUB);
        assert!(!f.deep().secret());
        assert!(f.reach().secret());
    }

    #[test]
    fn raise_and_release_reach_every_label() {
        let v = V::list(vec![V::scalar(Lab::PUB)], Lab::PUB).raise(Lab::SEC);
        assert!(v.lab.secret() && v.list.as_ref().unwrap().all.lab.secret());
        let r = v.release(Lab::SEC);
        assert!(!r.deep().secret());
    }

    #[test]
    fn widening_bounds_closure_nesting() {
        // c0 = zero; c_{n+1} = closure capturing c_n
        let mut c = V::func(
            FnSet {
                user: [Rc::from("zero")].into_iter().collect(),
                ..FnSet::default()
            },
            Lab::PUB,
        );
        for _ in 0..20 {
            let mut env = Env::new();
            env.insert("c".into(), c.clone());
            let mut fns = FnSet::default();
            fns.clos.insert(7, Clo::Env(Rc::new(env)));
            c = V::func(fns, Lab::PUB);
        }
        let mut summarized = 0;
        let w = c.widen_from(DATA_DEPTH, &mut |_, _, _| summarized += 1);
        assert!(summarized > 0);
        // Widening twice is stable.
        let w2 = w.widen_from(DATA_DEPTH, &mut |_, _, _| {});
        assert_eq!(w, w2);
    }

    #[test]
    fn a_computed_key_known_on_one_side_joins_into_the_other_sides_known_key() {
        let mut a = MapV {
            known: BTreeMap::new(),
            other: V::bottom(),
            klab: Lab::PUB,
            maybe: BTreeSet::new(),
        };
        a.known.insert("a".into(), V::scalar(Lab::PUB));
        let b = MapV {
            known: BTreeMap::new(),
            other: V::scalar(Lab::SEC),
            klab: Lab::PUB,
            maybe: BTreeSet::new(),
        };
        let va = V {
            map: Some(Rc::new(a)),
            ..V::default()
        };
        let vb = V {
            map: Some(Rc::new(b)),
            ..V::default()
        };
        let j = va.join(&vb);
        assert!(j.map.as_ref().unwrap().known["a"].lab.secret());
        assert!(!vb.leq(&va));
        assert!(vb.leq(&j) && va.leq(&j));
    }

    #[test]
    fn select_keeps_a_shape_both_alternatives_share() {
        let a = V::list(vec![V::scalar(Lab::PUB), V::scalar(Lab::PUB)], Lab::PUB);
        let b = V::list(vec![V::scalar(Lab::PUB), V::scalar(Lab::PUB)], Lab::PUB);
        let s = V::select(&a, &b, Lab::SEC);
        assert!(!s.lab.secret(), "same length: the length is not chosen");
        assert!(s.deep().secret(), "the elements are");
        let c = V::list(vec![V::scalar(Lab::PUB)], Lab::PUB);
        assert!(V::select(&a, &c, Lab::SEC).lab.secret());
    }

    #[test]
    fn nested_single_item_lists_compare_in_linear_time() {
        let mut v = V::scalar(Lab::PUB);
        for _ in 0..64 {
            v = V::list(vec![v], Lab::PUB);
        }
        let w = v.raise(Lab::SEC);
        assert_ne!(v, w);
        assert!(v.leq(&w));
        let mut h = std::collections::hash_map::DefaultHasher::new();
        w.hash(&mut h);
    }
}
