//! Mehrsprachigkeit: Alle Anzeigetexte stehen in `locales/app.yml`, je
//! Schlüssel auf Deutsch und Englisch. Im Code holt [`tr!`](crate::tr) einen
//! Text in der aktuellen Sprache, Platzhalter heißen `%{name}`:
//!
//! ```ignore
//! tr!("deck_check.siege_aus_partien", wins = 3, games = 5)
//! ```
//!
//! Die Sprache kommt aus den Einstellungen ([`Language`]); „wie das
//! Betriebssystem“ ist der Standard. Kartennamen bleiben englisch, wie in Arena.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// Sprache, in der Texte angezeigt werden.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Locale {
    #[default]
    German,
    English,
}

impl Locale {
    pub const ALL: [Locale; 2] = [Locale::German, Locale::English];

    /// Kürzel in `locales/app.yml`.
    pub fn code(self) -> &'static str {
        match self {
            Locale::German => "de",
            Locale::English => "en",
        }
    }

    /// Name der Sprache in ihr selbst (für die Auswahl).
    pub fn native_name(self) -> &'static str {
        match self {
            Locale::German => "Deutsch",
            Locale::English => "English",
        }
    }

    /// Sprache zu einem Gebietsschema wie „de-DE“ oder „en_US.UTF-8“;
    /// alles außer Deutsch wird Englisch.
    pub fn from_tag(tag: &str) -> Locale {
        let lang = tag.split(['-', '_', '.']).next().unwrap_or("");
        if lang.eq_ignore_ascii_case("de") {
            Locale::German
        } else {
            Locale::English
        }
    }

    /// Sprache des Betriebssystems (Englisch, wenn unbekannt).
    pub fn system() -> Locale {
        sys_locale::get_locale().map_or(Locale::English, |tag| Locale::from_tag(&tag))
    }
}

/// Spracheinstellung: wie das Betriebssystem oder fest gewählt.
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

/// Aktuelle Sprache (Index in [`Locale::ALL`]). Deutsch, bis die App die
/// Einstellung anwendet – Tests laufen damit auf Deutsch.
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Stellt die Sprache für alle folgenden Texte um.
pub fn set(locale: Locale) {
    let i = Locale::ALL.iter().position(|l| *l == locale).unwrap_or(0);
    CURRENT.store(i as u8, Ordering::Relaxed);
}

pub fn current() -> Locale {
    Locale::ALL[usize::from(CURRENT.load(Ordering::Relaxed)) % Locale::ALL.len()]
}

/// Text in der aktuellen Sprache, siehe Moduldoku.
#[macro_export]
macro_rules! tr {
    ($key:literal $(, $($args:tt)*)?) => {
        rust_i18n::t!($key, locale = $crate::i18n::current().code() $(, $($args)*)?).into_owned()
    };
}

#[cfg(test)]
#[path = "tests/i18n.rs"]
mod tests;
