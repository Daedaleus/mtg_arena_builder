use super::*;

fn summary(missing: u32, need: [u32; 4], unavailable: u32) -> Summary {
    Summary {
        total: 60,
        missing,
        need,
        unavailable,
    }
}

#[test]
fn cost_ordering() {
    let wc = [10, 10, 2, 0];
    let cheap = summary(2, [2, 0, 0, 0], 0);
    let rare = summary(2, [0, 0, 2, 0], 0);
    let mythic = summary(1, [0, 0, 0, 1], 0);
    let gone = summary(1, [0, 0, 0, 0], 1);
    let mut list = [&gone, &mythic, &rare, &cheap];
    list.sort_by_key(|s| cost_key(s, wc));
    assert_eq!(list, [&cheap, &rare, &mythic, &gone]);
}

#[test]
fn fit_texts() {
    let wc = [4, 4, 1, 0];
    assert_eq!(fit(&summary(0, [0; 4], 0), false, wc).0, Fit::Unknown);
    assert_eq!(
        fit(&summary(0, [0; 4], 0), true, wc),
        (Fit::Complete, "komplett".into())
    );
    assert_eq!(
        fit(&summary(3, [2, 1, 0, 0], 0), true, wc),
        (Fit::Craftable, "nur 3 C/U".into())
    );
    assert_eq!(
        fit(&summary(3, [0, 0, 2, 1], 1), true, wc),
        (Fit::TooExpensive, "2 R / 1 M (+1 n. v.)".into())
    );
}

#[test]
fn tooltip_mentions_everything() {
    let t = cost_tooltip(&summary(6, [1, 2, 3, 0], 2));
    assert!(t.starts_with("90 % vorhanden (54 von 60 Karten)"), "{t}");
    assert!(t.contains("1 Common, 2 Uncommon, 3 Rare, 0 Mythic"));
    assert!(t.contains("2 Karten gibt es in Arena nicht"));
    assert!(!cost_tooltip(&summary(0, [0; 4], 0)).contains("gibt es"));
}

#[test]
fn labels() {
    assert_eq!(event_label("Ladder"), "Rangliste");
    assert_eq!(event_label("Traditional_Ladder"), "Rangliste Bo3");
    assert_eq!(event_label("Play"), "Spielen");
    assert_eq!(event_label("Traditional_Play"), "Spielen Bo3");
    assert_eq!(event_label("DualColorPrecons"), "Starterdecks");
    assert_eq!(event_label(""), "?");
    assert_eq!(
        event_label("WelcomeDeckDuels_TRK_20261110"),
        "Willkommens-Duelle"
    );
    assert_eq!(event_label("ColorChallenge_Red"), "Farb-Challenge");
    assert_eq!(event_label("Quick_Draft"), "Quick Draft");
    assert_eq!(ratio(0, 0), "–");
    assert_eq!(ratio(3, 5), "3 von 5 gewonnen");
}

#[test]
fn relative_times() {
    let now = 1_000_000_000;
    assert_eq!(ago_at(now, now), "gerade eben");
    assert_eq!(ago_at(now, now + 5_000), "gerade eben", "Zukunft");
    assert_eq!(ago_at(now, now - 10 * 60_000), "vor 10 Min.");
    assert_eq!(ago_at(now, now - 3 * 3_600_000), "vor 3 Std.");
    assert_eq!(ago_at(now, now - 4 * 86_400_000), "vor 4 Tagen");
    assert!(!ago(0).is_empty());
}

#[test]
fn pages() {
    assert_eq!(page_range(0, 0, 10), (0, 0, 1, 0));
    assert_eq!(page_range(25, 0, 10), (0, 10, 3, 0));
    assert_eq!(page_range(25, 2, 10), (20, 25, 3, 2));
    // A page that is too large (e.g. after filtering) is limited to the last one.
    assert_eq!(page_range(25, 9, 10), (20, 25, 3, 2));
    assert_eq!(page_range(10, 1, 10), (0, 10, 1, 0));
    assert_eq!(page_range(3, 0, 0), (0, 1, 3, 0), "at least 1 per page");
}
