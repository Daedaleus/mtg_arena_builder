//! Internationalisation: all display texts are in `locales/app.yml`, each
//! key in German and English. In code, [`tr!`](crate::tr) fetches a text in
//! the current language; placeholders are called `%{name}`:
//!
//! ```ignore
//! tr!("deck_check.wins_games", wins = 3, games = 5)
//! ```
//!
//! The language comes from the settings ([`Language`]); "like the operating
//! system" is the default. Card names stay English, as in Arena.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// Language in which texts are shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Locale {
    #[default]
    German,
    English,
}

impl Locale {
    pub const ALL: [Locale; 2] = [Locale::German, Locale::English];

    /// Code in `locales/app.yml`.
    pub fn code(self) -> &'static str {
        match self {
            Locale::German => "de",
            Locale::English => "en",
        }
    }

    /// Name of the language in itself (for the choice).
    pub fn native_name(self) -> &'static str {
        match self {
            Locale::German => "Deutsch",
            Locale::English => "English",
        }
    }

    /// Language for a locale like "de-DE" or "en_US.UTF-8"; everything except
    /// German becomes English.
    pub fn from_tag(tag: &str) -> Locale {
        let lang = tag.split(['-', '_', '.']).next().unwrap_or("");
        if lang.eq_ignore_ascii_case("de") {
            Locale::German
        } else {
            Locale::English
        }
    }

    /// Language of the operating system (English if unknown).
    pub fn system() -> Locale {
        sys_locale::get_locale().map_or(Locale::English, |tag| Locale::from_tag(&tag))
    }
}

/// Language setting: like the operating system or chosen explicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    System,
    Fixed(Locale),
}

impl Language {
    pub fn resolve(self) -> Locale {
        match self {
            Language::System => Locale::system(),
            Language::Fixed(l) => l,
        }
    }
}

impl Serialize for Locale {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for Locale {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Locale, D::Error> {
        let code = String::deserialize(d)?;
        Ok(Locale::from_tag(&code))
    }
}

/// Current language (index in [`Locale::ALL`]). German until the app applies
/// the setting – tests therefore run in German.
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Switches the language for all following texts.
pub fn set(locale: Locale) {
    let i = Locale::ALL.iter().position(|l| *l == locale).unwrap_or(0);
    CURRENT.store(i as u8, Ordering::Relaxed);
}

pub fn current() -> Locale {
    Locale::ALL[usize::from(CURRENT.load(Ordering::Relaxed)) % Locale::ALL.len()]
}

/// Text in the current language, see the module docs.
#[macro_export]
macro_rules! tr {
    ($key:literal $(, $($args:tt)*)?) => {
        rust_i18n::t!($key, locale = $crate::i18n::current().code() $(, $($args)*)?).into_owned()
    };
}

#[cfg(test)]
#[path = "tests/i18n.rs"]
mod tests;
