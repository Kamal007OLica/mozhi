//! Tamil script → Tanglish (the casual Latin spelling Tamil speakers actually type).
//!
//! Not a scholarly romanisation (ISO 15919) — it follows chat conventions:
//! ழ → zh, ற்ற → tr, ன்ற → ndr, ங்க → ng, ஞ்ச → nj, ச்ச → ch, word-initial ச → s,
//! க/ட/த soften to g/d/dh between vowels, long vowels doubled (aa, ee, oo).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V {
    A,
    Aa,
    I,
    Ii,
    U,
    Uu,
    E,
    Ee,
    Ai,
    O,
    Oo,
    Au,
}

#[derive(Debug, Clone, PartialEq)]
enum U {
    /// Independent vowel (உயிர்).
    Vowel(V),
    /// Consonant with its vowel; `v == None && !pulli` is the inherent "a".
    Cons { c: char, v: Option<V>, pulli: bool },
    Aytham,
    Lit(&'static str),
    Other(char),
}

/// Stand-in for the க்ஷ conjunct.
const KSH: char = '\u{E000}';

fn independent(c: char) -> Option<V> {
    Some(match c {
        'அ' => V::A,
        'ஆ' => V::Aa,
        'இ' => V::I,
        'ஈ' => V::Ii,
        'உ' => V::U,
        'ஊ' => V::Uu,
        'எ' => V::E,
        'ஏ' => V::Ee,
        'ஐ' => V::Ai,
        'ஒ' => V::O,
        'ஓ' => V::Oo,
        'ஔ' => V::Au,
        _ => return None,
    })
}

fn is_consonant(c: char) -> bool {
    matches!(
        c,
        'க' | 'ங' | 'ச' | 'ஜ' | 'ஞ' | 'ட' | 'ண' | 'த' | 'ந' | 'ன' | 'ப' | 'ம' | 'ய' | 'ர' | 'ற' | 'ல' | 'ள' | 'ழ' | 'வ' | 'ஶ' | 'ஷ' | 'ஸ' | 'ஹ'
    )
}

const PULLI: char = '\u{0BCD}';

fn parse(text: &str) -> Vec<U> {
    let cs: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(cs.len());
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        // ஸ்ரீ
        if c == 'ஸ' && cs.get(i + 1) == Some(&PULLI) && cs.get(i + 2) == Some(&'ர') && cs.get(i + 3) == Some(&'ீ') {
            out.push(U::Lit("sri"));
            i += 4;
            continue;
        }
        if let Some(v) = independent(c) {
            // ஒ + ள (au length mark) is a legacy spelling of ஔ
            if c == 'ஒ' && cs.get(i + 1) == Some(&'ௗ') {
                out.push(U::Vowel(V::Au));
                i += 2;
            } else {
                out.push(U::Vowel(v));
                i += 1;
            }
            continue;
        }
        if is_consonant(c) {
            let mut base = c;
            let mut j = i + 1;
            if c == 'க' && cs.get(j) == Some(&PULLI) && cs.get(j + 1) == Some(&'ஷ') {
                base = KSH;
                j += 2;
            }
            let (v, pulli, used) = match cs.get(j) {
                Some(&PULLI) => (None, true, 1),
                Some('ா') => (Some(V::Aa), false, 1),
                Some('ி') => (Some(V::I), false, 1),
                Some('ீ') => (Some(V::Ii), false, 1),
                Some('ு') => (Some(V::U), false, 1),
                Some('ூ') => (Some(V::Uu), false, 1),
                Some('ெ') => match cs.get(j + 1) {
                    Some('ா') => (Some(V::O), false, 2),
                    Some('ௗ') => (Some(V::Au), false, 2),
                    _ => (Some(V::E), false, 1),
                },
                Some('ே') => match cs.get(j + 1) {
                    Some('ா') => (Some(V::Oo), false, 2),
                    _ => (Some(V::Ee), false, 1),
                },
                Some('ை') => (Some(V::Ai), false, 1),
                Some('ொ') => (Some(V::O), false, 1),
                Some('ோ') => (Some(V::Oo), false, 1),
                Some('ௌ') => (Some(V::Au), false, 1),
                _ => (None, false, 0),
            };
            out.push(U::Cons { c: base, v, pulli });
            i = j + used;
            continue;
        }
        match c {
            'ஃ' => out.push(U::Aytham),
            'ௐ' => out.push(U::Lit("om")),
            '\u{0BE6}'..='\u{0BEF}' => out.push(U::Other(char::from(b'0' + (c as u32 - 0x0BE6) as u8))),
            // Stray combining marks (malformed input) are dropped rather than echoed as Tamil.
            '\u{0BBE}'..='\u{0BCD}' | '\u{0BD7}' => {}
            _ => out.push(U::Other(c)),
        }
        i += 1;
    }
    out
}

fn vowel(v: V, independent: bool) -> &'static str {
    match v {
        V::A => "a",
        V::Aa => "aa",
        V::I => "i",
        V::Ii => "ee",
        V::U => "u",
        V::Uu => "oo",
        V::E => "e",
        V::Ee if independent => "ae",
        V::Ee => "e",
        V::Ai => "ai",
        V::O | V::Oo => "o",
        V::Au => "au",
    }
}

/// Ends in a vowel sound (so a following stop softens).
fn vowelish(u: Option<&U>) -> bool {
    match u {
        Some(U::Vowel(_)) | Some(U::Lit(_)) => true,
        Some(U::Cons { pulli, .. }) => !pulli,
        _ => false,
    }
}

fn pulli_of(u: Option<&U>) -> Option<char> {
    match u {
        Some(U::Cons { c, pulli: true, .. }) => Some(*c),
        _ => None,
    }
}

fn next_cons(u: Option<&U>) -> Option<char> {
    match u {
        Some(U::Cons { c, .. }) => Some(*c),
        _ => None,
    }
}

fn consonant(c: char, pulli: bool, prev: Option<&U>, next: Option<&U>) -> &'static str {
    let after = pulli_of(prev);
    let soft = vowelish(prev);
    let initial = !matches!(prev, Some(U::Vowel(_)) | Some(U::Cons { .. }) | Some(U::Aytham) | Some(U::Lit(_)));
    let nx = next_cons(next);

    if pulli {
        return match c {
            'ங' if nx == Some('க') => "n",
            'ஞ' if matches!(nx, Some('ச') | Some('ஞ') | Some('ஜ')) => "n",
            'ச' if nx == Some('ச') => "",
            'ச' => "ch",
            'த' if nx == Some('த') => "",
            'ற' if nx == Some('ற') => "t",
            'ய' if nx == Some('ய') => "y",
            'ய' => "i",
            // A closing consonant is never softened: வீட்டு → veettu, not veedtu.
            _ => base(c, after, false, initial),
        };
    }
    base(c, after, soft, initial)
}

fn base(c: char, after: Option<char>, soft: bool, initial: bool) -> &'static str {
    match c {
        'க' => match after {
            Some('ங') => "g",
            Some(_) => "k",
            None if soft => "g",
            None => "k",
        },
        'ங' => "ng",
        'ச' => match after {
            Some('ச') => "ch",
            Some('ஞ') => "j",
            _ => "s",
        },
        'ஜ' => "j",
        'ஞ' => {
            if initial {
                "gn"
            } else {
                "nj"
            }
        }
        'ட' => match after {
            Some('ண') | Some('ன') => "d",
            Some(_) => "t",
            None if soft => "d",
            None => "t",
        },
        'ண' | 'ந' | 'ன' => "n",
        'த' => match after {
            Some('ந') | Some('ன') => "dh",
            Some(_) => "th",
            None if soft => "dh",
            None => "th",
        },
        'ப' => match after {
            Some('ம') => "b",
            _ => "p",
        },
        'ம' => "m",
        'ய' => "y",
        'ர' => "r",
        'ற' => match after {
            Some('ன') => "dr",
            _ => "r",
        },
        'ல' | 'ள' => "l",
        'ழ' => "zh",
        'வ' => "v",
        'ஶ' | 'ஷ' => "sh",
        'ஸ' => "s",
        'ஹ' => "h",
        KSH => "ksh",
        _ => "",
    }
}

/// "நான் வீட்டுக்கு போறேன்" → "naan veettukku poren". Non-Tamil text passes through.
pub fn tamil_to_tanglish(text: &str) -> String {
    let units = parse(text);
    let mut out = String::with_capacity(text.len());
    for (i, u) in units.iter().enumerate() {
        let prev = if i > 0 { units.get(i - 1) } else { None };
        let next = units.get(i + 1);
        match u {
            U::Vowel(v) => out.push_str(vowel(*v, true)),
            U::Cons { c, v, pulli } => {
                out.push_str(consonant(*c, *pulli, prev, next));
                if !pulli {
                    out.push_str(v.map(|v| vowel(v, false)).unwrap_or("a"));
                }
            }
            U::Aytham => out.push('h'),
            U::Lit(s) => out.push_str(s),
            U::Other(c) => out.push(*c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::tamil_to_tanglish as t;

    #[test]
    fn everyday_phrases() {
        let cases = [
            ("நான் வீட்டுக்கு போறேன்", "naan veettukku poren"),
            ("வணக்கம்", "vanakkam"),
            ("தமிழ்", "thamizh"),
            ("எப்படி இருக்கீங்க", "eppadi irukkeenga"),
            ("அது", "adhu"),
            ("இது என்ன", "idhu enna"),
            ("எந்த", "endha"),
            ("வந்து", "vandhu"),
            ("மஞ்சள்", "manjal"),
            ("கொஞ்சம்", "konjam"),
            ("எங்க", "enga"),
            ("என்று", "endru"),
            ("நன்றி", "nandri"),
            ("காற்று", "kaatru"),
            ("சொல்லு", "sollu"),
            ("பச்சை", "pachai"),
            ("அப்பா அம்மா அக்கா", "appaa ammaa akkaa"),
            ("தம்பி", "thambi"),
            ("பேசு", "pesu"),
            ("ஆசை", "aasai"),
            ("ரெண்டு", "rendu"),
            ("மகன்", "magan"),
            ("பத்து", "pathu"),
            ("பார்த்து", "paarthu"),
            ("சாப்பிட்டியா", "saappittiyaa"),
            ("காதல்", "kaadhal"),
            ("மதுரை", "madhurai"),
            ("நாய்", "naai"),
            ("செய்", "sei"),
            ("பய்யன்", "payyan"),
            ("ஞாயிறு", "gnaayiru"),
            ("ஏன்", "aen"),
        ];
        for (ta, en) in cases {
            assert_eq!(t(ta), en, "{ta}");
        }
    }

    #[test]
    fn grantha_and_conjuncts() {
        assert_eq!(t("ஜாலி"), "jaali");
        assert_eq!(t("ஹலோ"), "halo");
        assert_eq!(t("லக்ஷ்மி"), "lakshmi");
        assert_eq!(t("ஸ்ரீராம்"), "sriraam");
        assert_eq!(t("ஷாப்"), "shaap");
        assert_eq!(t("எஃகு"), "ehku");
    }

    #[test]
    fn decomposed_vowel_signs() {
        // கொ written as க + ெ + ா, கோ as க + ே + ா, கௌ as க + ெ + ௗ
        assert_eq!(t("\u{0B95}\u{0BC6}\u{0BBE}"), "ko");
        assert_eq!(t("\u{0B95}\u{0BC7}\u{0BBE}"), "ko");
        assert_eq!(t("\u{0B95}\u{0BC6}\u{0BD7}"), "kau");
    }

    #[test]
    fn mixed_text_passes_through() {
        assert_eq!(t("இன்னைக்கு meeting-க்கு late ஆயிடுச்சு."), "innaikku meeting-kku late aayiduchu.");
        assert_eq!(t("Call me at 5:30, சரியா?"), "Call me at 5:30, sariyaa?");
        assert_eq!(t("௨௦௨௬"), "2026");
        assert_eq!(t(""), "");
    }
}
