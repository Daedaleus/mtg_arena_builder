//! Kartenbaukasten und eine kleine Standard-Kartendatenbank.

use crate::carddb::{
    Card, CardDb, ROLE_COUNTER, ROLE_DRAW, ROLE_REMOVAL, ROLE_WIPE, Rarity, TYPE_CREATURE,
    TYPE_INSTANT, TYPE_LAND, TYPE_SORCERY,
};

/// Bitmaske der Farben, die in Manakosten wie `{1}{U}{U}` vorkommen.
fn colors_of(cost: &str) -> u8 {
    "WUBRG"
        .chars()
        .enumerate()
        .filter(|(_, c)| cost.contains(&format!("{{{c}}}")))
        .fold(0, |m, (i, _)| m | (1 << i))
}

/// Baukasten für eine Karte.
pub struct CardBuilder(Card);

pub fn card(grp: u32, name: &str) -> CardBuilder {
    CardBuilder(Card {
        grp_id: grp,
        name: name.into(),
        rarity: Rarity::Common,
        set: "TST".into(),
        collector: grp.to_string(),
        is_primary: true,
        regular: true,
        ..Default::default()
    })
}

impl CardBuilder {
    pub fn cost(mut self, cost: &str) -> Self {
        self.0.mana_cost = cost.into();
        let (generic, colored) = self.0.pips();
        self.0.cmc = generic + colored.iter().sum::<u32>();
        self.0.colors |= colors_of(cost);
        self
    }
    pub fn creature(mut self, power: i32, toughness: i32) -> Self {
        self.0.types.push(TYPE_CREATURE);
        self.0.power = power.to_string();
        self.0.toughness = toughness.to_string();
        self
    }
    pub fn instant(mut self) -> Self {
        self.0.types.push(TYPE_INSTANT);
        self
    }
    pub fn sorcery(mut self) -> Self {
        self.0.types.push(TYPE_SORCERY);
        self
    }
    /// Land, das die Farben aus `colors` (z. B. „UG“) liefert.
    pub fn land(mut self, colors: &str) -> Self {
        self.0.types.push(TYPE_LAND);
        let symbols: String = colors.chars().map(|c| format!("{{{c}}}")).collect();
        self.0.colors = colors_of(&symbols);
        self
    }
    pub fn basic(mut self) -> Self {
        self.0.basic = true;
        self.0.rarity = Rarity::BasicLand;
        self
    }
    pub fn rarity(mut self, r: u8) -> Self {
        self.0.rarity = Rarity::from_arena(r);
        self
    }
    pub fn removal(mut self) -> Self {
        self.0.roles |= ROLE_REMOVAL;
        self
    }
    pub fn counter(mut self) -> Self {
        self.0.roles |= ROLE_COUNTER;
        self
    }
    pub fn draw(mut self) -> Self {
        self.0.roles |= ROLE_DRAW;
        self
    }
    pub fn wipe(mut self) -> Self {
        self.0.roles |= ROLE_WIPE | ROLE_REMOVAL;
        self
    }
    pub fn burn(mut self, n: u32) -> Self {
        self.0.burn = n;
        self
    }
    pub fn tapped(mut self) -> Self {
        self.0.enters_tapped = true;
        self
    }
    /// Manafähigkeit eines Nichtlands, z. B. „G“ (leer = farblos).
    pub fn mana(mut self, colors: &str) -> Self {
        let symbols: String = colors.chars().map(|c| format!("{{{c}}}")).collect();
        self.0.mana = Some(colors_of(&symbols));
        self
    }
    pub fn abilities(mut self, ids: &[u32]) -> Self {
        self.0.abilities = ids.to_vec();
        self
    }
    pub fn subtypes(mut self, ids: &[u32]) -> Self {
        self.0.subtypes = ids.to_vec();
        self
    }
    pub fn set(mut self, set: &str, collector: &str) -> Self {
        self.0.set = set.into();
        self.0.collector = collector.into();
        self
    }
    pub fn de(mut self, name: &str) -> Self {
        self.0.name_de = Some(name.into());
        self
    }
    pub fn build(self) -> Card {
        self.0
    }
}

// grpIds der Standard-Testkarten
pub const PLAINS: u32 = 1;
pub const ISLAND: u32 = 2;
pub const SWAMP: u32 = 3;
pub const MOUNTAIN: u32 = 4;
pub const FOREST: u32 = 5;
pub const DUAL_UG: u32 = 6;
pub const TAPLAND_UR: u32 = 7;
pub const BEAR: u32 = 10;
pub const OGRE: u32 = 11;
pub const ELF: u32 = 12;
pub const GIANT: u32 = 13;
pub const SPIRIT: u32 = 14;
pub const SHOCK: u32 = 20;
pub const MURDER: u32 = 21;
pub const COUNTER: u32 = 22;
pub const DIVINATION: u32 = 23;
pub const WRATH: u32 = 24;
pub const SPIKE: u32 = 25;
pub const GROWTH: u32 = 26;
pub const DRAGON: u32 = 30;
pub const ANGEL: u32 = 31;

/// Kleine, aber vielseitige Kartenauswahl für Tests.
pub fn std_cards() -> Vec<Card> {
    vec![
        card(PLAINS, "Plains").land("W").basic().build(),
        card(ISLAND, "Island").land("U").basic().build(),
        card(SWAMP, "Swamp").land("B").basic().build(),
        card(MOUNTAIN, "Mountain").land("R").basic().build(),
        card(FOREST, "Forest").land("G").basic().build(),
        card(DUAL_UG, "Hedge Maze").land("UG").rarity(3).build(),
        card(TAPLAND_UR, "Riverglide Path")
            .land("UR")
            .tapped()
            .build(),
        card(BEAR, "Grizzly Bears")
            .cost("{1}{G}")
            .creature(2, 2)
            .build(),
        card(OGRE, "Hill Ogre")
            .cost("{2}{R}")
            .creature(3, 3)
            .build(),
        card(ELF, "Llanowar Elves")
            .cost("{G}")
            .creature(1, 1)
            .mana("G")
            .build(),
        card(GIANT, "Stone Giant")
            .cost("{4}{G}{G}")
            .creature(6, 6)
            .rarity(3)
            .build(),
        card(SPIRIT, "Storm Spirit")
            .cost("{1}{U}{U}")
            .creature(2, 3)
            .abilities(&[8])
            .build(),
        card(SHOCK, "Shock")
            .cost("{R}")
            .instant()
            .removal()
            .burn(2)
            .build(),
        card(MURDER, "Murder")
            .cost("{1}{B}{B}")
            .instant()
            .removal()
            .build(),
        card(COUNTER, "Cancel")
            .cost("{1}{U}{U}")
            .instant()
            .counter()
            .build(),
        card(DIVINATION, "Divination")
            .cost("{2}{U}")
            .sorcery()
            .draw()
            .build(),
        card(WRATH, "Day of Judgment")
            .cost("{2}{W}{W}")
            .sorcery()
            .wipe()
            .rarity(4)
            .build(),
        card(SPIKE, "Lava Spike")
            .cost("{R}")
            .sorcery()
            .burn(3)
            .removal()
            .build(),
        card(GROWTH, "Giant Growth").cost("{G}").instant().build(),
        card(DRAGON, "Shivan Dragon")
            .cost("{4}{R}{R}")
            .creature(5, 5)
            .abilities(&[8])
            .rarity(5)
            .build(),
        card(ANGEL, "Serra Angel")
            .cost("{3}{W}{W}")
            .creature(4, 4)
            .abilities(&[8])
            .rarity(4)
            .build(),
    ]
}

pub fn std_db() -> CardDb {
    CardDb::from_cards(std_cards(), "test")
}
