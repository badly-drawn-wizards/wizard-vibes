// The words the player reads, in the language chosen (the settings'
// Language row; kept between runs), from client/lang/<language>.json:
// key -> text. The interface's keys are its English (tr("Settings") is
// "Settings" in English, its translation elsewhere, or the English again
// where a table has none); the parameters' (params.json's labels, docs and
// group names, and their options as option.<id>.<option>) are keys, with
// their English in en.json too. A string with values in it marks them {}
// (in order: see trf). Adding a language: a Lang, its name, and its file.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Es,
}

pub const LANGS: [Lang; 2] = [Lang::En, Lang::Es];

impl Lang {
    // Its own name for itself.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn from_key(k: &str) -> Option<Lang> {
        LANGS.into_iter().find(|l| l.key() == k)
    }

    fn table(self) -> &'static HashMap<String, String> {
        static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
        static ES: OnceLock<HashMap<String, String>> = OnceLock::new();
        let load = |text: &str| serde_json::from_str(text).unwrap_or_default();
        match self {
            Lang::En => EN.get_or_init(|| load(include_str!("../lang/en.json"))),
            Lang::Es => ES.get_or_init(|| load(include_str!("../lang/es.json"))),
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    LANGS[LANG.load(Ordering::Relaxed) as usize % LANGS.len()]
}

pub fn set_lang(l: Lang) {
    LANG.store(LANGS.iter().position(|&x| x == l).unwrap_or(0) as u8, Ordering::Relaxed);
}

// `en` in the language chosen.
pub fn tr(en: &'static str) -> &'static str {
    tr_in(lang(), en)
}

pub fn tr_in(l: Lang, en: &'static str) -> &'static str {
    l.table().get(en).map_or(en, |v| v.as_str())
}

// A key's text in the language chosen (else in English, else the key):
// for the parameters' keys, which are not English.
pub fn tr_key(key: &str) -> &str {
    let l = lang();
    match l.table().get(key).or_else(|| Lang::En.table().get(key)) {
        Some(v) => v.as_str(),
        None => key,
    }
}

// The same, its {}s filled in with `args`, in order.
pub fn trf(en: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    trf_in(lang(), en, args)
}

pub fn trf_in(l: Lang, en: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut parts = tr_in(l, en).split("{}");
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for p in parts {
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        out.push_str(p);
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looked_up_and_filled() {
        assert_eq!(tr_in(Lang::En, "Settings"), "Settings");
        assert_eq!(trf_in(Lang::En, "{} enemies defeated", &[&3]), "3 enemies defeated");
        assert_eq!(tr_in(Lang::Es, "Settings"), "Ajustes");
        assert_eq!(trf_in(Lang::Es, "{} enemies defeated", &[&3]), "3 enemigos derrotados");
        assert_eq!(tr_in(Lang::Es, "not in the table"), "not in the table");
        // (the parameters' keys: their English in en.json)
        assert_eq!(Lang::En.table().get("param.exposure").map(String::as_str), Some("Exposure"));
        assert_eq!(Lang::Es.table().get("param.exposure").map(String::as_str), Some("Exposición"));
    }
}
