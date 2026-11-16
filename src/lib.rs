//! MTG Arena Builder als Bibliothek: Kartendaten, Sammlung, Turnierdecks,
//! Partie-Auswertung und Begleitfenster. Die ausführbare Datei (`main.rs`)
//! enthält nur Kommandozeile und Programmstart.

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
pub mod meta;
pub mod opponents;
pub mod paths;
pub mod similar;
pub mod turnplan;

pub mod present;
pub mod settings;
#[cfg(test)]
pub mod testutil;
