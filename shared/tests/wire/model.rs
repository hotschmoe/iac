// The wire types as the Rust source declares them: every struct and enum
// reachable from the protocol's roots, read out of shared/src with `syn`.
//
// Reading the source means reading serde's attributes too. Whatever is not
// understood here is refused by name, and `samples.rs` holds what is
// understood against what serde really emits.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use syn::{Attribute, Expr, Fields, GenericArgument, Item, Lit, LitStr, Meta, PathArguments, Token, Type};

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Bool,
    Int,
    Float,
    Str,
    Hex,
    Option(Box<Ty>),
    List(Box<Ty>),
    Named(String),
}

/// When `skip_serializing_if` leaves a field out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Skip {
    None,
    EmptyList,
    EmptyString,
    False,
    Zero,
}

pub struct Field {
    pub rust: String,
    pub wire: String,
    pub docs: Vec<String>,
    pub ty: Ty,
    pub flatten: bool,
    /// serde fills the field when the input lacks it.
    pub default: bool,
    pub skip: Option<Skip>,
}

pub enum Shape {
    Unit,
    Newtype(Ty),
    Fields(Vec<Field>),
}

pub struct Variant {
    pub rust: String,
    pub wire: String,
    pub docs: Vec<String>,
    pub shape: Shape,
}

pub enum Body {
    Struct(Vec<Field>),
    /// An enum of unit variants: a string on the wire.
    Names(Vec<Variant>),
    /// `#[serde(tag = "...")]`: an object that carries its variant's name.
    Tagged { tag: String, variants: Vec<Variant> },
    /// `#[serde(untagged)]`: whatever its payload is.
    Untagged(Vec<Variant>),
}

pub struct TypeDef {
    pub name: String,
    pub docs: Vec<String>,
    pub eq: bool,
    pub rust_default: bool,
    pub body: Body,
}

pub struct Model {
    /// In source order, `protocol.rs` first.
    pub types: Vec<TypeDef>,
    const_docs: BTreeMap<String, Vec<String>>,
}

impl Model {
    pub fn read(src: &Path, roots: &[&str]) -> Model {
        let mut names: Vec<String> = fs::read_dir(src)
            .unwrap_or_else(|e| panic!("{}: {e}", src.display()))
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".rs"))
            .collect();
        names.sort_by_key(|name| (name.as_str() != "protocol.rs", name.clone()));

        let files: Vec<syn::File> = names
            .iter()
            .map(|name| {
                let text = fs::read_to_string(src.join(name)).unwrap();
                syn::parse_file(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
            })
            .collect();

        let mut declared: Vec<(String, &Item)> = Vec::new();
        let mut defaulted = BTreeSet::new();
        let mut const_docs = BTreeMap::new();
        for item in files.iter().flat_map(|file| &file.items) {
            match item {
                Item::Struct(s) if derives(&s.attrs).contains("Serialize") => declared.push((s.ident.to_string(), item)),
                Item::Enum(e) if derives(&e.attrs).contains("Serialize") => declared.push((e.ident.to_string(), item)),
                Item::Impl(i) => {
                    let of_default = i.trait_.as_ref().is_some_and(|(_, path, _)| path.is_ident("Default"));
                    if let (true, Type::Path(p)) = (of_default, &*i.self_ty) {
                        defaulted.extend(p.path.get_ident().map(ToString::to_string));
                    }
                }
                Item::Const(c) => {
                    const_docs.insert(c.ident.to_string(), docs(&c.attrs));
                }
                _ => {}
            }
        }

        let mut reached = BTreeMap::new();
        let mut waiting: Vec<String> = roots.iter().map(ToString::to_string).collect();
        while let Some(name) = waiting.pop() {
            if reached.contains_key(&name) {
                continue;
            }
            let mut found = declared.iter().filter(|(declared, _)| *declared == name);
            let (Some((_, item)), None) = (found.next(), found.next()) else {
                panic!("{name}: the wire uses it, and shared/src must declare it exactly once with derive(Serialize)");
            };
            let def = read_type(item, defaulted.contains(&name));
            waiting.extend(named_in(&def));
            reached.insert(name, def);
        }

        let types = declared.iter().filter_map(|(name, _)| reached.remove(name)).collect();
        Model { types, const_docs }
    }

    pub fn get(&self, name: &str) -> &TypeDef {
        self.types.iter().find(|def| def.name == name).unwrap_or_else(|| panic!("{name} is not a wire type"))
    }

    /// The doc comment of a `const` in shared/src.
    pub fn const_docs(&self, name: &str) -> &[String] {
        self.const_docs.get(name).unwrap_or_else(|| panic!("no const {name} in shared/src"))
    }
}

impl Ty {
    pub fn named(&self) -> Option<&str> {
        match self {
            Ty::Named(name) => Some(name),
            Ty::Option(inner) | Ty::List(inner) => inner.named(),
            _ => None,
        }
    }
}

impl TypeDef {
    /// Every field the type holds, with the name its serde defaults are kept under.
    pub fn field_sets(&self) -> Vec<(String, &[Field])> {
        match &self.body {
            Body::Struct(fields) => vec![(self.name.clone(), fields.as_slice())],
            Body::Tagged { variants, .. } => variants
                .iter()
                .filter_map(|v| match &v.shape {
                    Shape::Fields(fields) => Some((variant_owner(&self.name, v), fields.as_slice())),
                    _ => None,
                })
                .collect(),
            Body::Names(_) | Body::Untagged(_) => Vec::new(),
        }
    }
}

pub fn variant_owner(enum_name: &str, variant: &Variant) -> String {
    format!("{enum_name}::{}", variant.rust)
}

fn named_in(def: &TypeDef) -> Vec<String> {
    let mut out: Vec<String> = def
        .field_sets()
        .iter()
        .flat_map(|(_, fields)| fields.iter())
        .filter_map(|f| f.ty.named().map(ToString::to_string))
        .collect();
    if let Body::Tagged { variants, .. } | Body::Untagged(variants) = &def.body {
        for v in variants {
            if let Shape::Newtype(ty) = &v.shape {
                out.extend(ty.named().map(ToString::to_string));
            }
        }
    }
    out
}

fn read_type(item: &Item, rust_default: bool) -> TypeDef {
    match item {
        Item::Struct(s) => {
            let name = s.ident.to_string();
            assert!(s.generics.params.is_empty(), "{name}: a generic type has no one shape on the wire");
            let container = Serde::read(&s.attrs, &["default"], &name);
            let Fields::Named(named) = &s.fields else {
                panic!("{name}: only structs with named fields are read");
            };
            let mut fields = read_fields(named, &name);
            if container.default {
                fields.iter_mut().for_each(|f| f.default = true);
            }
            let derived = derives(&s.attrs);
            TypeDef {
                name,
                docs: docs(&s.attrs),
                eq: derived.contains("Eq"),
                rust_default: rust_default || derived.contains("Default"),
                body: Body::Struct(fields),
            }
        }
        Item::Enum(e) => {
            let name = e.ident.to_string();
            assert!(e.generics.params.is_empty(), "{name}: a generic type has no one shape on the wire");
            let container = Serde::read(&e.attrs, &["tag", "rename_all", "untagged"], &name);
            if let Some(rule) = &container.rename_all {
                assert_eq!(rule, "snake_case", "{name}: rename_all = \"{rule}\" is not read, only snake_case");
            }
            let variants: Vec<Variant> = e
                .variants
                .iter()
                .map(|v| {
                    let rust = v.ident.to_string();
                    let at = format!("{name}::{rust}");
                    let own = Serde::read(&v.attrs, &["rename"], &at);
                    let shape = match &v.fields {
                        Fields::Unit => Shape::Unit,
                        Fields::Named(named) => Shape::Fields(read_fields(named, &at)),
                        Fields::Unnamed(tuple) if tuple.unnamed.len() == 1 => {
                            Shape::Newtype(read_ty(&tuple.unnamed[0].ty, &at))
                        }
                        Fields::Unnamed(_) => panic!("{at}: a tuple variant of several fields is not read"),
                    };
                    let wire = own.rename.unwrap_or_else(|| match container.rename_all {
                        Some(_) => snake_case(&rust),
                        None => rust.clone(),
                    });
                    Variant { rust, wire, docs: docs(&v.attrs), shape }
                })
                .collect();
            let all_unit = variants.iter().all(|v| matches!(v.shape, Shape::Unit));
            let body = match (container.tag, container.untagged) {
                (Some(tag), false) => Body::Tagged { tag, variants },
                (None, true) => Body::Untagged(variants),
                (None, false) if all_unit => Body::Names(variants),
                (None, false) => panic!("{name}: an enum with data needs serde(tag) or serde(untagged) to be read"),
                (Some(_), true) => panic!("{name}: tagged and untagged at once"),
            };
            let derived = derives(&e.attrs);
            TypeDef { name, docs: docs(&e.attrs), eq: derived.contains("Eq"), rust_default: false, body }
        }
        _ => unreachable!(),
    }
}

fn read_fields(named: &syn::FieldsNamed, owner: &str) -> Vec<Field> {
    named
        .named
        .iter()
        .map(|f| {
            let rust = f.ident.as_ref().unwrap().to_string();
            let at = format!("{owner}.{rust}");
            let own = Serde::read(&f.attrs, &["rename", "default", "flatten", "skip_serializing_if"], &at);
            let skip = own.skip.as_deref().map(|path| match path {
                "Option::is_none" => Skip::None,
                "Vec::is_empty" => Skip::EmptyList,
                "String::is_empty" => Skip::EmptyString,
                "std::ops::Not::not" => Skip::False,
                "is_zero" => Skip::Zero,
                other => panic!("{at}: skip_serializing_if = \"{other}\" is not a test the Dart generator knows"),
            });
            Field {
                wire: own.rename.unwrap_or_else(|| rust.clone()),
                rust,
                docs: docs(&f.attrs),
                ty: read_ty(&f.ty, &at),
                flatten: own.flatten,
                default: own.default,
                skip,
            }
        })
        .collect()
}

fn read_ty(ty: &Type, at: &str) -> Ty {
    fn refuse(at: &str) -> ! {
        panic!("{at}: a type the Dart generator does not read")
    }
    let Type::Path(path) = ty else { refuse(at) };
    if path.qself.is_some() {
        refuse(at);
    }
    let last = path.path.segments.last().unwrap();
    let inner = || match &last.arguments {
        PathArguments::AngleBracketed(args) if args.args.len() == 1 => match &args.args[0] {
            GenericArgument::Type(inner) => Box::new(read_ty(inner, at)),
            _ => refuse(at),
        },
        _ => refuse(at),
    };
    let plain = matches!(last.arguments, PathArguments::None);
    match last.ident.to_string().as_str() {
        "Option" => Ty::Option(inner()),
        "Vec" => Ty::List(inner()),
        _ if !plain => refuse(at),
        "bool" => Ty::Bool,
        "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" => Ty::Int,
        "f32" | "f64" => Ty::Float,
        "String" => Ty::Str,
        "Hex" => Ty::Hex,
        "usize" | "isize" => panic!("{at}: a field sized by the target has no place on the wire"),
        name => Ty::Named(name.to_string()),
    }
}

#[derive(Default)]
struct Serde {
    rename: Option<String>,
    rename_all: Option<String>,
    tag: Option<String>,
    untagged: bool,
    default: bool,
    flatten: bool,
    skip: Option<String>,
}

impl Serde {
    fn read(attrs: &[Attribute], allowed: &[&str], at: &str) -> Serde {
        let mut out = Serde::default();
        for attr in attrs.iter().filter(|attr| attr.path().is_ident("serde")) {
            let read = attr.parse_nested_meta(|meta| {
                let key = meta.path.get_ident().map(ToString::to_string).unwrap_or_default();
                assert!(allowed.contains(&key.as_str()), "{at}: serde({key}) is not read by the Dart generator here");
                let text = || -> syn::Result<String> { Ok(meta.value()?.parse::<LitStr>()?.value()) };
                match key.as_str() {
                    "rename" => out.rename = Some(text()?),
                    "rename_all" => out.rename_all = Some(text()?),
                    "tag" => out.tag = Some(text()?),
                    "skip_serializing_if" => out.skip = Some(text()?),
                    "untagged" => out.untagged = true,
                    "flatten" => out.flatten = true,
                    "default" => {
                        out.default = true;
                        if meta.input.peek(Token![=]) {
                            text()?;
                        }
                    }
                    _ => unreachable!(),
                }
                Ok(())
            });
            read.unwrap_or_else(|e| panic!("{at}: {e}"));
        }
        out
    }
}

fn derives(attrs: &[Attribute]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("derive")) {
        let _ = attr.parse_nested_meta(|meta| {
            out.extend(meta.path.segments.last().map(|segment| segment.ident.to_string()));
            Ok(())
        });
    }
    out
}

fn docs(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| match &attr.meta {
            Meta::NameValue(pair) => match &pair.value {
                Expr::Lit(lit) => match &lit.lit {
                    Lit::Str(text) => Some(text.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .map(|line| line.strip_prefix(' ').unwrap_or(&line).trim_end().to_string())
        .collect()
}

/// serde's `rename_all = "snake_case"` for a variant name.
fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.char_indices() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}
