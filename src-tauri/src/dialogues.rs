//! Single source of truth for everything Fairy says — reminders,
//! idle-bark, and eye-click flavor lines — in both display (English) and
//! voice (English/Japanese) form.
//!
//! Previously this content was scattered: English reminder/idle-bark text
//! lived inline in `behavior.rs`, Japanese translations lived in separate
//! index-parallel arrays in `voice.rs`, and flavor lines' English lived in
//! `renderer/index.ts` with no Japanese counterpart at all until a second
//! array was added to `voice.rs`. Pairing each line's English and Japanese
//! together in one [`Line`] struct, in one file, makes "the display text
//! and the voiced text are for the same line" a structural guarantee
//! instead of a "keep these arrays in sync by index" convention.
//!
//! Flavor lines are the one category the Renderer used to own outright
//! (picked and displayed with zero backend involvement, for an instant
//! click response). They're canonical here now too — the Renderer fetches
//! them once via the `get_flavor_lines` command at startup and picks
//! locally from that cached copy afterward, so the click itself is still
//! instant; only the *source* of the English text moved, not when it's
//! read.

use crate::state::VoiceLanguage;
use rand::Rng;

/// One line of dialogue, paired across languages so a line picked for
/// display always has a matching voice line — see the module docs above
/// for why this replaced two separately-indexed arrays.
pub struct Line {
    pub en: &'static str,
    pub ja: &'static str,
}

impl Line {
    pub fn for_language(&self, language: VoiceLanguage) -> &'static str {
        match language {
            VoiceLanguage::En => self.en,
            VoiceLanguage::Ja => self.ja,
        }
    }
}

/// Picks a uniformly random line from `lines`. Callers pass one of this
/// module's `*_LINES` constants, all `'static`, so the returned reference
/// borrows for `'static` too — no lifetime ties back to the caller.
pub fn pick(lines: &'static [Line]) -> &'static Line {
    let index = rand::thread_rng().gen_range(0..lines.len());
    &lines[index]
}

// Translations below are drafts — see VOICE_SPEC.md's "Translations"
// section: worth a native speaker's review before shipping, same caveat
// as when these were first written.

pub const WATER_LINES: &[Line] = &[
    Line {
        en: "Time to drink some water, master.",
        ja: "水を飲む時間だよ、マスター。",
    },
    Line {
        en: "Stay hydrated, master — grab some water.",
        ja: "マスター、水分補給を忘れないでね。",
    },
    Line {
        en: "Master, a little water break?",
        ja: "マスター、少し水を飲もうか?",
    },
];

pub const BREAK_LINES: &[Line] = &[
    Line {
        en: "Stand up and stretch for 5 minutes, master.",
        ja: "5分間、立って伸びをしよう、マスター。",
    },
    Line {
        en: "Master, time for a quick break.",
        ja: "マスター、少し休憩しようか。",
    },
    Line {
        en: "Give your eyes a rest, master.",
        ja: "目を休めてね、マスター。",
    },
];

pub const WORKOUT_LINES: &[Line] = &[
    Line {
        en: "Workout time, master.",
        ja: "ワークアウトの時間だよ、マスター。",
    },
    Line {
        en: "Let's get moving, master.",
        ja: "そろそろ体を動かそうか、マスター。",
    },
    Line {
        en: "Master, time to exercise.",
        ja: "マスター、運動の時間だよ。",
    },
];

pub const IDLE_BARK_LINES: &[Line] = &[
    Line {
        en: "Just checking in, master.",
        ja: "ちょっと様子を見に来たよ、マスター。",
    },
    Line {
        en: "Don't forget I'm here, master.",
        ja: "私がここにいること、忘れないでね、マスター。",
    },
    Line {
        en: "It's quiet today, master.",
        ja: "今日は静かだね、マスター。",
    },
    Line {
        en: "Still here, master.",
        ja: "まだここにいるよ、マスター。",
    },
];

/// Eye-click flavor lines — fluff/testing aid, not a real reminder. See
/// the module docs above for how the Renderer consumes these.
pub const FLAVOR_LINES: &[Line] = &[
    Line {
        en: "Yes, master?",
        ja: "はい、マスター?",
    },
    Line {
        en: "I'm right here, master.",
        ja: "ここにいるよ、マスター。",
    },
    Line {
        en: "Did you need something, master?",
        ja: "何か用かな、マスター?",
    },
    Line {
        en: "Master, Koro needs your attention.",
        ja: "マスター、コロが構ってほしがっています。",
    },
    Line {
        en: "Master, don't you have other things to do?",
        ja: "マスター、他にやるべきことはないのですか?",
    },
    Line {
        en: "Master, have you checked in on Belle?",
        ja: "マスター、リンの様子は見ましたか?",
    },
    Line {
        en: "Silverwolf has sent a message for you, master.",
        ja: "銀狼様から、マスターへのメッセージが届いております。",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_LINE_SETS: &[(&str, &[Line])] = &[
        ("WATER_LINES", WATER_LINES),
        ("BREAK_LINES", BREAK_LINES),
        ("WORKOUT_LINES", WORKOUT_LINES),
        ("IDLE_BARK_LINES", IDLE_BARK_LINES),
        ("FLAVOR_LINES", FLAVOR_LINES),
    ];

    #[test]
    fn every_line_set_has_at_least_two_variations() {
        for (name, lines) in ALL_LINE_SETS {
            assert!(
                lines.len() >= 2,
                "{name} has only {} variation(s) — random selection needs at least 2 to be meaningful",
                lines.len()
            );
        }
    }

    #[test]
    fn every_line_has_non_empty_english_and_japanese() {
        for (name, lines) in ALL_LINE_SETS {
            for line in *lines {
                assert!(!line.en.is_empty(), "{name} has an empty English line");
                assert!(!line.ja.is_empty(), "{name} has an empty Japanese line");
            }
        }
    }

    #[test]
    fn for_language_selects_the_matching_field() {
        let line = &WATER_LINES[0];
        assert_eq!(line.for_language(VoiceLanguage::En), line.en);
        assert_eq!(line.for_language(VoiceLanguage::Ja), line.ja);
    }

    #[test]
    fn pick_always_returns_a_member_of_the_input_slice() {
        for _ in 0..50 {
            let picked = pick(WATER_LINES);
            assert!(WATER_LINES.iter().any(|l| l.en == picked.en));
        }
    }

    #[test]
    fn pick_eventually_produces_more_than_one_distinct_line() {
        // Probabilistic, not exact — with 3 variations, 50 draws all
        // landing on the same one has a ~1-in-7*10^23 chance, so this is
        // effectively deterministic in practice without pinning an RNG seed.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            seen.insert(pick(WATER_LINES).en);
        }
        assert!(seen.len() > 1, "pick() returned the same line 50/50 times");
    }
}
