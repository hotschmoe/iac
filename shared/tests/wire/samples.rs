// Sample messages made from the model and played through serde.
//
// The model is read from source text, so it could misread an attribute. It
// is held to the real derives here: for every type, variant and field a
// message is made from the model alone, parsed by serde into the Rust type
// and emitted again, and what comes back must be what the model says. The
// values serde fills in for fields a message leaves out are learnt on the
// way. The same messages are written to fixtures/protocol/synth, where the
// Dart client must turn each input into the same output.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use super::model::{Body, Field, Model, Shape, Skip, Ty, variant_owner};
use super::{ROOTS, reemit};

pub struct Case {
    pub root: &'static str,
    /// The variant names the message carries, outermost first.
    pub tags: Vec<String>,
    pub input: Value,
    pub canonical: Value,
}

/// What serde fills in for a field the input lacks, by owner and Rust name.
pub type Defaults = BTreeMap<(String, String), Value>;

pub struct Proven {
    pub cases: Vec<Case>,
    pub defaults: Defaults,
}

pub fn prove(model: &Model) -> Proven {
    let mut synth = Synth { model, count: 0 };
    let mut check = Check { model, defaults: Defaults::new(), faults: Vec::new() };
    let mut cases = Vec::new();
    for root in ROOTS {
        for input in synth.named(root) {
            let tags = tags(model, root, input.as_object().unwrap());
            let at = format!("{root} {}", tags.join(" "));
            match reemit(root, &input) {
                Ok(canonical) => {
                    check.value(&Ty::Named(root.to_string()), &input, &canonical, &at);
                    cases.push(Case { root, tags, input, canonical });
                }
                Err(e) => check.faults.push(format!("{at}: serde refuses {input}: {e}")),
            }
        }
    }
    for def in &model.types {
        for (owner, fields) in def.field_sets() {
            for f in fields.iter().filter(|f| f.default) {
                if !check.defaults.contains_key(&(owner.clone(), f.rust.clone())) {
                    check.faults.push(format!("{owner}.{}: no sample showed its default", f.rust));
                }
            }
        }
    }
    assert!(
        check.faults.is_empty(),
        "the wire model read from shared/src disagrees with serde:\n  {}",
        check.faults.join("\n  ")
    );
    Proven { cases, defaults: check.defaults }
}

impl Skip {
    fn holds(self, value: &Value) -> bool {
        *value == self.empty()
    }

    /// The value the test leaves out, which is also the one serde's default
    /// must have been when a defaulted field is left out of the output.
    pub fn empty(self) -> Value {
        match self {
            Skip::None => Value::Null,
            Skip::EmptyList => json!([]),
            Skip::EmptyString => json!(""),
            Skip::False => json!(false),
            Skip::Zero => json!(0.0),
        }
    }
}

struct Synth<'a> {
    model: &'a Model,
    count: u32,
}

impl Synth<'_> {
    /// 1 to 97, so that it fits a `u8` and no two neighbours are alike.
    fn next(&mut self) -> u32 {
        self.count = self.count % 97 + 1;
        self.count
    }

    fn values(&mut self, ty: &Ty) -> Vec<Value> {
        match ty {
            Ty::Bool => vec![json!(true), json!(false)],
            Ty::Int => vec![json!(self.next())],
            Ty::Float => vec![json!(f64::from(self.next()) + 0.25)],
            Ty::Str => vec![json!(format!("s{}", self.next()))],
            Ty::Hex => vec![json!({"q": self.next(), "r": -i64::from(self.next())})],
            Ty::Option(inner) => self.values(inner),
            Ty::List(inner) => vec![Value::Array(self.values(inner)), json!([])],
            Ty::Named(name) => self.named(name),
        }
    }

    fn named(&mut self, name: &str) -> Vec<Value> {
        let model = self.model;
        match &model.get(name).body {
            Body::Struct(fields) => self.objects(fields).into_iter().map(Value::Object).collect(),
            Body::Names(variants) => variants.iter().map(|v| json!(v.wire)).collect(),
            Body::Untagged(variants) => variants
                .iter()
                .flat_map(|v| match &v.shape {
                    Shape::Newtype(ty) => self.values(ty),
                    _ => panic!("{name}::{}: an untagged variant must wrap one type", v.rust),
                })
                .collect(),
            Body::Tagged { tag, variants } => variants
                .iter()
                .flat_map(|v| {
                    let objects = match &v.shape {
                        Shape::Unit => vec![Map::new()],
                        Shape::Fields(fields) => self.objects(fields),
                        Shape::Newtype(ty) => self
                            .values(ty)
                            .into_iter()
                            .map(|value| match value {
                                Value::Object(object) => object,
                                _ => panic!("{name}::{}: a tagged variant must wrap an object", v.rust),
                            })
                            .collect(),
                    };
                    objects.into_iter().map(|mut object| {
                        object.insert(tag.clone(), json!(v.wire));
                        Value::Object(object)
                    })
                })
                .collect(),
        }
    }

    /// Enough objects to show every value of every field once, then one with
    /// only what serde demands, then one with every optional field empty. A
    /// field that has run out of values keeps its last, which is its
    /// smallest: the first object is the full one and the rest stay short.
    fn objects(&mut self, fields: &[Field]) -> Vec<Map<String, Value>> {
        let lists: Vec<Vec<Value>> = fields.iter().map(|f| self.values(&f.ty)).collect();
        let most = lists.iter().map(Vec::len).max().unwrap_or(1);
        let last = |i: usize| lists[i].last().unwrap().clone();
        let build = |pick: &dyn Fn(usize, &Field) -> Option<Value>| {
            let mut object = Map::new();
            for (i, f) in fields.iter().enumerate() {
                match (pick(i, f), f.flatten) {
                    (Some(Value::Object(inner)), true) => object.extend(inner),
                    (Some(value), false) => {
                        object.insert(f.wire.clone(), value);
                    }
                    (None, false) => {}
                    (_, true) => panic!("{}: a flattened field must be an object", f.rust),
                }
            }
            object
        };
        let optional = |f: &Field| f.default || matches!(f.ty, Ty::Option(_));

        let mut out: Vec<Map<String, Value>> =
            (0..most).map(|k| build(&|i, _| Some(lists[i].get(k).cloned().unwrap_or_else(|| last(i))))).collect();
        let least = build(&|i, f| (!optional(f)).then(|| last(i)));
        let empty = build(&|i, f| {
            Some(match (f.skip, &f.ty) {
                (Some(skip), _) => skip.empty(),
                (None, Ty::Option(_)) => Value::Null,
                _ => last(i),
            })
        });
        for extra in [least, empty] {
            if !out.contains(&extra) {
                out.push(extra);
            }
        }
        out
    }
}

fn tags(model: &Model, name: &str, object: &Map<String, Value>) -> Vec<String> {
    match &model.get(name).body {
        Body::Tagged { tag, variants } => {
            let wire = object[tag].as_str().unwrap();
            let variant = variants.iter().find(|v| v.wire == wire).unwrap();
            let mut out = vec![wire.to_string()];
            if let Shape::Newtype(Ty::Named(inner)) = &variant.shape {
                out.extend(tags(model, inner, object));
            }
            out
        }
        Body::Struct(fields) => fields
            .iter()
            .filter(|f| f.flatten)
            .filter_map(|f| f.ty.named())
            .flat_map(|inner| tags(model, inner, object))
            .collect(),
        Body::Names(_) | Body::Untagged(_) => Vec::new(),
    }
}

struct Check<'a> {
    model: &'a Model,
    defaults: Defaults,
    faults: Vec<String>,
}

impl Check<'_> {
    fn value(&mut self, ty: &Ty, input: &Value, actual: &Value, at: &str) {
        let model = self.model;
        let object_like = |name: &str| matches!(model.get(name).body, Body::Struct(_) | Body::Tagged { .. });
        match (ty, input, actual) {
            (Ty::Option(_), Value::Null, _) | (Ty::Bool | Ty::Int | Ty::Float | Ty::Str | Ty::Hex, _, _) => {
                self.same(input, actual, at)
            }
            (Ty::Option(inner), _, _) => self.value(inner, input, actual, at),
            (Ty::List(inner), Value::Array(sent), Value::Array(back)) if sent.len() == back.len() => {
                for (i, (sent, back)) in sent.iter().zip(back).enumerate() {
                    self.value(inner, sent, back, &format!("{at}[{i}]"));
                }
            }
            (Ty::Named(name), Value::Object(sent), Value::Object(back)) if object_like(name) => {
                let mut read = BTreeSet::new();
                self.object(name, sent, back, &mut read, at);
                for key in sent.keys().chain(back.keys()).filter(|key| !read.contains(*key)) {
                    self.faults.push(format!("{at}.{key}: on the wire, and no field of {name} in the model"));
                }
            }
            (Ty::Named(name), _, _) if !object_like(name) => self.same(input, actual, at),
            _ => self.faults.push(format!("{at}: sent {input}, serde emits {actual}")),
        }
    }

    fn same(&mut self, input: &Value, actual: &Value, at: &str) {
        if input != actual {
            self.faults.push(format!("{at}: sent {input}, serde emits {actual}"));
        }
    }

    fn object(
        &mut self,
        name: &str,
        input: &Map<String, Value>,
        actual: &Map<String, Value>,
        read: &mut BTreeSet<String>,
        at: &str,
    ) {
        let model = self.model;
        match &model.get(name).body {
            Body::Struct(fields) => self.fields(name, fields, input, actual, read, at),
            Body::Tagged { tag, variants } => {
                read.insert(tag.clone());
                let sent = input.get(tag).and_then(Value::as_str);
                if sent != actual.get(tag).and_then(Value::as_str) {
                    self.faults.push(format!("{at}: {name} sent as {sent:?} comes back as {:?}", actual.get(tag)));
                }
                let Some(variant) = variants.iter().find(|v| Some(v.wire.as_str()) == sent) else {
                    return;
                };
                match &variant.shape {
                    Shape::Unit => {}
                    Shape::Fields(fields) => {
                        self.fields(&variant_owner(name, variant), fields, input, actual, read, at)
                    }
                    Shape::Newtype(Ty::Named(inner)) => self.object(inner, input, actual, read, at),
                    Shape::Newtype(_) => self.faults.push(format!("{at}: {name} wraps what is not an object")),
                }
            }
            Body::Names(_) | Body::Untagged(_) => self.faults.push(format!("{at}: {name} is not an object")),
        }
    }

    fn fields(
        &mut self,
        owner: &str,
        fields: &[Field],
        input: &Map<String, Value>,
        actual: &Map<String, Value>,
        read: &mut BTreeSet<String>,
        at: &str,
    ) {
        for f in fields {
            let at = format!("{at}.{}", f.wire);
            if f.flatten {
                match &f.ty {
                    Ty::Named(inner) => self.object(inner, input, actual, read, &at),
                    _ => self.faults.push(format!("{at}: flattens what is not an object")),
                }
                continue;
            }
            read.insert(f.wire.clone());
            let back = actual.get(&f.wire);
            match input.get(&f.wire) {
                Some(sent) if f.skip.is_some_and(|skip| skip.holds(sent)) => {
                    if let Some(back) = back {
                        self.faults.push(format!("{at}: sent {sent}, expected it left out, serde emits {back}"));
                    }
                }
                Some(sent) => match back {
                    Some(back) => self.value(&f.ty, sent, back, &at),
                    None => self.faults.push(format!("{at}: sent {sent}, serde leaves it out")),
                },
                None if f.default => {
                    let learnt = match (back, f.skip) {
                        (Some(back), Some(skip)) if skip.holds(back) => {
                            self.faults.push(format!("{at}: serde emits {back}, which its skip test should leave out"));
                            continue;
                        }
                        (Some(back), _) => back.clone(),
                        (None, Some(skip)) => skip.empty(),
                        (None, None) => {
                            self.faults.push(format!("{at}: defaulted, never skipped, and serde leaves it out"));
                            continue;
                        }
                    };
                    let key = (owner.to_string(), f.rust.clone());
                    if self.defaults.get(&key).is_some_and(|known| *known != learnt) {
                        self.faults.push(format!("{at}: two defaults, {} and {learnt}", self.defaults[&key]));
                    }
                    self.defaults.insert(key, learnt);
                }
                None => {
                    let expected = match (&f.ty, f.skip) {
                        (Ty::Option(_), Some(Skip::None)) => None,
                        (Ty::Option(_), _) => Some(&Value::Null),
                        _ => {
                            self.faults.push(format!("{at}: the sample left out a field serde demands"));
                            continue;
                        }
                    };
                    if back != expected {
                        self.faults.push(format!("{at}: left out, expected {expected:?}, serde emits {back:?}"));
                    }
                }
            }
        }
    }
}
