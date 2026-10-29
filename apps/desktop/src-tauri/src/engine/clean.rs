//! Post-processing of raw Whisper text: strips the well-known hallucinations
//! (subtitle credits, "thanks for watching", [music] tags), collapses runaway
//! repetition and tidies whitespace around punctuation.

/// Sentences containing any of these are dropped outright — Whisper invents them
/// from YouTube subtitle data, nobody dictates them.
const JUNK_SENTENCES: &[&str] = &[
    "thanks for watching",
    "thank you for watching",
    "thanks for listening",
    "please subscribe",
    "like and subscribe",
    "subscribe to my channel",
    "subscribe to the channel",
    "don't forget to subscribe",
    "see you in the next video",
    "see you next time",
    "subtitles by",
    "subtitled by",
    "captions by",
    "transcribed by",
    "translated by",
    "amara.org",
    "சப்ஸ்கிரைப்",
    "சப்ஸ்க்ரைப்",
    "பார்த்ததற்கு நன்றி",
    "பார்த்தமைக்கு நன்றி",
    "வீடியோவை லைக்",
    "லைக் பண்ணுங்க",
    "ஷேர் பண்ணுங்க",
    "அடுத்த வீடியோவில்",
    "தமிழ் வசனம்",
];

/// When the whole transcript of a very short clip is one of these, it's noise.
const JUNK_WHOLE_SHORT: &[&str] = &["you", "thank you", "thanks", "bye", "நன்றி", "வணக்கம்", "ok", "okay", "hmm", "um"];

/// Bracketed tags like (music), [applause], (சிரிப்பு).
const TAG_WORDS: &[&str] = &[
    "music", "applause", "laughter", "laughs", "silence", "inaudible", "noise", "background", "upbeat", "cheering", "sigh", "coughs",
    "இசை", "சிரிப்பு", "கைதட்டல்", "சத்தம்", "பின்னணி",
];

pub fn clean(text: &str, voiced_ms: u64) -> String {
    let mut s = strip_tags(text);
    s = s.replace(['♪', '♫', '�'], "");
    s = drop_junk_sentences(&s);
    s = collapse_repeats(&s);
    s = collapse_substring_loops(&s);
    s = collapse_char_runs(&s);
    s = soften_dashes(&s);
    s = tidy_spaces(&s);
    let bare: String = s.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace() || is_tamil(*c)).collect::<String>().trim().to_lowercase();
    if voiced_ms < 1200 && JUNK_WHOLE_SHORT.contains(&bare.as_str()) {
        return String::new();
    }
    if bare.is_empty() {
        return String::new();
    }
    s
}

/// Whisper likes em dashes in English; Mozhi's house style writes them as commas.
fn soften_dashes(s: &str) -> String {
    s.replace(" — ", ", ").replace(" —", ",").replace("— ", ", ").replace('—', ", ").replace(" ,", ",")
}

fn is_tamil(c: char) -> bool {
    ('\u{0B80}'..='\u{0BFF}').contains(&c)
}

fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let close = match c {
            '[' => Some(']'),
            '(' => Some(')'),
            '*' => Some('*'),
            _ => None,
        };
        if let Some(close) = close {
            if let Some(len) = chars[i + 1..].iter().position(|&x| x == close) {
                let inner: String = chars[i + 1..i + 1 + len].iter().collect();
                let lower = inner.to_lowercase();
                // [..] is always a Whisper tag; (..) and *..* only when they describe sounds.
                if c == '[' || TAG_WORDS.iter().any(|w| lower.contains(w)) {
                    i += len + 2;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Splits after sentence punctuation, keeping the punctuation.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        // "Amara.org" or "5.30" is not a sentence end.
        let at_break = chars.peek().map(|n| n.is_whitespace()).unwrap_or(true);
        if matches!(c, '.' | '!' | '?' | '।' | '\n') && at_break {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn drop_junk_sentences(text: &str) -> String {
    sentences(text)
        .into_iter()
        .filter(|s| {
            let l = s.to_lowercase();
            !JUNK_SENTENCES.iter().any(|j| l.contains(j))
        })
        .collect::<Vec<_>>()
        .join("")
}

const NUMBER_WORDS: &[&str] = &[
    "zero", "oh", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "double", "triple",
    "பூஜ்ஜியம்", "ஒன்று", "ஒண்ணு", "இரண்டு", "ரெண்டு", "மூன்று", "மூணு", "நான்கு", "நாலு", "ஐந்து", "அஞ்சு", "ஆறு", "ஏழு", "எட்டு", "ஒன்பது",
];

/// "naan naan naan naan" → "naan"; "go home go home go home" → "go home".
/// Any short n-gram repeated three or more times in a row collapses to one; a phrase of
/// four or more words (up to 32) repeated even twice is a decoder loop, not speech.
fn collapse_repeats(text: &str) -> String {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    let norm = |w: &str| w.trim_matches(|c: char| c.is_ascii_punctuation() || c == '…').to_lowercase();
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for n in 1..=32usize {
            let min_reps = if n >= 4 { 2 } else { 3 };
            if words.len() < n * min_reps {
                continue;
            }
            let mut i = 0;
            while i + n * min_reps <= words.len() {
                let gram: Vec<String> = words[i..i + n].iter().map(|w| norm(w)).collect();
                let mut reps = 1;
                while i + (reps + 1) * n <= words.len() && words[i + reps * n..i + (reps + 1) * n].iter().map(|w| norm(w)).collect::<Vec<_>>() == gram {
                    reps += 1;
                }
                // Dictated numbers repeat legitimately: "nine nine nine", "9 9 9".
                let numeric = gram.iter().all(|w| w.chars().all(|c| c.is_ascii_digit()) || NUMBER_WORDS.contains(&w.as_str()));
                if !numeric && (reps >= 3 || (reps >= 2 && n >= 4)) {
                    // keep the last copy (it carries the trailing punctuation)
                    words.drain(i..i + (reps - 1) * n);
                    changed = true;
                    break 'outer;
                }
                i += 1;
            }
        }
    }
    words.join(" ")
}

const LOOP_MAX_UNIT: usize = 16;
const LOOP_MIN_REPS: usize = 4;

/// Finds a unit of 2–16 characters repeated four or more times back to back
/// ("கும்கும்கும்கும்…"). Returns (start, unit length, repetitions) in chars.
fn find_substring_loop(chars: &[char]) -> Option<(usize, usize, usize)> {
    for unit in 2..=LOOP_MAX_UNIT {
        if chars.len() < unit * LOOP_MIN_REPS {
            break;
        }
        let mut i = 0;
        while i + unit * LOOP_MIN_REPS <= chars.len() {
            let mut reps = 1;
            while i + (reps + 1) * unit <= chars.len() && chars[i + reps * unit..i + (reps + 1) * unit] == chars[i..i + unit] {
                reps += 1;
            }
            if reps >= LOOP_MIN_REPS && chars[i..i + unit].iter().any(|c| c.is_alphabetic()) {
                return Some((i, unit, reps));
            }
            i += 1;
        }
    }
    None
}

/// True when Whisper got stuck repeating itself — the decode should be retried.
pub fn has_loop(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    find_substring_loop(&chars).is_some() || collapse_repeats(text).chars().count() * 3 < text.split_whitespace().collect::<Vec<_>>().join(" ").chars().count() * 2
}

/// "நான்னைக்கும்கும்கும்கும்கும்" → "நான்னைக்கும்".
fn collapse_substring_loops(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    while let Some((start, unit, reps)) = find_substring_loop(&chars) {
        chars.drain(start + unit..start + unit * reps);
    }
    chars.into_iter().collect()
}

/// "ஆஆஆஆஆஆ" / "!!!!!!" → at most three of the same character.
fn collapse_char_runs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = None;
    let mut run = 0;
    for c in text.chars() {
        if Some(c) == last {
            run += 1;
        } else {
            run = 1;
            last = Some(c);
        }
        if run <= 3 || c.is_ascii_digit() {
            out.push(c);
        }
    }
    out
}

fn tidy_spaces(text: &str) -> String {
    let mut s = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for p in [" ,", " .", " ?", " !", " :", " ;"] {
        s = s.replace(p, &p[1..]);
    }
    s.trim().trim_start_matches(['-', '–', '—', ',', '.']).trim().to_string()
}

#[cfg(test)]
mod tests {

    #[test]
    fn em_dashes_become_commas() {
        assert_eq!(soften_dashes("Yes — that works"), "Yes, that works");
        assert_eq!(soften_dashes("Yes—that works"), "Yes, that works");
        assert_eq!(soften_dashes("நாளைக்கு — சரி"), "நாளைக்கு, சரி");
    }

    use super::*;

    #[test]
    fn removes_subtitle_hallucinations() {
        assert_eq!(clean("Thanks for watching!", 3000), "");
        assert_eq!(clean("நான் நாளைக்கு வருவேன். சேனலை சப்ஸ்கிரைப் பண்ணுங்க.", 3000), "நான் நாளைக்கு வருவேன்.");
        assert_eq!(clean("Send it now. Subtitles by the Amara.org community", 3000), "Send it now.");
    }

    #[test]
    fn removes_tags_but_keeps_real_parentheses() {
        assert_eq!(clean("[Music] hello there (applause)", 3000), "hello there");
        assert_eq!(clean("call me (after 5) please", 3000), "call me (after 5) please");
        assert_eq!(clean("(இசை) வணக்கம் எல்லாருக்கும்", 3000), "வணக்கம் எல்லாருக்கும்");
    }

    #[test]
    fn collapses_runaway_repetition() {
        assert_eq!(clean("சரி சரி சரி சரி சரி சரி", 3000), "சரி");
        assert_eq!(clean("I will go home I will go home I will go home I will go home.", 4000), "I will go home.");
        assert_eq!(clean("yes yes I agree", 3000), "yes yes I agree");
        assert_eq!(clean("ஆஆஆஆஆஆஆ", 3000), "ஆஆஆ");
        assert_eq!(
            clean("Please share the design files before the meeting. Please share the design files before the meeting.", 5000),
            "Please share the design files before the meeting."
        );
        assert_eq!(clean("I said no, no.", 3000), "I said no, no.");
        assert_eq!(
            clean("I will send the quarterly report by tomorrow evening and share the files before. I will send the quarterly report by tomorrow evening and share the files before.", 6000),
            "I will send the quarterly report by tomorrow evening and share the files before."
        );
        assert_eq!(clean("நான்னைக்கும்கும்கும்கும்கும்கும்", 3000), "நான்னைக்கும்");
        assert_eq!(clean("hahahahahaha that's funny", 3000), "ha that's funny");
        assert_eq!(clean("அம்மா அப்பா", 3000), "அம்மா அப்பா");
    }

    #[test]
    fn detects_loops() {
        assert!(has_loop("நான்னைக்கும்கும்கும்கும்கும்கும்"));
        assert!(has_loop("சரி, நாம் சரி, நாம் சரி, நாம் சரி, நாம் சரி, நாம்"));
        assert!(!has_loop("நான் நாளைக்கு வருவேன், சரியா?"));
        assert!(!has_loop("Please call me at 9 9 9 0 0."));
        assert_eq!(clean("my number is nine nine nine eight", 3000), "my number is nine nine nine eight");
        assert_eq!(clean("PIN 4 4 4 4", 3000), "PIN 4 4 4 4");
    }

    #[test]
    fn drops_lone_filler_on_short_clips_only() {
        assert_eq!(clean(" you", 400), "");
        assert_eq!(clean("Thank you.", 600), "");
        assert_eq!(clean("Thank you.", 2500), "Thank you.");
    }

    #[test]
    fn tidies_spacing() {
        assert_eq!(clean("  hello ,  world .  ", 3000), "hello, world.");
        assert_eq!(clean("- வணக்கம்", 3000), "வணக்கம்");
    }
}
