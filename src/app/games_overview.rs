//! „Meine Partien“ > „Übersicht & Tipps“: Bilanz, Verbesserungen, Gegner.

use super::*;

impl App {
    /// Übersicht; liefert die Partie, deren Eintrag angeklickt wurde.
    pub(super) fn games_overview(
        &self,
        ui: &mut egui::Ui,
        r: &games::Report,
        opp_groups: &[opponents::OppGroup],
    ) -> Option<usize> {
        let mut clicked = None;
        // --- Bilanz ---
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let color = if r.wins >= r.losses { GREEN } else { RED };
            ui.label(
                RichText::new(format!("{} Siege, {} Niederlagen", r.wins, r.losses))
                    .color(color)
                    .strong(),
            );
            ui.label(format!("aus {} Partien", r.games));
            if r.conceded > 0 {
                ui.label(format!("· {} davon aufgegeben", r.conceded));
            }
        });
        ui.label(format!(
            "Als Erster: {} · Als Zweiter: {} · Partien dauern im Schnitt {:.0} Züge",
            ratio(r.on_play.0, r.on_play.1),
            ratio(r.on_draw.0, r.on_draw.1),
            r.avg_turns
        ));
        ui.add_space(6.0);

        // --- Größte Baustellen ---
        ui.heading("Was du als Nächstes verbessern kannst");
        let top: Vec<&games::Category> = r
            .categories
            .iter()
            .filter(|c| !c.items.is_empty())
            .take(3)
            .collect();
        if top.is_empty() {
            ui.label(
                RichText::new("Keine typischen Fehler gefunden. Die App erkennt allerdings nur einfache Muster.")
                    .color(GREEN),
            );
        }
        for (n, c) in top.iter().enumerate() {
            let in_games: HashSet<usize> = c.items.iter().map(|f| f.game).collect();
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("{}. {}", n + 1, c.title))
                        .strong()
                        .color(YELLOW),
                );
                ui.label(format!(
                    "– {}× in {} von {} Partien",
                    c.items.len(),
                    in_games.len(),
                    r.games
                ));
            });
            ui.label(c.advice);
            ui.add_space(4.0);
        }
        if r.good_keeps.1 + r.bad_keeps.1 > 0 {
            ui.label(
                RichText::new(format!(
                    "Starthand mit 2–4 Ländern: {} · andere Starthände: {}",
                    ratio(r.good_keeps.0, r.good_keeps.1),
                    ratio(r.bad_keeps.0, r.bad_keeps.1)
                ))
                .weak(),
            );
        }
        ui.add_space(8.0);

        // --- Alle Auffälligkeiten ---
        ui.strong("Alle Auffälligkeiten");
        ui.label(
            RichText::new(format!(
                "Zug-Auswertung über {} eigene Züge aus vollständig mitgeschriebenen Partien. Ein Klick auf einen Eintrag öffnet die Partie.",
                r.my_turns
            ))
            .weak(),
        );
        for c in &r.categories {
            let color = if c.items.is_empty() { GREEN } else { YELLOW };
            egui::CollapsingHeader::new(
                RichText::new(format!("{}: {}×", c.title, c.items.len())).color(color),
            )
            .id_salt(c.title)
            .show(ui, |ui| {
                ui.label(RichText::new(c.advice).weak());
                for f in &c.items {
                    let when = if f.turn == 0 {
                        "Starthand".to_string()
                    } else {
                        format!("Zug {}", f.turn)
                    };
                    if ui
                        .link(format!("Partie #{}, {when}: {}", f.game, f.text))
                        .clicked()
                    {
                        clicked = Some(f.game - 1);
                    }
                }
            });
        }
        if r.discards > 0 {
            ui.label(format!(
                "Abgeworfene Karten: {} (z. B. weil die Hand am Zugende zu voll war)",
                r.discards
            ));
        }
        egui::CollapsingHeader::new(format!(
            "Was dir am meisten Schaden gemacht hat ({})",
            r.damage_sources.len()
        ))
        .id_salt("damage_sources")
        .show(ui, |ui| {
            ui.label(
                RichText::new(
                    "Gegen diese Karten lohnt es sich, Removal aufzuheben oder zu blocken.",
                )
                .weak(),
            );
            for (name, dmg) in &r.damage_sources {
                let resp = ui.label(format!("{name} – {dmg} Schaden"));
                self.card_hover(resp, name, None);
            }
        });
        egui::CollapsingHeader::new(format!(
            "Häufigste Karten deiner Gegner ({})",
            r.opp_cards.len()
        ))
        .id_salt("opp_cards")
        .show(ui, |ui| {
            for (name, n) in &r.opp_cards {
                let resp = ui.label(format!("{name} – in {n} Partie(n)"));
                self.card_hover(resp, name, None);
            }
        });
        ui.add_space(6.0);
        ui.separator();

        // --- Gegner ---
        egui::CollapsingHeader::new(RichText::new("Gegen wen du gewinnst und verlierst").strong())
            .id_salt("opponents")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Das gegnerische Deck erkennt die App an den Karten, die du gesehen hast: passend zu einem geladenen Turnier-Archetyp („erkannt“), sonst nach Farben.",
                    )
                    .weak(),
                );
                egui::Grid::new("opp_grid").striped(true).num_columns(4).show(ui, |ui| {
                    for t in ["Gegner", "Bilanz", "Ø Züge", "Hat dir den meisten Schaden gemacht"] {
                        ui.strong(t);
                    }
                    ui.end_row();
                    for o in opp_groups {
                        ui.label(&o.name);
                        let color = if o.wins >= o.losses() { GREEN } else { RED };
                        ui.label(RichText::new(format!("{} Siege, {} Niederlagen", o.wins, o.losses())).color(color));
                        ui.label(format!("{:.0}", o.avg_turns));
                        let threats: Vec<String> =
                            o.threats.iter().map(|(n, d)| format!("{n} ({d})")).collect();
                        ui.label(if threats.is_empty() { "–".into() } else { threats.join(", ") });
                        ui.end_row();
                    }
                });
            });
        ui.add_space(6.0);
        clicked
    }
}
