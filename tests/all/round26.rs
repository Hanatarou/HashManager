//! rev26: the decimal separator is a choice in the menu, and the languages are only languages.
use hashcore::i18n::{self, best_match, menu_order, LANGS};
use hashcore::numfmt::{self, Decimal};
use hashcore::settings;

fn code(text: &str) -> Option<&'static str> { best_match(text).map(|i| LANGS[i].code) }

#[test]
fn decimal_choice_has_a_stable_name_and_round_trips_in_the_settings() {
    for d in [Decimal::System, Decimal::Point, Decimal::Comma] {
        assert_eq!(Decimal::parse(d.name()), Some(d));
        let s = settings::Settings { decimal: d, ..Default::default() };
        assert_eq!(settings::parse(&settings::serialize(&s)).decimal, d);
    }
    assert_eq!(settings::Settings::default().decimal, Decimal::System);
    assert_eq!(Decimal::parse(" Comma "), Some(Decimal::Comma));
    assert_eq!(Decimal::parse("dot"), None);
    // an unknown value falls back to the default
    assert_eq!(settings::parse("decimal=whatever\n").decimal, Decimal::System);
}

/// One test, because the choice is global to the process.
#[test]
fn the_choice_decides_the_separator_at_once() {
    numfmt::set(Decimal::Point);
    assert_eq!(numfmt::separator(), ".");
    assert_eq!(numfmt::fixed(1.5, 2), "1.50");
    numfmt::set(Decimal::Comma);
    assert_eq!(numfmt::separator(), ",");
    assert_eq!(numfmt::fixed(1.5, 2), "1,50");
    numfmt::set(Decimal::System);
    assert_eq!(numfmt::separator(), numfmt::system());
    numfmt::set(Decimal::Point);
}

#[test]
fn spanish_and_portuguese_pick_the_variant_by_region() {
    assert_eq!(code("es"), Some("es-ES"));
    assert_eq!(code("es-ES"), Some("es-ES"));
    assert_eq!(code("es_ES.UTF-8"), Some("es-ES"));
    for l in ["es-MX", "es-AR", "es-419", "es-CO", "es-US"] { assert_eq!(code(l), Some("es-MX"), "{l}"); }
    assert_eq!(code("pt"), Some("pt-BR"));
    assert_eq!(code("pt-BR"), Some("pt-BR"));
    for l in ["pt-PT", "pt-AO", "pt-MZ"] { assert_eq!(code(l), Some("pt-PT"), "{l}"); }
    assert_eq!(code("en-GB"), Some("en-US"));
    assert_eq!(code("zh-Hans-CN"), Some("zh-CN"));
    assert_eq!(code("zh-TW"), Some("zh-CN"));
}

#[test]
fn the_menu_lists_every_language_once_in_alphabetical_order() {
    let order = menu_order();
    assert_eq!(order.len(), LANGS.len());
    let mut seen = order.to_vec();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), LANGS.len());
    let names: Vec<&str> = order.iter().map(|&i| LANGS[i].sort).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    // English stays first in LANGS: it is the fallback of every missing text
    assert_eq!(LANGS[0].code, "en-US");
    assert_eq!(i18n::DEFAULT, "en-US");
}

#[test]
fn every_name_is_language_and_country_in_the_same_pattern() {
    for l in LANGS.iter() {
        let ok = (l.name.contains(" (") && l.name.ends_with(')')) || (l.name.contains('（') && l.name.ends_with('）'));
        assert!(ok, "{}: {}", l.code, l.name);
        assert!(l.sort.contains(" (") && l.sort.ends_with(')'), "{}: {}", l.code, l.sort);
    }
    let mut names: Vec<&str> = LANGS.iter().map(|l| l.name).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), LANGS.len());
}

#[test]
fn the_new_texts_exist_in_every_language() {
    for (i, l) in LANGS.iter().enumerate() {
        let t = i18n::table_of(i);
        for k in ["opt.decimal", "opt.decimal.point", "opt.decimal.comma"] {
            assert!(t.get(k).is_some_and(|v| !v.is_empty()), "{} lacks {k}", l.code);
        }
    }
}
