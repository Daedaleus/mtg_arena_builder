//! MTG Arena Builder as a library: card data, collection, tournament decks,
//! game analysis and companion. The executable (`main.rs`) only contains the
//! command line and program start.

// Display texts from `locales/app.yml` (see `i18n`); if an English text is
// missing, the German one applies.
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
pub mod http;
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
