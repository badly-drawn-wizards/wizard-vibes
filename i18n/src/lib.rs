// The client's words, compiled in. On a trait, #[langs("lang.json")] (the
// path from the crate's manifest) reads that file at compile time, a table
// of languages ({"en": {...}, "es": {...}}, the first the one the others
// are checked against), each a table of strings or of tables of them, its
// keys snake_case. A string's path (its keys, outermost first) joined with
// _ names the trait method that gives it: lang.json's en.pause.title is
// fn pause_title(&self) -> &'static str. A string with values in it names
// them, {n} (as format! does: {{ and }} for braces), and its method takes
// them, by those names, and gives a String: "{n} enemies defeated" is
// fn defeat_many_defeated(&self, n: usize) -> String, implemented with
// format!("{n} enemies defeated").
//
// It adds to the trait fn get(&self, path: &str) -> Option<&'static str>,
// a string by its dotted path (for words chosen at run time: the
// parameters' labels, from params.json), and generates beside it a unit
// struct implementing it for each language (en: En), and
//     pub enum Lang { En, Es, .. }
// with Lang::ALL, key() ("en"), from_key() and text() (its &'static dyn
// Trait). What it finds wrong (the JSON malformed, a key not snake_case,
// a language without a string another has or with other values in it, a
// method with no string or whose string names a value it doesn't take)
// fails the build, pointing at the attribute or the method.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote};
use serde::de::{Deserialize, Deserializer, Error as _, MapAccess, Visitor};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use syn::spanned::Spanned;
use syn::{Error, FnArg, ItemTrait, LitStr, Pat, ReturnType, TraitItem};

// A language's table, or a string in it (its entries in the file's order).
enum Node {
    Text(String),
    Table(Vec<(String, Node)>),
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Node, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Node;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string, or a table of them")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Node, E> {
                Ok(Node::Text(s.to_string()))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Node, A::Error> {
                let mut out: Vec<(String, Node)> = Vec::new();
                while let Some(k) = m.next_key::<String>()? {
                    if out.iter().any(|(o, _)| *o == k) {
                        return Err(A::Error::custom(format!("`{k}` twice in one table")));
                    }
                    let v = m.next_value()?;
                    out.push((k, v));
                }
                Ok(Node::Table(out))
            }
        }
        d.deserialize_any(V)
    }
}

// A string of a language's: its dotted path, and its text.
struct Leaf {
    path: String,
    text: String,
}

fn snake(k: &str) -> bool {
    !k.is_empty() && k.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') && !k.starts_with(|c: char| c.is_ascii_digit())
}

// Its strings, by method name (their paths joined with _).
fn leaves(lang: &str, node: &Node, at: &mut Vec<String>, out: &mut BTreeMap<String, Leaf>, errs: &mut Vec<String>) {
    match node {
        Node::Text(t) => {
            let name = at.join("_");
            let path = at.join(".");
            if let Some(o) = out.get(&name) {
                errs.push(format!("{lang}.{path} and {lang}.{} both name the method `{name}`", o.path));
            }
            out.insert(name, Leaf { path, text: t.clone() });
        }
        Node::Table(entries) => {
            for (k, v) in entries {
                if !snake(k) {
                    errs.push(format!("`{k}` (in {lang}.{}) is not snake_case", at.join(".")));
                }
                at.push(k.clone());
                leaves(lang, v, at, out, errs);
                at.pop();
            }
        }
    }
}

// The values a string names ({n}), in order; or what is wrong with its
// braces.
fn values(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cs = text.chars().peekable();
    while let Some(c) = cs.next() {
        match c {
            '{' if cs.peek() == Some(&'{') => {
                cs.next();
            }
            '}' if cs.peek() == Some(&'}') => {
                cs.next();
            }
            '{' => {
                let mut name = String::new();
                loop {
                    match cs.next() {
                        Some('}') => break,
                        Some(c) => name.push(c),
                        None => return Err("a { never closed (write {{ for a brace)".into()),
                    }
                }
                if !snake(&name) {
                    return Err(format!("{{{name}}}: a value is named in snake_case (write {{{{ for a brace)"));
                }
                out.push(name);
            }
            '}' => return Err("a } never opened (write }} for a brace)".into()),
            _ => {}
        }
    }
    Ok(out)
}

fn camel(k: &str) -> String {
    k.split('_').map(|w| w[..1].to_uppercase() + &w[1..]).collect()
}

#[proc_macro_attribute]
pub fn langs(attr: TokenStream, item: TokenStream) -> TokenStream {
    let file = syn::parse_macro_input!(attr as LitStr);
    let mut tr = syn::parse_macro_input!(item as ItemTrait);
    let out = expand(&file, &tr);
    // (added either way: failing, only what is wrong is reported)
    let get: TraitItem = syn::parse_quote! {
        // A string by its dotted path (e.g. "param.exposure").
        fn get(&self, path: &str) -> Option<&'static str>;
    };
    tr.items.push(get);
    match out {
        Ok(out) => quote!(#tr #out).into(),
        Err(e) => {
            let e = e.to_compile_error();
            quote!(#tr #e).into()
        }
    }
}

fn expand(file: &LitStr, tr: &ItemTrait) -> syn::Result<Tokens> {
    let err = |m: String| Error::new(file.span(), m);
    let dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|e| err(e.to_string()))?;
    let path = std::path::Path::new(&dir).join(file.value());
    let shown = file.value();
    let text = std::fs::read_to_string(&path).map_err(|e| err(format!("{}: {e}", path.display())))?;
    let root: Node = serde_json::from_str(&text).map_err(|e| {
        // (serde_json's message ends with where; said first instead)
        let m = e.to_string();
        let m = m.rsplit_once(" at line ").map_or(m.as_str(), |(m, _)| m).to_string();
        err(format!("{shown}:{}:{}: {m}", e.line(), e.column()))
    })?;
    let Node::Table(langs) = root else {
        return Err(err(format!("{shown}: a table of languages, by key (\"en\": {{..}}), not a string")));
    };
    if langs.is_empty() {
        return Err(err(format!("{shown}: no languages")));
    }
    // each language's strings, by method name
    let mut errs = Vec::new();
    let mut tables: Vec<(String, BTreeMap<String, Leaf>)> = Vec::new();
    for (key, node) in &langs {
        if !snake(key) {
            errs.push(format!("the language `{key}` is not snake_case"));
        }
        if let Node::Text(_) = node {
            errs.push(format!("the language `{key}` is a string, not a table"));
            continue;
        }
        let mut out = BTreeMap::new();
        leaves(key, node, &mut Vec::new(), &mut out, &mut errs);
        for l in out.values() {
            if let Err(e) = values(&l.text) {
                errs.push(format!("{key}.{}: {e}", l.path));
            }
        }
        tables.push((key.clone(), out));
    }
    // every language with the strings the first has, naming the same values
    let (first, reference) = &tables[0];
    for (key, t) in &tables[1..] {
        for (name, l) in reference {
            match t.get(name) {
                None => errs.push(format!("{key} has no {} (as {first} has)", l.path)),
                Some(o) => {
                    let a: BTreeSet<_> = values(&l.text).unwrap_or_default().into_iter().collect();
                    let b: BTreeSet<_> = values(&o.text).unwrap_or_default().into_iter().collect();
                    if a != b {
                        errs.push(format!("{key}.{} names {:?}, {first}'s {:?}", l.path, b, a));
                    }
                }
            }
        }
        for (name, l) in t {
            if !reference.contains_key(name) {
                errs.push(format!("{key}.{} is not in {first}", l.path));
            }
        }
    }
    if !errs.is_empty() {
        return Err(err(format!("{shown}:\n  {}", errs.join("\n  "))));
    }

    // the methods: each a string's, its values its arguments
    let trait_name = &tr.ident;
    let mut methods: Vec<(syn::Signature, Vec<String>)> = Vec::new();
    let mut problems: Option<Error> = None;
    let mut problem = |e: Error| match &mut problems {
        Some(p) => p.combine(e),
        None => problems = Some(e),
    };
    for it in &tr.items {
        let TraitItem::Fn(f) = it else { continue };
        if f.default.is_some() {
            continue;
        }
        let sig = &f.sig;
        let name = sig.ident.to_string();
        let mut args = Vec::new();
        for (i, a) in sig.inputs.iter().enumerate() {
            match (i, a) {
                (0, FnArg::Receiver(r)) if r.reference.is_some() && r.mutability.is_none() => {}
                (0, _) => problem(Error::new(a.span(), "takes &self")),
                (_, FnArg::Typed(t)) => match &*t.pat {
                    Pat::Ident(p) => args.push(p.ident.to_string()),
                    _ => problem(Error::new(t.pat.span(), "a value's argument is named (as the string names it)")),
                },
                _ => {}
            }
        }
        let Some(l) = reference.get(&name) else {
            let near: Vec<_> = reference.keys().filter(|k| k.starts_with(name.split('_').next().unwrap_or(""))).take(6).cloned().collect();
            problem(Error::new(sig.ident.span(), format!("no string in {shown} for `{name}` (its path's keys joined with _){}", if near.is_empty() { String::new() } else { format!("; like it: {}", near.join(", ")) })));
            continue;
        };
        let ret = match &sig.output {
            ReturnType::Type(_, t) => quote!(#t).to_string().replace(' ', ""),
            ReturnType::Default => String::new(),
        };
        if args.is_empty() && ret != "&'staticstr" {
            problem(Error::new(sig.output.span(), format!("{first}.{} takes no values: gives &'static str", l.path)));
        } else if !args.is_empty() && ret != "String" {
            problem(Error::new(sig.output.span(), format!("{first}.{} takes values: gives String", l.path)));
        }
        for (key, t) in &tables {
            for v in values(&t[&name].text).unwrap_or_default() {
                if !args.contains(&v) {
                    problem(Error::new(sig.ident.span(), format!("{key}.{} names {{{v}}}, not an argument of `{name}`", l.path)));
                }
            }
        }
        methods.push((sig.clone(), args));
    }
    if let Some(p) = problems {
        return Err(p);
    }

    // each language's struct and impl
    let mut impls = Vec::new();
    let mut variants = Vec::new();
    for (key, t) in &tables {
        let ty = format_ident!("{}", camel(key));
        let fns = methods.iter().map(|(sig, args)| {
            let s = &t[&sig.ident.to_string()].text;
            if args.is_empty() {
                quote!(#sig { #s })
            } else {
                let lit = LitStr::new(s, Span::call_site());
                let used: Vec<_> = args.iter().map(|a| format_ident!("{}", a)).collect();
                quote!(#sig { let _ = (#(&#used,)*); format!(#lit) })
            }
        });
        let paths = t.values().map(|l| &l.path);
        let texts = t.values().map(|l| &l.text);
        impls.push(quote! {
            #[derive(Clone, Copy, Debug)]
            pub struct #ty;
            impl #trait_name for #ty {
                #(#fns)*
                fn get(&self, path: &str) -> Option<&'static str> {
                    match path {
                        #(#paths => Some(#texts),)*
                        _ => None,
                    }
                }
            }
        });
        variants.push((key.clone(), ty));
    }
    let n = variants.len();
    let keys: Vec<_> = variants.iter().map(|v| &v.0).collect();
    let tys: Vec<_> = variants.iter().map(|v| &v.1).collect();
    let abs = path.display().to_string();
    Ok(quote! {
        // (rebuilt when the file changes)
        const _: &[u8] = include_bytes!(#abs);

        #(#impls)*

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Lang {
            #(#tys,)*
        }

        impl Lang {
            pub const ALL: [Lang; #n] = [#(Lang::#tys,)*];

            pub fn key(self) -> &'static str {
                match self {
                    #(Lang::#tys => #keys,)*
                }
            }

            pub fn from_key(k: &str) -> Option<Lang> {
                match k {
                    #(#keys => Some(Lang::#tys),)*
                    _ => None,
                }
            }

            pub fn text(self) -> &'static dyn #trait_name {
                match self {
                    #(Lang::#tys => &#tys,)*
                }
            }
        }
    })
}
