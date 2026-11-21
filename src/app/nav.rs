//! Navigation: the game mode at the top left frames everything; below it
//! three areas (Decks, My games, Improve deck); the Decks area has sub tabs
//! per source of decks. Only what fits the game mode is shown.

use super::*;

/// Main area of the interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Area {
    /// Decks from tournaments, events, the collection or an import.
    Decks,
    Games,
    Improve,
}

impl Area {
    const ALL: [Area; 3] = [Area::Decks, Area::Games, Area::Improve];

    /// The pages of the area in display order.
    fn tabs(self) -> &'static [Tab] {
        match self {
            Area::Decks => &[Tab::Meta, Tab::Events, Tab::Built, Tab::Paste],
            Area::Games => &[Tab::Games],
            Area::Improve => &[Tab::DeckCheck],
        }
    }

    /// First page of the area that fits the game mode.
    pub(super) fn first_tab(self, mode: GameFormat) -> Option<Tab> {
        self.tabs().iter().copied().find(|t| t.fits(mode))
    }

    fn label(self) -> String {
        match self {
            Area::Decks => tr!("nav.decks"),
            Area::Games => tr!("chrome.my_games"),
            Area::Improve => tr!("chrome.improve_deck"),
        }
    }

    fn hint(self) -> String {
        match self {
            Area::Decks => tr!("nav.decks_hint"),
            Area::Games => tr!("chrome.record_tips_games_played"),
            Area::Improve => tr!("chrome.which_cards_deck_should"),
        }
    }
}

impl Tab {
    pub(super) fn area(self) -> Area {
        match self {
            Tab::Meta | Tab::Events | Tab::Built | Tab::Paste => Area::Decks,
            Tab::Games => Area::Games,
            Tab::DeckCheck => Area::Improve,
        }
    }

    /// Label and hint of a sub tab in the Decks area.
    fn sub_label(self) -> (String, String) {
        match self {
            Tab::Meta => (
                tr!("chrome.tournament_decks"),
                tr!("chrome.current_tournament_decks_what"),
            ),
            Tab::Events => (
                tr!("chrome.official_events"),
                tr!("chrome.decklists_magic_gg_championships"),
            ),
            Tab::Built => (
                tr!("chrome.my_collection"),
                tr!("chrome.decks_made_only_cards"),
            ),
            Tab::Paste => (
                tr!("chrome.own_list"),
                tr!("chrome.paste_decklist_check_against"),
            ),
            Tab::Games | Tab::DeckCheck => (String::new(), String::new()),
        }
    }
}

impl App {
    /// Game mode switch, highlighted: everything below applies to it.
    pub(super) fn mode_switch(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.settings.mode;
        egui::Frame::none()
            .fill(theme::accent_fill(ui.ctx()))
            .rounding(theme::NAV_ROUNDING)
            .inner_margin(theme::NAV_PADDING)
            .show(ui, |ui| {
                ui.label(RichText::new(tr!("chrome.format")).strong());
                egui::ComboBox::from_id_salt("format")
                    .selected_text(RichText::new(mode.label()).strong())
                    .show_ui(ui, |ui| {
                        for m in GameFormat::SELECTABLE {
                            ui.selectable_value(&mut mode, m, m.label());
                        }
                    })
                    .response
                    .on_hover_text(tr!("chrome.format_tournament_decks_legality"));
            });
        if mode != self.settings.mode {
            self.set_mode(mode);
        }
    }

    /// The three areas; an area opens its last (or first fitting) page.
    pub(super) fn area_bar(&mut self, ui: &mut egui::Ui) {
        let mode = self.settings.mode;
        for area in Area::ALL {
            let Some(first) = area.first_tab(mode) else {
                continue;
            };
            let text = RichText::new(area.label()).size(theme::AREA_TEXT);
            let open = self.tab.area() == area;
            if ui
                .add(egui::SelectableLabel::new(open, text))
                .on_hover_text(area.hint())
                .clicked()
                && !open
            {
                self.tab = match area {
                    Area::Decks if self.deck_tab.fits(mode) => self.deck_tab,
                    _ => first,
                };
            }
        }
    }

    /// Sub tabs of the Decks area (nothing in the other areas).
    pub(super) fn sub_bar(&mut self, ui: &mut egui::Ui) {
        let mode = self.settings.mode;
        let area = self.tab.area();
        if area != Area::Decks {
            return;
        }
        for tab in area.tabs().iter().copied().filter(|t| t.fits(mode)) {
            let (label, hint) = tab.sub_label();
            if ui
                .add(egui::SelectableLabel::new(self.tab == tab, label))
                .on_hover_text(hint)
                .clicked()
            {
                self.tab = tab;
            }
        }
        self.deck_tab = self.tab;
    }
}
