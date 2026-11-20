//! City names from syllables (`assets/mapgen/names.txt`, our own tables):
//! two or three syllables, an onset, a vowel and now and then a coda, and
//! sometimes an ending; a capital's with a grand ending. Every name on a map
//! is its own.

use super::rng::Rng;

/// The tables, as the file has them.
pub const NAMES: &str = include_str!("../../assets/mapgen/names.txt");

/// The tables of syllables.
pub struct Syllables {
    onsets: Vec<String>,
    vowels: Vec<String>,
    codas: Vec<String>,
    endings: Vec<String>,
    grand: Vec<String>,
}

impl Syllables {
    /// The tables in a text of `key word word ...` lines.
    pub fn parse(text: &str) -> Syllables {
        let line = |key: &str| -> Vec<String> {
            text.lines()
                .map(|l| l.split('#').next().unwrap_or("").trim())
                .find(|l| l.split_whitespace().next() == Some(key))
                .map(|l| l.split_whitespace().skip(1).map(str::to_string).collect())
                .unwrap_or_default()
        };
        Syllables {
            onsets: line("onsets"),
            vowels: line("vowels"),
            codas: line("codas"),
            endings: line("endings"),
            grand: line("grand"),
        }
    }

    /// A name not in `taken`: a capital's (`grand`) or another's.
    pub fn name(&self, rng: &mut Rng, grand: bool, taken: &[String]) -> String {
        for _ in 0..200 {
            let n = self.one(rng, grand);
            if n.len() >= 4 && n.len() <= 13 && !taken.iter().any(|t| t.eq_ignore_ascii_case(&n)) {
                return n;
            }
        }
        // Many maps' worth of names taken: a number makes it new.
        format!("{} {}", self.one(rng, grand), taken.len() + 1)
    }

    fn one(&self, rng: &mut Rng, grand: bool) -> String {
        let pick = |list: &[String], rng: &mut Rng| rng.pick(list).cloned().unwrap_or_default();
        let ending = if grand {
            Some(pick(&self.grand, rng))
        } else if rng.below(5) < 2 {
            Some(pick(&self.endings, rng))
        } else {
            None
        };
        let syllables = if ending.is_some() { 1 + rng.below(2) } else { 2 + rng.below(2) };
        let mut word = String::new();
        for i in 0..syllables {
            if i == 0 || rng.below(3) > 0 {
                word.push_str(&pick(&self.onsets, rng));
            }
            word.push_str(&pick(&self.vowels, rng));
            if rng.below(3) == 0 {
                word.push_str(&pick(&self.codas, rng));
            }
        }
        if let Some(e) = ending {
            word.push_str(&e);
        }
        let mut chars = word.chars();
        chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_words_of_their_own_and_new() {
        let s = Syllables::parse(NAMES);
        let mut rng = Rng::stream(4, "names");
        let mut taken: Vec<String> = Vec::new();
        for i in 0..200 {
            let n = s.name(&mut rng, i % 20 == 0, &taken);
            assert!(n.chars().next().is_some_and(|c| c.is_ascii_uppercase()), "{n}");
            assert!(n.chars().skip(1).all(|c| c.is_ascii_lowercase()), "{n}");
            assert!(!taken.contains(&n));
            taken.push(n);
        }
        // A capital's has a grand ending.
        let capital = s.name(&mut rng, true, &taken);
        assert!(
            ["hold", "keep", "gate", "crown", "spire", "haven", "garde", "mount", "throne"]
                .iter()
                .any(|e| capital.ends_with(e))
        );
    }
}
