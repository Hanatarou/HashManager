//! rev27: what the audit of rev26 found, each with the case that showed it.
use hashcore::i18n::{self, LANGS};
use hashcore::numfmt::{self, Decimal};
use hashcore::ui::{Bar, Tab};

/// The status bar keeps what happened, so the numbers and the words in it follow the decimal separator and the language.
/// One test, because both choices are global to the process.
#[test]
fn the_status_bar_follows_the_decimal_separator_and_the_language() {
    let done = Bar::Done { tab: Tab::File, secs: 1.5, bytes: 3 << 20, algos: 2 };
    i18n::set_language("en-US");
    numfmt::set(Decimal::Point);
    let a = done.text();
    assert!(a.contains("1.5") && a.contains("3.00 MiB"), "{a}");
    numfmt::set(Decimal::Comma);
    let b = done.text();
    assert!(b.contains("1,5") && b.contains("3,00 MiB"), "{b}");
    i18n::set_language("pt-BR");
    assert_eq!(Bar::Ready.text(), i18n::tr("status.ready"));
    assert_ne!(Bar::Ready.text(), "Ready");
    assert_eq!(Bar::Cancelled.text(), i18n::tr("status.cancelled"));
    assert_eq!(Bar::Error.text(), i18n::tr("status.error"));
    // a job that read nothing says only the time
    assert!(!Bar::Done { tab: Tab::Duplicates, secs: 2.0, bytes: 0, algos: 0 }.text().contains("MiB"));
    numfmt::set(Decimal::Point);
    i18n::set_language("en-US");
}

#[test]
fn the_help_of_every_language_names_the_decimal_separator_menu() {
    for (i, l) in LANGS.iter().enumerate() {
        let t = i18n::table_of(i);
        let how = t["how.body"];
        let entry = format!("{} > {}", t["menu.file"], t["opt.decimal"]);
        assert!(how.contains(&entry), "{} does not say `{entry}` in how.body", l.code);
        assert!(how.contains(t["opt.decimal.point"]) && how.contains(t["opt.decimal.comma"]), "{}", l.code);
    }
}
