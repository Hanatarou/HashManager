//! Text that runs from right to left (Arabic, Urdu), or that has some in it (a file called `تقرير.pdf` in an English window).
//! egui shapes each piece of text correctly (the letters join) but does not know about the Unicode bidirectional algorithm,
//! so a sentence that mixes Arabic with Latin words or numbers (`SHA-256`, `12.5 MB/s`, a path) would come out in the wrong
//! order. Here the text is split into runs of one direction (`unicode-bidi`), the runs are put in visual order, and egui draws
//! them left to right, each shaped on its own.
//!
//! The base direction of a text is the one of the interface, not the one of its first letter: an Arabic window shows
//! `Zstandard (المستوى 3)` from the right, an English window shows `تقرير.pdf` from the left (what a browser does with
//! `dir=rtl` and `dir=ltr`).
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Align, Color32, FontId, Label, Layout, TextWrapMode, Ui, WidgetText};

/// A piece of the line, in visual order (left to right), as egui has to be given it so that it draws what the Unicode
/// algorithm says.
#[derive(Debug, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    /// Letters that egui itself shapes from right to left (Arabic and the like): the text is then given in logical order,
    /// and egui puts the first letter at the right. Everything else is drawn from left to right as given.
    pub rtl_shaped: bool,
}

/// The same bracket seen from the other side (UAX #9, rule L4): in a right-to-left run `(` is drawn as `)`.
fn mirrored(c: char) -> char {
    match c {
        '(' => ')', ')' => '(', '[' => ']', ']' => '[', '{' => '}', '}' => '{', '<' => '>', '>' => '<',
        '«' => '»', '»' => '«', '‹' => '›', '›' => '‹', '⟨' => '⟩', '⟩' => '⟨', '「' => '」', '」' => '「',
        c => c,
    }
}

/// Does the text have anything that is drawn from right to left? Without it the order of the characters is the one they
/// have, whatever the base direction is, and the text needs no work (the usual case in a left-to-right window).
pub fn has_rtl(text: &str) -> bool {
    use unicode_bidi::BidiClass::{AL, AN, R, RLE, RLI, RLO};
    !text.is_ascii() && text.chars().any(|c| matches!(unicode_bidi::bidi_class(c), R | AL | AN | RLE | RLO | RLI))
}

/// Is this character one that egui shapes from right to left on its own (the direction of its script)? A mark follows the
/// letter it belongs to.
fn rtl_script(c: char, level_is_rtl: bool) -> bool {
    use unicode_bidi::BidiClass::{AL, AN, NSM, R};
    match unicode_bidi::bidi_class(c) {
        AL | R | AN => true,
        NSM => level_is_rtl,
        _ => false,
    }
}

/// One kind of line break (`\r\n` is one of them, not two paragraphs), and none at the very end: a label does not draw it.
fn one_kind_of_break(text: &str) -> std::borrow::Cow<'_, str> {
    let text = text.trim_end_matches(|c| unicode_bidi::bidi_class(c) == unicode_bidi::BidiClass::B);
    if text.contains('\r') { text.replace("\r\n", "\n").replace('\r', "\n").into() } else { text.into() }
}

/// The text as pieces in visual order, each paragraph (a line break ends one) on its own line. `rtl_base` is the base
/// direction of the paragraphs: that of the interface language. The order of the characters is the one of UAX #9 (rule L2): the
/// words of a right-to-left sentence run from right to left, a Latin word or a number inside it keeps its own order, and the
/// signs around them (spaces, brackets, full stops) are placed and mirrored by the same rules. Done here, character by
/// character, because egui lays a piece out in one direction only, and splits it again by font: an unmirrored `(` that egui
/// draws in the Latin font would otherwise stay on the wrong side of the word it encloses.
pub fn pieces(text: &str, rtl_base: bool) -> Vec<Piece> {
    use unicode_bidi::{BidiClass::B, BidiInfo, Level};
    let mut out: Vec<Piece> = Vec::new();
    let text = one_kind_of_break(text);
    let text = &*text;
    let info = BidiInfo::new(text, Some(if rtl_base { Level::rtl() } else { Level::ltr() }));
    for (n, para) in info.paragraphs.iter().enumerate() {
        if n > 0 { out.push(Piece { text: "\n".to_string(), rtl_shaped: false }); }
        let chars: Vec<char> = text[para.range.clone()].trim_end_matches(|c| unicode_bidi::bidi_class(c) == B).chars().collect();
        // The levels come for the whole text, one per character: this paragraph's are the ones from where it starts.
        let first = text[..para.range.start].chars().count();
        let levels = info.reordered_levels_per_char(para, para.range.clone())[first..first + chars.len()].to_vec();
        let visual = BidiInfo::reorder_visual(&levels);
        let mut run: Vec<char> = Vec::new();
        let mut run_rtl = false;
        let flush = |run: &mut Vec<char>, rtl: bool, out: &mut Vec<Piece>| {
            if run.is_empty() { return; }
            // Letters of a right-to-left script go in logical order, which is the visual order read backwards.
            if rtl { run.reverse(); }
            out.push(Piece { text: run.drain(..).collect(), rtl_shaped: rtl });
        };
        for i in visual {
            let (c, level) = (chars[i], levels[i]);
            let rtl_level = level.is_rtl();
            let shaped_rtl = rtl_script(c, rtl_level);
            if shaped_rtl != run_rtl { flush(&mut run, run_rtl, &mut out); run_rtl = shaped_rtl; }
            run.push(if !shaped_rtl && rtl_level { mirrored(c) } else { c });
        }
        flush(&mut run, run_rtl, &mut out);
    }
    out
}

/// The text as a layout job with its pieces in visual order. `color` is `Color32::PLACEHOLDER` to take the color of the widget.
pub fn job(text: &str, font: FontId, color: Color32, rtl_base: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    for piece in pieces(text, rtl_base) {
        job.append(&piece.text, 0.0, TextFormat { font_id: font.clone(), color, ..Default::default() });
    }
    job
}

pub fn widget(text: &str, size: f32, color: Color32, rtl_base: bool) -> WidgetText {
    WidgetText::LayoutJob(std::sync::Arc::new(job(text, FontId::proportional(size), color, rtl_base)))
}

/// The text for a label that wraps. egui breaks a line where the room ends, in the order the pieces are given, which is the
/// visual one: the first row would hold the end of an Arabic sentence and the second its beginning. Here the lines are
/// broken first, in reading order, by the room the label has, and every line is put in visual order on its own. A text that
/// fits, and a label that does not wrap (in a row of buttons, in a table), are given as [`widget`] gives them.
pub fn fit(ui: &Ui, text: &str, size: f32, color: Color32, rtl_base: bool) -> WidgetText {
    let font = FontId::proportional(size);
    let width = ui.available_width();
    let natural = ui.painter().layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE).size().x;
    if ui.wrap_mode() != TextWrapMode::Wrap || natural <= width { return widget(text, size, color, rtl_base); }
    let format = TextFormat { font_id: font, color, ..Default::default() };
    let mut job = LayoutJob::default();
    for (n, paragraph) in one_kind_of_break(text).split('\n').enumerate() {
        if n > 0 { job.append("\n", 0.0, format.clone()); }
        for (m, line) in wrap(ui, paragraph, size, width).iter().enumerate() {
            if m > 0 { job.append("\n", 0.0, format.clone()); }
            for piece in pieces(line, rtl_base) { job.append(&piece.text, 0.0, format.clone()); }
        }
    }
    // Already broken into rows: as a galley egui draws it as it is and does not break it again.
    WidgetText::Galley(ui.ctx().fonts_mut(|f| f.layout_job(job)))
}

/// Split a paragraph into lines no wider than `width`, breaking at spaces. The words are in logical order here;
/// every line is turned into visual order on its own afterwards, which is what a reader expects.
pub fn wrap(ui: &Ui, paragraph: &str, size: f32, width: f32) -> Vec<String> {
    let font = FontId::proportional(size);
    let measure = |s: &str| ui.painter().layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE).size().x;
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in paragraph.split(' ') {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && measure(&candidate) > width {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = candidate;
        }
        // A word wider than a line (a path has no spaces) is cut where the room ends, as egui cuts it. Each cut looks only at the
        // line it makes, so a very long word costs no more per character than a short one.
        if measure(&line) > width {
            let mut rest = line.as_str();
            while let Some(cut) = cut_point(rest, |s| measure(s) <= width) {
                lines.push(rest[..cut].to_string());
                rest = &rest[cut..];
            }
            line = rest.to_string();
        }
    }
    lines.push(line);
    lines
}

/// Where to cut `s` so that the first part still `fits` (at least one character, so that every cut makes progress); None when
/// all of it fits or there is only one character. A mark or a joiner stays with the letter it belongs to. The size of the first
/// part is looked for by doubling and then halving, so only text about as long as one line is ever measured.
fn cut_point(s: &str, fits: impl Fn(&str) -> bool) -> Option<usize> {
    // the byte where the first `k` characters end (the end of the text when it has fewer)
    let at = |k: usize| s.char_indices().nth(k).map_or(s.len(), |(i, _)| i);
    // `lo` characters fit (one at least); `hi` do not, or are more than the text has
    let (mut lo, mut hi) = (1usize, 2usize);
    while at(hi) < s.len() && fits(&s[..at(hi)]) { lo = hi; hi *= 2; }
    if at(hi) == s.len() && fits(s) { return None; }
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if fits(&s[..at(mid)]) { lo = mid } else { hi = mid }
    }
    let attached = |k: usize| {
        let c = s[at(k)..].chars().next().unwrap_or(' ');
        c == '\u{200d}' || c == '\u{200c}' || unicode_bidi::bidi_class(c) == unicode_bidi::BidiClass::NSM
    };
    let joined = |k: usize| s[..at(k)].ends_with('\u{200d}');
    while lo > 1 && (attached(lo) || joined(lo)) { lo -= 1; }
    Some(at(lo)).filter(|&cut| cut < s.len())
}

/// A block of text with line breaks (`\n`): wrapped to the width of `ui`, right-aligned when `rtl`.
pub fn paragraphs(ui: &mut Ui, text: &str, rtl: bool) {
    if !rtl {
        ui.add(Label::new(text).selectable(true));
        return;
    }
    let width = (ui.available_width() - 24.0).max(120.0);
    ui.with_layout(Layout::top_down(Align::Max), |ui| {
        for para in text.split('\n') {
            if para.trim().is_empty() {
                ui.add_space(8.0);
                continue;
            }
            for line in wrap(ui, para, 14.0, width) {
                ui.add(Label::new(widget(&line, 14.0, Color32::PLACEHOLDER, true)).selectable(true));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str, rtl_shaped: bool) -> Piece { Piece { text: text.to_string(), rtl_shaped } }

    #[test]
    fn left_to_right_text_is_left_as_it_is() {
        assert_eq!(pieces("Hello (world) 12.5 MB/s.", false), vec![p("Hello (world) 12.5 MB/s.", false)]);
        assert_eq!(pieces("", false), vec![]);
        assert_eq!(pieces("", true), vec![]);
    }

    #[test]
    fn a_latin_word_in_brackets_inside_arabic_is_enclosed_by_the_brackets() {
        // هذا (abc) ثم  ->  left to right: ثم, space, (abc), space, هذا
        assert_eq!(pieces("هذا (abc) ثم", true), vec![p("ثم", true), p(" (abc) ", false), p("هذا", true)]);
    }

    #[test]
    fn the_full_stop_is_at_the_end_of_the_sentence_which_is_its_left() {
        // نهاية. -> the full stop is to the left of the word
        assert_eq!(pieces("نهاية.", true), vec![p(".", false), p("نهاية", true)]);
        // A path inside the sentence keeps its own order and its brackets enclose it.
        assert_eq!(pieces("في (E:\\Tools).", true), vec![p(".(E:\\Tools) ", false), p("في", true)]);
    }

    #[test]
    fn the_words_of_a_right_to_left_sentence_come_in_reverse_order() {
        assert_eq!(pieces("لا يمكن", true), vec![p("يمكن", true), p(" ", false), p("لا", true)]);
    }

    #[test]
    fn a_new_line_at_the_end_is_not_drawn() {
        assert_eq!(pieces("abc\n", false), vec![p("abc", false)]);
        assert_eq!(pieces("abc\r\n\r\n", true), vec![p("abc", false)]);
    }

    #[test]
    fn every_paragraph_stays_on_its_own_line() {
        // Without the line break the two lines of a message ran into one.
        let text = |v: Vec<Piece>| v.into_iter().map(|x| x.text).collect::<String>();
        assert_eq!(pieces("abc\nيمكن", true), vec![p("abc", false), p("\n", false), p("يمكن", true)]);
        // a later paragraph has the levels of its own text, not those of the first one
        assert_eq!(pieces("abc\nيمكن", false), vec![p("abc", false), p("\n", false), p("يمكن", true)]);
        assert_eq!(pieces("يمكن\nabc", true), vec![p("يمكن", true), p("\n", false), p("abc", false)]);
        assert_eq!(text(pieces("abc\n\ndef", false)), "abc\n\ndef");
        assert_eq!(text(pieces("abc\r\ndef", false)), "abc\ndef");
        // each line is put in visual order on its own
        assert_eq!(pieces("لا يمكن\nلا", true), vec![p("يمكن", true), p(" ", false), p("لا", true), p("\n", false), p("لا", true)]);
    }

    #[test]
    fn the_direction_of_the_window_decides_not_the_first_letter() {
        // An Arabic window: a sentence that starts with a Latin word still runs from the right.
        let ar = pieces("Zstandard (المستوى 3)", true);
        // visual order, left to right: `(3 المستوى) Zstandard`
        assert_eq!(ar, vec![p("(3 ", false), p("المستوى", true), p(") Zstandard", false)]);
        // An English window: a name in Arabic keeps the folder and the extension where a left-to-right reader expects them.
        let en = pieces("D:\\Docs\\تقرير شهري (نهائي).pdf", false);
        assert!(en.first().is_some_and(|x| x.text.starts_with("D:\\Docs\\")), "{en:?}");
        assert_eq!(en.last().map(|x| x.text.as_str()), Some(".pdf"), "{en:?}");
    }

    #[test]
    fn only_text_with_right_to_left_characters_needs_the_work() {
        assert!(!has_rtl("Hello (world) 12.5 MB/s. é 文件"));
        assert!(has_rtl("تقرير.pdf") && has_rtl("דוח") && has_rtl("a\u{200F}b") && has_rtl("\u{0661}"));
        // and for the rest the base direction changes nothing
        for t in ["Hello (world) 12.5 MB/s.", "(a) [b] <c>", "文件.txt", "é (ü)"] {
            assert_eq!(pieces(t, false), vec![p(t, false)], "{t}");
        }
    }

    #[test]
    fn no_character_is_lost_whatever_the_text_and_the_direction() {
        let alphabet: Vec<char> = "ab 1(.)[]\n\rءآبتثجحخ\u{200f}\u{202e}\u{2067}\u{2069}\u{0661}\u{05d0}\u{2029}".chars().collect();
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |m: usize| { x ^= x << 13; x ^= x >> 7; x ^= x << 17; (x % m as u64) as usize };
        for _ in 0..20_000 {
            let s: String = (0..next(24)).map(|_| alphabet[next(alphabet.len())]).collect();
            let kept = |t: &str| t.chars().filter(|c| !matches!(c, '\n' | '\r' | '\u{2029}')).count();
            for rtl in [false, true] {
                let got: usize = pieces(&s, rtl).iter().map(|q| kept(&q.text)).sum();
                assert_eq!(got, kept(&s), "{s:?} rtl={rtl}");
            }
        }
    }

    /// Runs `f` with a `Ui` of a window as wide as `width`.
    fn with_ui(f: impl FnOnce(&Ui)) {
        let ctx = eframe::egui::Context::default();
        // The fonts of the program (a bare `Context` has none: egui's own are not built in).
        crate::ui::fonts::install(&ctx, crate::i18n::current(), false);
        let raw = eframe::egui::RawInput { screen_rect: Some(eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(800.0, 600.0))), ..Default::default() };
        let _ = ctx.run_ui(raw.clone(), |_| {});
        let mut f = Some(f);
        let _ = ctx.run_ui(raw, |ui| if let Some(f) = f.take() { f(ui) });
    }

    #[test]
    fn a_word_wider_than_the_room_is_cut_where_the_room_ends() {
        with_ui(|ui| {
            let measure = |s: &str| ui.painter().layout_no_wrap(s.to_string(), FontId::proportional(14.0), Color32::WHITE).size().x;
            // A path in an Arabic window, without a single space: it used to stay in one line and run out of the window.
            let path = "/home/user/Downloads/تقارير_شهرية/نسخة_احتياطية_كاملة_للمستندات_المهمة_2026/installer-with-a-very-long-file-name.iso";
            for width in [60.0, 150.0, 300.0] {
                let lines = wrap(ui, path, 14.0, width);
                assert!(lines.len() > 1, "{width}");
                assert_eq!(lines.concat(), path, "no character is lost or added at a cut");
                for l in &lines { assert!(measure(l) <= width || l.chars().count() == 1, "{width}: {l:?} is {}", measure(l)); }
            }
            // Words that fit are still broken at the spaces, and a long one after a short one starts its own line.
            assert_eq!(wrap(ui, "ab cd", 14.0, 500.0), vec!["ab cd"]);
            let lines = wrap(ui, &format!("ab {path}"), 14.0, 200.0);
            assert_eq!(lines[0], "ab");
            assert_eq!(lines[1..].concat(), path);
            // a room of nothing makes progress, a letter at a time
            assert_eq!(wrap(ui, "abc", 14.0, 0.0), vec!["a", "b", "c"]);
        });
    }

    #[test]
    fn a_cut_never_parts_a_letter_from_its_mark() {
        with_ui(|ui| {
            // every letter has a combining mark, and the Arabic one a vowel sign
            let text = "e\u{301}".repeat(60) + &"بَ".repeat(40);
            for width in [20.0, 47.0, 90.0] {
                for l in wrap(ui, &text, 14.0, width) {
                    let first = l.chars().next().unwrap();
                    assert!(unicode_bidi::bidi_class(first) != unicode_bidi::BidiClass::NSM, "{width}: a line starts with a mark: {l:?}");
                }
            }
        });
    }

    #[test]
    fn cutting_a_long_word_measures_about_as_much_as_the_word_is_long() {
        use std::cell::Cell;
        // 5 characters of room, a word of 40 000: measuring the rest of the word at every cut would be 40 000 * 8 000 / 2 characters.
        let word = "x".repeat(40_000);
        let measured = Cell::new(0usize);
        let mut rest = word.as_str();
        let mut parts = 0;
        while let Some(cut) = cut_point(rest, |s| { measured.set(measured.get() + s.len()); s.len() <= 5 }) {
            assert_eq!(cut, 5);
            rest = &rest[cut..];
            parts += 1;
        }
        assert_eq!(parts, 7_999);
        assert_eq!(rest.len(), 5);
        assert!(measured.get() < 40 * word.len(), "{} characters measured", measured.get());
    }
}
