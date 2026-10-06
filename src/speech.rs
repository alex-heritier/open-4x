//! What the leaders say: `Text/diplomacy.txt`, converted to UTF-8 by
//! `tools/prep_assets.py diplomacy`.
//!
//! The file is a list of `#KEY` blocks. A block says how its lines are
//! arranged with `#civ`, `#power`, `#mood` and `#random`, and lists them
//! flat: one line for every combination, civilization outermost and the
//! random variant innermost. A `#civ 1` block holds a line for each of the
//! 32 text sets (`RACE.diplomacy_text_index`, which is the race's row minus
//! one when it is -1), `#power 1` three tones for the leader being weaker
//! than, level with or stronger than the player, `#mood 1` three tones from
//! friendly to hostile, and `#random n` n phrasings of each. Every block's
//! line count is the product (`tests::every_block_holds_all_its_lines`).
//!
//! The lines name things with `$NAME<digit>` (`$AI0`, `$CIVADJ2`, ...); the
//! comment above the block says what each digit stands for ("player's
//! name", "ai's people (adj)"), and that is what the fill reads.
use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::civs::CIVS;
use crate::diplomacy::people;
use crate::leaders::LEADERS;

const PATH: &str = "assets/cache/text/diplomacy.txt";
/// Text sets in the file.
#[cfg(test)]
pub const TEXT_SETS: usize = 32;
/// Tones of `#power` and `#mood` blocks.
const TONES: usize = 3;

#[derive(Debug, Default)]
struct Set {
    by_civ: bool,
    by_power: bool,
    by_mood: bool,
    random: usize,
    lines: Vec<String>,
    /// `(name, digit, description)` from the comment above the block.
    vars: Vec<(String, u8, String)>,
}

impl Set {
    /// Lines a block holds: one per combination of civ, tone and phrasing.
    #[cfg(test)]
    fn expected(&self) -> usize {
        let n = |on: bool, n: usize| if on { n } else { 1 };
        n(self.by_civ, TEXT_SETS) * n(self.by_power, TONES) * n(self.by_mood, TONES) * self.random
    }
}

/// Everything a line may name.
pub struct Who<'a> {
    /// The leader who speaks.
    pub ai: usize,
    /// The human spoken to.
    pub player: usize,
    /// A third civilization (an embargo or alliance target, an introduction).
    pub third: Option<usize>,
    /// What the leader offers (`$GIVE`, `$TECH`, `$LUXURY`).
    pub give: &'a str,
    /// What the leader wants (`$GET`).
    pub get: &'a str,
    pub city: &'a str,
}

impl Who<'_> {
    pub fn between(ai: usize, player: usize) -> Who<'static> {
        Who { ai, player, third: None, give: "something", get: "something", city: "our lands" }
    }
}

#[derive(Resource, Default)]
pub struct Speech {
    sets: HashMap<String, Set>,
}

impl Speech {
    /// The converted file; empty (every `say` is `None`) when it is missing.
    pub fn load() -> Speech {
        match crate::web::read_text(PATH) {
            Ok(text) => Speech::parse(&text),
            Err(_) => Speech::default(),
        }
    }

    pub fn parse(text: &str) -> Speech {
        let mut sets: HashMap<String, Set> = HashMap::new();
        let mut comments: Vec<&str> = vec![];
        let mut current: Option<String> = None;
        for raw in text.lines() {
            let line = raw.trim();
            if let Some(note) = line.strip_prefix(';') {
                comments.push(note.trim());
            } else if let Some(head) = line.strip_prefix('#') {
                let mut words = head.split_whitespace();
                let word = words.next().unwrap_or("");
                let number = words.next().and_then(|n| n.parse::<usize>().ok());
                let key = current.as_ref().and_then(|k| sets.get_mut(k));
                match (word, number, key) {
                    ("civ", Some(n), Some(s)) => s.by_civ = n != 0,
                    ("power", Some(n), Some(s)) => s.by_power = n != 0,
                    ("mood", Some(n), Some(s)) => s.by_mood = n != 0,
                    ("random", Some(n), Some(s)) => s.random = n.max(1),
                    _ => {
                        let name = word.to_string();
                        let vars = comments.iter().filter_map(|c| var_note(c)).collect();
                        sets.insert(name.clone(), Set { random: 1, vars, ..Set::default() });
                        current = Some(name);
                        comments.clear();
                    }
                }
            } else if !line.is_empty()
                && let Some(s) = current.as_ref().and_then(|k| sets.get_mut(k))
            {
                s.lines.push(unquote(line).to_string());
            }
        }
        Speech { sets }
    }

    /// The line of block `key` that the leader of `ai` says, in the tone of
    /// `power` (0 weaker, 1 level, 2 stronger than the player) and `mood`
    /// (0 friendly .. 2 hostile); `roll` picks among the phrasings.
    pub fn say(&self, key: &str, power: usize, mood: usize, roll: usize, who: &Who) -> Option<String> {
        let set = self.sets.get(key)?;
        let n = |on: bool| if on { TONES } else { 1 };
        let civ = if set.by_civ { LEADERS[who.ai].text_set } else { 0 };
        let p = if set.by_power { power.min(TONES - 1) } else { 0 };
        let m = if set.by_mood { mood.min(TONES - 1) } else { 0 };
        let at = ((civ * n(set.by_power) + p) * n(set.by_mood) + m) * set.random + roll % set.random;
        Some(fill(set, set.lines.get(at)?, who))
    }
}

/// `$GET6 = what ai wants` -> `("GET", 6, "what ai wants")`.
fn var_note(comment: &str) -> Option<(String, u8, String)> {
    let rest = comment.strip_prefix('$')?;
    let name_len = rest.find(|c: char| !c.is_ascii_uppercase())?;
    let (name, rest) = rest.split_at(name_len);
    let digit = rest.chars().next()?.to_digit(10)? as u8;
    let desc = rest[1..].trim_start().strip_prefix('=')?.trim();
    Some((name.to_string(), digit, desc.to_lowercase()))
}

fn unquote(line: &str) -> &str {
    let quote = |c: char| matches!(c, '"' | '\u{201c}' | '\u{201d}');
    let line = line.strip_prefix(quote).unwrap_or(line);
    line.strip_suffix(quote).unwrap_or(line)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Owner {
    Ai,
    Player,
    Third,
}

fn owner(desc: &str) -> Owner {
    if ["enemy", "embargo", "stabbed", "communications"].iter().any(|w| desc.contains(w)) {
        Owner::Third
    } else if desc.contains("player") {
        Owner::Player
    } else {
        Owner::Ai
    }
}

/// Replace every `$NAME<digit>` of `line`.
fn fill(set: &Set, line: &str, who: &Who) -> String {
    let mut out = String::with_capacity(line.len() + 16);
    let mut rest = line;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        let name_len = rest.find(|c: char| !c.is_ascii_uppercase()).unwrap_or(rest.len());
        let name = &rest[..name_len];
        let digit = rest[name_len..].chars().next().and_then(|c| c.to_digit(10));
        if name.is_empty() {
            out.push('$');
            continue;
        }
        rest = &rest[name_len + digit.map_or(0, |_| 1)..];
        let desc = digit
            .and_then(|d| set.vars.iter().find(|(n, k, _)| n == name && u32::from(*k) == d))
            .map_or("", |(_, _, d)| d.as_str());
        out.push_str(&value(name, desc, who));
    }
    out.push_str(rest);
    out
}

fn value(name: &str, desc: &str, who: &Who) -> String {
    let civ = || match owner(desc) {
        Owner::Ai => who.ai,
        Owner::Player => who.player,
        Owner::Third => who.third.unwrap_or(who.ai),
    };
    match name {
        n if n.starts_with("PLAY") => LEADERS[who.player].name.into(),
        "AI" | "LEADER" | "LEADERNAME" => LEADERS[who.ai].name.into(),
        "CIVNAME" => CIVS[civ()].name.into(),
        "CIVADJ" | "CIVDJ" => CIVS[civ()].adjective.into(),
        "CIVNOUN" => people(civ()),
        "GIVE" | "TECH" | "LUXURY" => who.give.into(),
        "GET" => who.get.into(),
        "CITY" => who.city.into(),
        "UNIT" => "armies".into(),
        _ => String::new(),
    }
}

/// How the leader stands to the human, as the tone of a `#power` block:
/// 0 when weaker, 1 level, 2 stronger. `ai` and `human` are their scores.
pub fn power_tone(ai: i32, human: i32) -> usize {
    if ai * 4 < human * 3 {
        0
    } else if ai * 3 > human * 4 {
        2
    } else {
        1
    }
}

/// The five attitude classes of `diplomacy::attitude_label` on the three
/// tones of a `#mood` block: gracious and polite are friendly, cautious
/// neutral, annoyed and furious hostile.
pub fn mood_tone(class: i32) -> usize {
    match class {
        ..=1 => 0,
        2 => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civs::{CIV_CAP, civ_count};

    const SAMPLE: &str = "\
;\tDiplomacy
; $AI0 = AI's name
; $CIVNAME1 = AI's Civ
; $CIVADJ2 = player's people (ADJ)
#HELLO
#civ 1
#power 0
#mood 0
#random 1
\"I, $AI0, of $CIVNAME1, greet the $CIVADJ2.\"
\"Second text set.\"

; $PLAYER0 = player's name
; $CIVNAME3 = civ the ai wants to embargo
#WAR
#civ 0
#power 1
#mood 0
#random 2
\"Weak one, $PLAYER0.\"
\"Weak two.\"
\"Level one.\"
\"Level two.\"
\u{201c}Strong one, $PLAYER0, and $CIVNAME3.\u{201d}
\"Strong two.\"
";

    #[test]
    fn blocks_keep_their_arrangement_and_lines() {
        let s = Speech::parse(SAMPLE);
        let hello = &s.sets["HELLO"];
        assert!(hello.by_civ && !hello.by_power && !hello.by_mood);
        assert_eq!(hello.lines.len(), 2);
        let war = &s.sets["WAR"];
        assert_eq!((war.by_civ, war.by_power, war.random, war.lines.len()), (false, true, 2, 6));
        assert_eq!(war.lines[4], "Strong one, $PLAYER0, and $CIVNAME3.", "curly quotes come off too");
        assert_eq!(war.expected(), 6);
    }

    #[test]
    fn a_line_is_picked_by_tone_and_roll_and_filled_from_the_notes() {
        let s = Speech::parse(SAMPLE);
        // Japan (human, 0) listens to Rome (1): "$CIVADJ2" is the player's people.
        let mut who = Who::between(1, 0);
        let hello = s.say("HELLO", 0, 0, 0, &who).unwrap();
        assert!(hello.starts_with("I, Caesar, of Rome, greet the Japanese"), "{hello}");
        who.third = Some(2);
        assert_eq!(s.say("WAR", 2, 0, 0, &who).unwrap(), "Strong one, Tokugawa, and Egypt.");
        assert_eq!(s.say("WAR", 0, 0, 3, &who).unwrap(), "Weak two.", "the roll wraps over the phrasings");
        assert_eq!(s.say("WAR", 1, 0, 1, &who).unwrap(), "Level two.");
        assert_eq!(s.say("WAR", 9, 0, 1, &who).unwrap(), "Strong two.", "tones saturate");
        assert!(s.say("NOPE", 0, 0, 0, &who).is_none());
    }

    #[test]
    fn a_dollar_that_names_nothing_stays_and_unnoted_names_default_to_their_owner() {
        let s = Speech::parse("#X\n#random 1\n\"Pay $5 to $AI9 and $PLAYER0\"\n");
        assert_eq!(s.say("X", 0, 0, 0, &Who::between(2, 0)).unwrap(), "Pay $5 to Cleopatra and Tokugawa");
    }

    #[test]
    fn tones_follow_strength_and_attitude() {
        assert_eq!([power_tone(5, 10), power_tone(10, 10), power_tone(20, 10)], [0, 1, 2]);
        assert_eq!([0, 1, 2, 3, 4].map(mood_tone), [0, 0, 1, 2, 2]);
    }

    /// The real file, when prep has converted it.
    fn real() -> Option<Speech> {
        let text = std::fs::read_to_string(PATH).ok()?;
        Some(Speech::parse(&text))
    }

    #[test]
    fn every_block_holds_all_its_lines() {
        let Some(s) = real() else {
            eprintln!("skipped: {PATH} is not built");
            return;
        };
        let mut checked = 0;
        for (key, set) in &s.sets {
            // HEADINGS, GOLD and friends are plain lists, not arranged.
            if !set.by_civ && !set.by_power && !set.by_mood && set.random == 1 {
                continue;
            }
            assert_eq!(set.lines.len(), set.expected(), "{key}");
            checked += 1;
        }
        assert!(checked > 60, "{checked} arranged blocks");
    }

    #[test]
    fn each_leader_greets_in_their_own_voice() {
        let Some(s) = real() else {
            eprintln!("skipped: {PATH} is not built");
            return;
        };
        for ai in 0..civ_count() {
            let human = (ai + 1) % civ_count();
            let said = s.say("AIFIRSTCONTACT", 0, 0, 0, &Who::between(ai, human)).unwrap();
            assert!(said.contains(LEADERS[ai].name), "{ai}: {said}");
            assert!(!said.contains('$'), "{said}");
        }
    }

    #[test]
    fn no_line_of_the_real_file_leaves_a_variable_unfilled() {
        let Some(s) = real() else {
            eprintln!("skipped: {PATH} is not built");
            return;
        };
        let who = Who::between(1, 0);
        for (key, set) in &s.sets {
            for line in &set.lines {
                let said = fill(set, line, &who);
                assert!(!said.contains('$'), "{key}: {said}");
            }
        }
    }
}
