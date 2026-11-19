//! MTG Arena Builder als Bibliothek: Kartendaten, Sammlung, Turnierdecks,
//! Partie-Auswertung und Begleitfenster. Die ausführbare Datei (`main.rs`)
//! enthält nur Kommandozeile und Programmstart.

// Anzeigetexte aus `locales/app.yml` (siehe `i18n`); fehlt ein englischer
// Text, gilt der deutsche.
rust_i18n::i18n!("locales", fallback = "de");

#[macro_use]
pub mod i18n;

pub mod app;
pub mod budget;
pub mod builder;
pub mod carddb;
pub mod collection;
pub mod combat;
pub mod deck;
pub mod deckcheck;
pub mod games;
pub mod images;
pub mod live;
pub mod magicgg;
pub mod meta;
pub mod opponents;
pub mod paths;
pub mod sim;
pub mod similar;
pub mod style;
pub mod turnplan;

pub mod present;
pub mod progress;
pub mod settings;
#[cfg(test)]
pub mod testutil;
