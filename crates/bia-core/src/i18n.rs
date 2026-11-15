//! Interface translations.
//!
//! Each language is a flat JSON file in `assets/locales/`, compiled into the
//! binary. English is the reference: a key missing from another language
//! falls back to English, and a test makes sure that never ships.
//!
//! Adding a language = one JSON file + one line in [`Lang`].

use std::collections::HashMap;
use std::fmt::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    En,
    PtBr,
}

impl Lang {
    pub const ALL: &'static [Lang] = &[Lang::En, Lang::PtBr];

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::PtBr => "pt-BR",
        }
    }

    fn source(self) -> &'static str {
        match self {
            Lang::En => include_str!("../../../assets/locales/en.json"),
            Lang::PtBr => include_str!("../../../assets/locales/pt-BR.json"),
        }
    }

    /// Maps a BCP-47-ish tag (`pt_BR.UTF-8`, `pt-PT`, `en-US`…) to a language.
    pub fn from_tag(tag: &str) -> Option<Self> {
        let tag = tag.trim().to_ascii_lowercase().replace('_', "-");
        let primary = tag.split(['-', '.', '@']).next().unwrap_or_default();
        match primary {
            "pt" => Some(Lang::PtBr),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    /// Resolves the `language` setting: `"auto"` asks the operating system.
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "" | "auto" => Self::detect(),
            other => Self::from_tag(other).unwrap_or(Lang::En),
        }
    }

    pub fn detect() -> Self {
        sys_locale::get_locale().and_then(|tag| Self::from_tag(&tag)).unwrap_or(Lang::En)
    }
}

#[derive(Debug, Clone)]
pub struct I18n {
    lang: Lang,
    strings: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

fn parse(lang: Lang) -> HashMap<String, String> {
    serde_json::from_str(lang.source()).expect("locale files are valid JSON (tested)")
}

impl I18n {
    pub fn new(lang: Lang) -> Self {
        Self {
            lang,
            strings: parse(lang),
            fallback: if lang == Lang::En { HashMap::new() } else { parse(Lang::En) },
        }
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// The translation for `key`, or `key` itself if nobody translated it.
    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings.get(key).or_else(|| self.fallback.get(key)).map_or(key, String::as_str)
    }

    /// Like [`t`](Self::t), replacing `{name}` placeholders.
    pub fn f(&self, key: &str, args: &[(&str, &dyn Display)]) -> String {
        let mut out = self.t(key).to_owned();
        for (name, value) in args {
            out = out.replace(&format!("{{{name}}}"), &value.to_string());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<String> {
        s.split('{').skip(1).filter_map(|p| p.split_once('}')).map(|(n, _)| n.to_owned()).collect()
    }

    #[test]
    fn every_language_is_complete() {
        let en = parse(Lang::En);
        for &lang in Lang::ALL {
            let other = parse(lang);
            for (key, value) in &en {
                let translated =
                    other.get(key).unwrap_or_else(|| panic!("{} is missing `{key}`", lang.code()));
                assert_eq!(
                    placeholders(value),
                    placeholders(translated),
                    "{}: placeholders differ in `{key}`",
                    lang.code()
                );
            }
            for key in other.keys() {
                assert!(en.contains_key(key), "{} has unknown key `{key}`", lang.code());
            }
        }
    }

    #[test]
    fn tags() {
        assert_eq!(Lang::from_tag("pt_BR.UTF-8"), Some(Lang::PtBr));
        assert_eq!(Lang::from_tag("pt-PT"), Some(Lang::PtBr));
        assert_eq!(Lang::from_tag("en-US"), Some(Lang::En));
        assert_eq!(Lang::from_tag("C"), None);
        assert_eq!(Lang::from_setting("pt-BR"), Lang::PtBr);
        assert_eq!(Lang::from_setting("xx"), Lang::En);
    }

    #[test]
    fn formatting() {
        let i = I18n::new(Lang::PtBr);
        assert_eq!(i.f("find.count", &[("current", &2), ("total", &5)]), "2 de 5");
        assert_eq!(i.t("no.such.key"), "no.such.key");
    }
}
