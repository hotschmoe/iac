// The Dart wire types, written from the model.
//
// The file is laid out the way `dart format` lays it out at `WIDTH` columns,
// so formatting the client leaves it alone: a list that fits its line stays
// on it, one that does not is split one item a line with a trailing comma,
// and a line that would still be too long is refused here.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::model::{Body, Field, Model, Shape, Skip, Ty, TypeDef, Variant, variant_owner};
use super::samples::Defaults;
use super::{Cell, Column, Extras, ROOTS};

/// From the repository root.
pub const PATH: &str = "clients/web/lib/protocol/protocol.g.dart";

const WIDTH: usize = 120;

const HEADER: &str = "\
// GENERATED FILE, DO NOT EDIT.
//
// The wire protocol, written from the Rust types in shared/src (everything
// ClientMessage and ServerMessage carry) by shared/tests/wire. Change the
// Rust, then run
//
//   UPDATE_FIXTURES=1 cargo test -p iac-shared
//
// and `cargo test -p iac-shared` fails for as long as this file is stale.
// What the client adds to these types is in protocol.dart.
//
// Decoding is strict: an unknown tag or enum name throws FormatException.
// A field serde may leave out is nullable or has serde's default here, and
// is left out again when serde would leave it out.
//
// dart format width=120

import 'dart:convert';

import 'hex.dart';

typedef Json = Map<String, dynamic>;

Json _obj(Object? v) => v as Json;
bool _bool(Object? v) => v as bool;
int _int(Object? v) => v as int;
double _double(Object? v) => (v as num).toDouble();
String _string(Object? v) => v as String;
List<T> _list<T>(Object? v, T Function(Object?) f) => [for (final e in v as List) f(e)];
T? _opt<T>(Object? v, T Function(Object?) f) => v == null ? null : f(v);

T _named<T extends Enum>(List<T> values, String Function(T) wire, Object? v) {
  for (final value in values) {
    if (wire(value) == v) return value;
  }
  throw FormatException('unknown $T \"$v\"');
}

bool _same(List<Object?> a, List<Object?> b) {
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
";

const RESERVED: &[&str] = &[
    "abstract", "as", "assert", "async", "await", "break", "case", "catch", "class", "const", "continue",
    "covariant", "default", "deferred", "do", "dynamic", "else", "enum", "export", "extends", "extension",
    "external", "factory", "false", "final", "finally", "for", "Function", "get", "if", "implements", "import",
    "in", "interface", "is", "late", "library", "mixin", "new", "null", "operator", "part", "required",
    "rethrow", "return", "set", "static", "super", "switch", "this", "throw", "true", "try", "typedef", "var",
    "void", "while", "with", "yield",
];

const CLASS_MEMBERS: &[&str] = &["toJson", "fromJson", "hashCode", "runtimeType", "toString", "noSuchMethod"];
const ENUM_MEMBERS: &[&str] = &["toJson", "fromJson", "hashCode", "runtimeType", "toString", "values", "index", "name", "wire"];

pub fn render(model: &Model, defaults: &Defaults, extras: &Extras) -> String {
    let mut out = Writer { text: HEADER.to_string() };
    let scheme = Scheme { model, defaults, extras, parents: parents(model) };
    let mut classes = BTreeSet::new();
    let mut claim = |name: &str| assert!(classes.insert(name.to_string()), "two Dart types would be named {name}");

    for def in &model.types {
        claim(&def.name);
        out.blank();
        match &def.body {
            Body::Names(variants) => scheme.names(&mut out, def, variants),
            Body::Untagged(variants) => scheme.untagged(&mut out, def, variants),
            Body::Struct(fields) => scheme.class(
                &mut out,
                &Class {
                    name: def.name.clone(),
                    docs: &def.docs,
                    owner: def.name.clone(),
                    fields,
                    parent: scheme.parents.get(&def.name),
                    eq: def.eq,
                    whole: extras.rust_defaults.get(&def.name),
                },
            ),
            Body::Tagged { tag, variants } => {
                scheme.sealed(&mut out, def, tag, variants);
                for v in variants.iter().filter(|v| !scheme.inlined(v)) {
                    let name = variant_class(&def.name, v);
                    claim(&name);
                    out.blank();
                    let parent = Parent { class: def.name.clone(), tag: tag.clone(), wire: v.wire.clone() };
                    match &v.shape {
                        Shape::Newtype(ty) => scheme.wrapper(&mut out, &name, v, ty, &parent),
                        Shape::Unit | Shape::Fields(_) => scheme.class(
                            &mut out,
                            &Class {
                                name,
                                docs: &v.docs,
                                owner: variant_owner(&def.name, v),
                                fields: match &v.shape {
                                    Shape::Fields(fields) => fields,
                                    _ => &[],
                                },
                                parent: Some(&parent),
                                eq: false,
                                whole: None,
                            },
                        ),
                    }
                }
            }
        }
    }

    for constant in &extras.constants {
        out.blank();
        out.docs(0, model.const_docs(constant.rust));
        let name = camel(&constant.rust.replace("::", "_").to_lowercase());
        let ty = if matches!(constant.cell, Cell::Real(_)) { "double " } else { "" };
        out.line(0, &format!("const {ty}{name} = {};", cell(&constant.cell)));
    }
    out.text
}

struct Writer {
    text: String,
}

impl Writer {
    fn line(&mut self, indent: usize, text: &str) {
        let width = indent + text.chars().count();
        assert!(width <= WIDTH, "a line of {width} columns, and the Dart file is laid out for {WIDTH}:\n{text}");
        self.text.push_str(&" ".repeat(indent));
        self.text.push_str(text);
        self.text.push('\n');
    }

    fn blank(&mut self) {
        if !self.text.ends_with("\n\n") && !self.text.ends_with("{\n") {
            self.text.push('\n');
        }
    }

    fn docs(&mut self, indent: usize, docs: &[String]) {
        for line in docs {
            self.text.push_str(&" ".repeat(indent));
            self.text.push_str(if line.is_empty() { "///" } else { "/// " });
            self.text.push_str(line);
            self.text.push('\n');
        }
    }

    /// `head`, the items and `tail` on one line when it fits; else an item a line.
    fn list(&mut self, indent: usize, head: &str, items: &[String], tail: &str) {
        let flat = format!("{head}{}{tail}", items.join(", "));
        if items.is_empty() || indent + flat.chars().count() <= WIDTH {
            self.line(indent, &flat);
            return;
        }
        self.line(indent, head);
        for item in items {
            self.line(indent + 2, &format!("{item},"));
        }
        self.line(indent, tail);
    }
}

/// The sealed class a class extends, and the tag it carries there.
struct Parent {
    class: String,
    tag: String,
    wire: String,
}

struct Class<'a> {
    name: String,
    docs: &'a [String],
    /// The name its serde defaults were learnt under.
    owner: String,
    fields: &'a [Field],
    parent: Option<&'a Parent>,
    eq: bool,
    /// Its `Default` in Rust: then every field may be left out in Dart too.
    whole: Option<&'a Value>,
}

struct Scheme<'a> {
    model: &'a Model,
    defaults: &'a Defaults,
    extras: &'a Extras,
    /// The structs a tagged enum wraps and nothing else names: each is
    /// written as a subclass of the enum's sealed class.
    parents: BTreeMap<String, Parent>,
}

fn parents(model: &Model) -> BTreeMap<String, Parent> {
    let mut uses: BTreeMap<&str, u32> = BTreeMap::new();
    for def in &model.types {
        for (_, fields) in def.field_sets() {
            for name in fields.iter().filter_map(|f| f.ty.named()) {
                *uses.entry(name).or_default() += 1;
            }
        }
        if let Body::Tagged { variants, .. } | Body::Untagged(variants) = &def.body {
            for v in variants {
                if let Shape::Newtype(ty) = &v.shape {
                    *uses.entry(ty.named().unwrap_or_default()).or_default() += 1;
                }
            }
        }
    }
    let mut out = BTreeMap::new();
    for def in &model.types {
        let Body::Tagged { tag, variants } = &def.body else { continue };
        for v in variants {
            let Shape::Newtype(Ty::Named(inner)) = &v.shape else { continue };
            if uses[inner.as_str()] == 1 && matches!(model.get(inner).body, Body::Struct(_)) {
                let parent = Parent { class: def.name.clone(), tag: tag.clone(), wire: v.wire.clone() };
                out.insert(inner.clone(), parent);
            }
        }
    }
    out
}

/// `Move` of `Command` is `MoveCommand`; `Command` of `ClientMessage` is `CommandMessage`.
fn variant_class(enum_name: &str, variant: &Variant) -> String {
    let last_word = enum_name.rfind(char::is_uppercase).unwrap_or(0);
    format!("{}{}", variant.rust, &enum_name[last_word..])
}

fn camel(snake: &str) -> String {
    let mut out = String::new();
    for (i, word) in snake.split('_').filter(|word| !word.is_empty()).enumerate() {
        let mut chars = word.chars();
        let first = chars.next().unwrap();
        out.extend(if i == 0 { first.to_lowercase().collect::<Vec<_>>() } else { first.to_uppercase().collect() });
        out.push_str(chars.as_str());
    }
    out
}

fn lower_first(pascal: &str) -> String {
    let mut chars = pascal.chars();
    let first = chars.next().unwrap();
    first.to_lowercase().chain(chars).collect()
}

fn member(name: String, taken: &[&str], at: &str) -> String {
    assert!(
        !RESERVED.contains(&name.as_str()) && !taken.contains(&name.as_str()),
        "{at}: `{name}` cannot be a member's name in Dart; rename it in Rust"
    );
    name
}

fn quote(text: &str) -> String {
    let mut out = String::from("'");
    for c in text.chars() {
        match c {
            '\\' | '\'' | '$' => out.extend(['\\', c]),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('\'');
    out
}

fn number(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}

fn cell(cell: &Cell) -> String {
    match cell {
        Cell::Text(text) => quote(text),
        Cell::Int(n) => n.to_string(),
        Cell::Real(x) => number(*x),
    }
}

fn dart_type(ty: &Ty) -> String {
    match ty {
        Ty::Bool => "bool".into(),
        Ty::Int => "int".into(),
        Ty::Float => "double".into(),
        Ty::Str => "String".into(),
        Ty::Hex => "Hex".into(),
        Ty::Option(inner) => format!("{}?", dart_type(inner)),
        Ty::List(inner) => format!("List<{}>", dart_type(inner)),
        Ty::Named(name) => name.clone(),
    }
}

fn plain(ty: &Ty) -> bool {
    matches!(ty, Ty::Bool | Ty::Int | Ty::Float | Ty::Str)
}

/// A function from the decoded JSON to the Dart value.
fn decoder(ty: &Ty) -> String {
    match ty {
        Ty::Bool => "_bool".into(),
        Ty::Int => "_int".into(),
        Ty::Float => "_double".into(),
        Ty::Str => "_string".into(),
        Ty::Hex => "Hex.fromJson".into(),
        Ty::Named(name) => format!("{name}.fromJson"),
        Ty::List(inner) => format!("(v) => _list(v, {})", decoder(inner)),
        Ty::Option(inner) => format!("(v) => _opt(v, {})", decoder(inner)),
    }
}

fn decode(ty: &Ty, from: &str) -> String {
    match ty {
        Ty::List(inner) => format!("_list({from}, {})", decoder(inner)),
        Ty::Option(inner) => format!("_opt({from}, {})", decoder(inner)),
        _ => format!("{}({from})", decoder(ty)),
    }
}

fn encode(ty: &Ty, value: &str, at: &str) -> String {
    match ty {
        _ if plain(ty) => value.to_string(),
        Ty::Hex | Ty::Named(_) => format!("{value}.toJson()"),
        Ty::List(inner) if plain(inner) => value.to_string(),
        Ty::List(inner) => format!("{value}.map((e) => {}).toList()", encode_item(inner, at)),
        Ty::Option(inner) => match &**inner {
            Ty::Hex | Ty::Named(_) => format!("{value}?.toJson()"),
            Ty::List(item) if !plain(item) => format!("{value}?.map((e) => {}).toList()", encode_item(item, at)),
            Ty::Option(_) => panic!("{at}: an Option of an Option has no JSON of its own"),
            _ => value.to_string(),
        },
        _ => unreachable!(),
    }
}

fn encode_item(ty: &Ty, at: &str) -> String {
    assert!(plain(ty) || matches!(ty, Ty::Hex | Ty::Named(_)), "{at}: a list of lists or of options is not written");
    encode(ty, "e", at)
}

impl Scheme<'_> {
    fn inlined(&self, variant: &Variant) -> bool {
        matches!(&variant.shape, Shape::Newtype(Ty::Named(inner)) if self.parents.contains_key(inner))
    }

    fn columns(&self, enum_name: &str) -> Vec<&Column> {
        self.extras.columns.iter().filter(|(owner, _)| owner == enum_name).map(|(_, column)| column).collect()
    }

    fn names(&self, out: &mut Writer, def: &TypeDef, variants: &[Variant]) {
        let columns = self.columns(&def.name);
        let taken: Vec<&str> = ENUM_MEMBERS.iter().copied().chain(columns.iter().map(|c| c.name)).collect();
        out.docs(0, &def.docs);
        out.line(0, &format!("enum {} {{", def.name));
        for (i, v) in variants.iter().enumerate() {
            if i > 0 && !(v.docs.is_empty() && variants[i - 1].docs.is_empty()) {
                out.blank();
            }
            out.docs(2, &v.docs);
            let name = member(lower_first(&v.rust), &taken, &def.name);
            let mut args = vec![quote(&v.wire)];
            args.extend(columns.iter().map(|c| format!("{}: {}", c.name, cell(&c.cells[i]))));
            let end = if i + 1 == variants.len() { ';' } else { ',' };
            out.list(2, &format!("{name}("), &args, &format!("){end}"));
        }
        out.blank();
        let named: Vec<String> = columns.iter().map(|c| format!("required this.{}", c.name)).collect();
        let flat = format!("const {}(this.wire, {{{}}});", def.name, named.join(", "));
        if named.is_empty() {
            out.line(2, &format!("const {}(this.wire);", def.name));
        } else if 2 + flat.chars().count() <= WIDTH {
            out.line(2, &flat);
        } else {
            out.line(2, &format!("const {}(", def.name));
            out.line(4, "this.wire, {");
            for param in &named {
                out.line(4, &format!("{param},"));
            }
            out.line(2, "});");
        }
        out.blank();
        out.line(2, "final String wire;");
        for c in &columns {
            let ty = match c.cells[0] {
                Cell::Text(_) => "String",
                Cell::Int(_) => "int",
                Cell::Real(_) => "double",
            };
            out.line(2, &format!("final {ty} {};", c.name));
        }
        out.blank();
        out.line(2, &format!("static {} fromJson(Object? json) => _named(values, (v) => v.wire, json);", def.name));
        out.blank();
        out.line(2, "String toJson() => wire;");
        out.line(0, "}");
    }

    /// One nullable field a variant: the wire carries the payload alone, so
    /// decoding takes the first variant that reads it, as serde does.
    fn untagged(&self, out: &mut Writer, def: &TypeDef, variants: &[Variant]) {
        let payloads: Vec<(String, &str)> = variants
            .iter()
            .map(|v| match &v.shape {
                Shape::Newtype(Ty::Named(inner)) if matches!(self.model.get(inner).body, Body::Names(_)) => {
                    (member(lower_first(&v.rust), CLASS_MEMBERS, &def.name), inner.as_str())
                }
                _ => panic!("{}::{}: an untagged variant is written only when it wraps an enum of names", def.name, v.rust),
            })
            .collect();
        let name = &def.name;
        out.docs(0, &def.docs);
        out.line(0, &format!("class {name} {{"));
        for (field, ty) in &payloads {
            out.line(2, &format!("final {ty}? {field};"));
        }
        out.blank();
        for (field, ty) in &payloads {
            let others: Vec<String> =
                payloads.iter().filter(|(other, _)| other != field).map(|(other, _)| format!("{other} = null")).collect();
            out.line(2, &format!("const {name}.{field}({ty} this.{field}) : {};", others.join(", ")));
        }
        out.blank();
        out.line(2, &format!("factory {name}.fromJson(Object? json) {{"));
        for (field, ty) in &payloads {
            out.line(4, &format!("for (final v in {ty}.values) {{"));
            out.line(6, &format!("if (v.wire == json) return {name}.{field}(v);"));
            out.line(4, "}");
        }
        out.line(4, &format!("throw FormatException('unknown {name} \"$json\"');"));
        out.line(2, "}");
        out.blank();
        let (last, rest) = payloads.split_last().unwrap();
        let rest: String = rest.iter().map(|(field, _)| format!("{field}?.toJson() ?? ")).collect();
        out.line(2, &format!("String toJson() => {rest}{}!.toJson();", last.0));
        if def.eq {
            let props: Vec<String> = payloads.iter().map(|(field, _)| field.clone()).collect();
            self.equality(out, name, &props);
        }
        out.line(0, "}");
    }

    fn sealed(&self, out: &mut Writer, def: &TypeDef, tag: &str, variants: &[Variant]) {
        let name = &def.name;
        let root = ROOTS.contains(&name.as_str());
        out.docs(0, &def.docs);
        out.line(0, &format!("sealed class {name} {{"));
        out.line(2, &format!("const {name}();"));
        out.blank();
        out.line(2, &format!("static {name} fromJson(Object? json) {{"));
        out.line(4, &format!("return switch (_obj(json)[{}]) {{", quote(tag)));
        for v in variants {
            let class = match &v.shape {
                Shape::Newtype(Ty::Named(inner)) if self.inlined(v) => inner.clone(),
                _ => variant_class(name, v),
            };
            let made = match &v.shape {
                Shape::Unit => format!("const {class}()"),
                Shape::Fields(fields) if fields.is_empty() => format!("const {class}()"),
                _ => format!("{class}.fromJson(json)"),
            };
            out.line(6, &format!("{} => {made},", quote(&v.wire)));
        }
        out.line(6, &format!("final tag => throw FormatException('unknown {name} {tag} \"$tag\"'),"));
        out.line(4, "};");
        out.line(2, "}");
        if root {
            out.blank();
            out.line(2, &format!("static {name} decode(String raw) => fromJson(jsonDecode(raw));"));
        }
        out.blank();
        out.line(2, "Json toJson();");
        if root {
            out.blank();
            out.line(2, "String encode() => jsonEncode(toJson());");
        }
        out.line(0, "}");
    }

    fn wrapper(&self, out: &mut Writer, name: &str, variant: &Variant, ty: &Ty, parent: &Parent) {
        let Ty::Named(inner) = ty else {
            panic!("{name}: a tagged variant is written only when it wraps a struct or a tagged enum");
        };
        assert!(
            matches!(self.model.get(inner).body, Body::Struct(_) | Body::Tagged { .. }),
            "{name}: {inner} is not an object on the wire, and serde cannot tag it"
        );
        let field = member(lower_first(&variant.rust), CLASS_MEMBERS, name);
        out.docs(0, &variant.docs);
        out.line(0, &format!("class {name} extends {} {{", parent.class));
        out.line(2, &format!("final {inner} {field};"));
        out.blank();
        out.line(2, &format!("const {name}(this.{field});"));
        out.blank();
        out.line(2, &format!("factory {name}.fromJson(Object? json) => {name}({inner}.fromJson(json));"));
        out.blank();
        out.line(2, "@override");
        let tag = format!("{}: {}", quote(&parent.tag), quote(&parent.wire));
        out.line(2, &format!("Json toJson() => {{{tag}, ...{field}.toJson()}};"));
        out.line(0, "}");
    }

    fn class(&self, out: &mut Writer, class: &Class) {
        let name = &class.name;
        let fields: Vec<(String, &Field)> =
            class.fields.iter().map(|f| (member(camel(&f.rust), CLASS_MEMBERS, name), f)).collect();

        out.docs(0, class.docs);
        match class.parent {
            Some(parent) => out.line(0, &format!("class {name} extends {} {{", parent.class)),
            None => out.line(0, &format!("class {name} {{")),
        }
        for (i, (field, f)) in fields.iter().enumerate() {
            if i > 0 && !f.docs.is_empty() {
                out.blank();
            }
            out.docs(2, &f.docs);
            out.line(2, &format!("final {} {field};", dart_type(&f.ty)));
        }

        out.blank();
        let params: Vec<String> = fields.iter().map(|(field, f)| self.param(class, field, f)).collect();
        match params.is_empty() {
            true => out.line(2, &format!("const {name}();")),
            false => out.list(2, &format!("const {name}({{"), &params, "});"),
        }

        if !fields.is_empty() {
            out.blank();
            out.line(2, &format!("factory {name}.fromJson(Object? json) {{"));
            if fields.iter().any(|(_, f)| !f.flatten) {
                out.line(4, "final m = _obj(json);");
            }
            let args: Vec<String> =
                fields.iter().map(|(field, f)| format!("{field}: {}", self.read(class, f))).collect();
            out.list(4, &format!("return {name}("), &args, ");");
            out.line(2, "}");
        }

        out.blank();
        let mut entries = Vec::new();
        if let Some(parent) = class.parent {
            out.line(2, "@override");
            entries.push(format!("{}: {}", quote(&parent.tag), quote(&parent.wire)));
        }
        entries.extend(fields.iter().map(|(field, f)| entry(field, f, name)));
        out.list(2, "Json toJson() => {", &entries, "};");

        if class.eq {
            let listed = |ty: &Ty| matches!(ty, Ty::List(_)) || matches!(ty, Ty::Option(inner) if matches!(**inner, Ty::List(_)));
            assert!(
                !fields.iter().any(|(_, f)| listed(&f.ty)),
                "{name}: Eq in Rust, and a list in it would need a deep comparison in Dart"
            );
            let props: Vec<String> = fields.iter().map(|(field, _)| field.clone()).collect();
            self.equality(out, name, &props);
        }
        out.line(0, "}");
    }

    fn equality(&self, out: &mut Writer, name: &str, props: &[String]) {
        out.blank();
        out.list(2, "List<Object?> get _props => [", props, "];");
        out.blank();
        out.line(2, "@override");
        out.line(2, &format!("bool operator ==(Object other) => other is {name} && _same(_props, other._props);"));
        out.blank();
        out.line(2, "@override");
        out.line(2, "int get hashCode => Object.hashAll(_props);");
    }

    /// The value a field takes when the constructor is not given one.
    fn default_of(&self, class: &Class, f: &Field) -> Option<Value> {
        match class.whole {
            Some(whole) => Some(whole.get(&f.wire).cloned().unwrap_or(Value::Null)),
            None => f.default.then(|| self.defaults[&(class.owner.clone(), f.rust.clone())].clone()),
        }
    }

    fn param(&self, class: &Class, field: &str, f: &Field) -> String {
        let at = format!("{}.{field}", class.name);
        let optional = matches!(f.ty, Ty::Option(_));
        match self.default_of(class, f) {
            Some(Value::Null) if optional => format!("this.{field}"),
            Some(value) => format!("this.{field} = {}", self.constant(&f.ty, &value, &at)),
            None if optional => format!("this.{field}"),
            None => format!("required this.{field}"),
        }
    }

    fn read(&self, class: &Class, f: &Field) -> String {
        let from = format!("m[{}]", quote(&f.wire));
        let at = format!("{}.{}", class.name, f.rust);
        let default = f.default.then(|| &self.defaults[&(class.owner.clone(), f.rust.clone())]);
        match (&f.ty, f.flatten, default) {
            (Ty::Named(inner), true, _) => format!("{inner}.fromJson(json)"),
            (_, true, _) => panic!("{at}: flattens what is not a named type"),
            (Ty::Option(_), _, Some(value)) if !value.is_null() => {
                panic!("{at}: an Option that serde defaults to {value}, not to None, is not written")
            }
            (Ty::Option(_), _, _) | (_, _, None) => decode(&f.ty, &from),
            (ty, _, Some(value)) => format!("_opt({from}, {}) ?? {}", decoder(ty), self.constant(ty, value, &at)),
        }
    }

    /// `value` as a Dart constant of type `ty`.
    fn constant(&self, ty: &Ty, value: &Value, at: &str) -> String {
        let literal = self.literal(ty, value, at);
        let built = match ty {
            Ty::Option(inner) => !value.is_null() && !plain(inner) && !self.is_names(inner),
            _ => !plain(ty) && !self.is_names(ty),
        };
        if built { format!("const {literal}") } else { literal }
    }

    fn is_names(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::Named(name) if matches!(self.model.get(name).body, Body::Names(_)))
    }

    fn literal(&self, ty: &Ty, value: &Value, at: &str) -> String {
        let refuse = || -> String { panic!("{at}: {value} is no constant of its type the Dart generator can write") };
        match (ty, value) {
            (Ty::Option(_), Value::Null) => "null".into(),
            (Ty::Option(inner), _) => self.literal(inner, value, at),
            (Ty::Bool, Value::Bool(b)) => b.to_string(),
            (Ty::Int, Value::Number(n)) => n.as_i64().map(|n| n.to_string()).unwrap_or_else(refuse),
            (Ty::Float, Value::Number(n)) => n.as_f64().map(number).unwrap_or_else(refuse),
            (Ty::Str, Value::String(text)) => quote(text),
            (Ty::Hex, Value::Object(hex)) => format!("Hex({}, {})", hex["q"], hex["r"]),
            (Ty::List(inner), Value::Array(items)) => {
                let items: Vec<String> = items.iter().map(|item| self.literal(inner, item, at)).collect();
                format!("[{}]", items.join(", "))
            }
            (Ty::Named(name), _) => match (&self.model.get(name).body, value) {
                (Body::Names(variants), Value::String(wire)) => match variants.iter().find(|v| v.wire == *wire) {
                    Some(v) => format!("{name}.{}", lower_first(&v.rust)),
                    None => refuse(),
                },
                (Body::Struct(_), _) if self.extras.rust_defaults.get(name) == Some(value) => format!("{name}()"),
                (Body::Struct(fields), Value::Object(object)) => {
                    let args: Vec<String> = fields
                        .iter()
                        .filter_map(|f| Some((f, object.get(&f.wire)?)))
                        .map(|(f, value)| format!("{}: {}", camel(&f.rust), self.literal(&f.ty, value, at)))
                        .collect();
                    format!("{name}({})", args.join(", "))
                }
                _ => refuse(),
            },
            _ => refuse(),
        }
    }
}

fn entry(field: &str, f: &Field, class: &str) -> String {
    let at = format!("{class}.{field}");
    if f.flatten {
        return format!("...{field}.toJson()");
    }
    let key = quote(&f.wire);
    let value = encode(&f.ty, field, &at);
    let fits = |skip: Skip| {
        matches!(
            (skip, &f.ty),
            (Skip::None, Ty::Option(_))
                | (Skip::EmptyList, Ty::List(_))
                | (Skip::EmptyString, Ty::Str)
                | (Skip::False, Ty::Bool)
                | (Skip::Zero, Ty::Float)
        )
    };
    match f.skip {
        Some(skip) if !fits(skip) => panic!("{at}: its skip_serializing_if is not one for its type"),
        None => format!("{key}: {value}"),
        Some(Skip::None) => format!("{key}: ?{value}"),
        Some(Skip::EmptyList | Skip::EmptyString) => format!("if ({field}.isNotEmpty) {key}: {value}"),
        Some(Skip::False) => format!("if ({field}) {key}: {value}"),
        Some(Skip::Zero) => format!("if ({field} != 0) {key}: {value}"),
    }
}
