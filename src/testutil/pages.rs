//! Ausschnitte von magic.gg-Seiten im Originalaufbau.

/// Ausschnitt der Übersichtsseite (Aufbau wie auf magic.gg).
pub const GG_INDEX_HTML: &str = r#"<ul>
<li class="css-3pfjz"><a href="/decklists/arena-championship-13-standard-decklists-a-k" target="_self" rel="" class="css-Q01Pa"><div class="css-2es8F"><div class="css-vT_kr" style="padding-top:56.25%;"></div></div> <div class="css-1wd27"><div class="css-1JXy9">
      Arena Championship 13 Standard Decklists A-K
    </div> <div class="css-xkkQe"><span>October 24, 2026</span> <!----></div> <!----></div></a></li>
<li><a href="/decklists/champions-cup-final-yokohama-october-2026-top-16-modern-decklists" target="_self"><div>
      Champions Cup Final Yokohama Top 16 Modern Decklists
    </div> <div><span>October 20, 2026</span></div></a></li>
<li><a href="/decklists/traditional-standard-ranked-decklists-november-9-2026" target="_self"><div>
      Traditional Standard Ranked Decklists: November 9, 2026
    </div> <div><span>November 9, 2026</span></div></a></li>
<li><a href="/decklists/arena-championship-13-standard-decklists-a-k" target="_self"><div>Doppelt</div></a></li>
<li><a href="/news/mtg-arena-premier-play-policy">Keine Deckliste</a></li>
</ul>"#;

/// Ausschnitt einer Event-Seite mit zwei Decks und einem leeren Eintrag.
pub const GG_EVENT_HTML: &str = r#"<div>
<deck-list deck-title="Erika Muster" subtitle="1st Place" event-date="October 24, 2026" event-name="Arena Championship 13" format="Standard">
<main-deck>
20 Mountain
4 Shock
4 Hill Ogre
</main-deck>
<side-board>
2 Lava Spike
</side-board>
<companion-card>
</companion-card>
</deck-list>
<deck-list deck-title="Max O&#39;Neil" subtitle="" event-date="October 24, 2026" event-name="Arena Championship 13" format="Standard">
<main-deck>
24 Forest
4 Grizzly Bears
</main-deck>
<side-board>
</side-board>
</deck-list>
<deck-list deck-title="Leer" format="Standard">
<main-deck>
</main-deck>
</deck-list>
</div>"#;
