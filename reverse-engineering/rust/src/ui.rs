//! UI text: civilopedia hypertext tokenizer (`0x5FE280` ff).
//!
//! See `../ui.md`. The engine lexes `$`-tags out of civilopedia text with
//! `strncmp` (`0x64AE80`); lengths 6/9/9 below are the pushed immediates.

/// Token kinds of the hypertext lexer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    /// `$LINK<`: hyperlink open (`[edi+0x3C] = 1`).
    Link,
    /// `$DROPDOWN`: dropdown open (`[edi+0x48] = 1`).
    Dropdown,
    /// `$DROPLINK`: dropdown link (`[edi+0x48] = 1`, `ebp = 3`).
    Droplink,
    /// `$$` or `$~`: literal escape.
    Escape(char),
    /// Any other byte: literal text.
    Lit(u8),
}

/// Lex `text` into hypertext tokens. Tag match order follows the binary:
/// `$LINK<` (6) before `$DROPDOWN` (9) before `$DROPLINK` (9); `$$`/`$~`
/// collapse to one literal `$`/`~`.
pub fn tokenize(text: &[u8]) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if let Some(t) = tag_at(rest) {
            out.push(t);
            i += tag_len(t);
        } else {
            out.push(Tok::Lit(text[i]));
            i += 1;
        }
    }
    out
}

fn tag_at(rest: &[u8]) -> Option<Tok> {
    if rest.starts_with(b"$LINK<") {
        Some(Tok::Link)
    } else if rest.starts_with(b"$DROPDOWN") {
        Some(Tok::Dropdown)
    } else if rest.starts_with(b"$DROPLINK") {
        Some(Tok::Droplink)
    } else if rest.starts_with(b"$$") {
        Some(Tok::Escape('$'))
    } else if rest.starts_with(b"$~") {
        Some(Tok::Escape('~'))
    } else {
        None
    }
}

fn tag_len(t: Tok) -> usize {
    match t {
        Tok::Link => 6,
        Tok::Dropdown | Tok::Droplink => 9,
        Tok::Escape(_) => 2,
        Tok::Lit(_) => 1,
    }
}

/// Military-advice rank ratio (`0x429BC3`/`0x429C6D`): `(power * 4) / 10`
/// via the `0x66666667` magic divider (truncating). Compared against the
/// foe threshold to select `RANK_WEAK`/`RANK_STRONG`.
pub fn rank_ratio(power: i32) -> i32 {
    power.wrapping_mul(4) / 10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_lex_in_binary_order() {
        // $DROPLINK must not mislex as $DROPDOWN prefix: order matters.
        assert_eq!(
            tokenize(b"$LINK<$DROPDOWN$DROPLINK"),
            vec![Tok::Link, Tok::Dropdown, Tok::Droplink]
        );
    }

    #[test]
    fn escapes_and_literals() {
        assert_eq!(
            tokenize(b"a$$b$~c$d"),
            vec![
                Tok::Lit(b'a'),
                Tok::Escape('$'),
                Tok::Lit(b'b'),
                Tok::Escape('~'),
                Tok::Lit(b'c'),
                Tok::Lit(b'$'),
                Tok::Lit(b'd'),
            ]
        );
    }

    #[test]
    fn partial_tag_is_literal() {
        // "$LIN" is not a tag: every byte stays literal.
        assert_eq!(
            tokenize(b"$LIN"),
            vec![Tok::Lit(b'$'), Tok::Lit(b'L'), Tok::Lit(b'I'), Tok::Lit(b'N')]
        );
    }

    #[test]
    fn rank_ratio_matches_magic_divider() {
        // (x*4)/10 truncating: 0x429BC3 imul/sar/sign-fix sequence.
        assert_eq!(rank_ratio(0), 0);
        assert_eq!(rank_ratio(10), 4);
        assert_eq!(rank_ratio(7), 2);
        assert_eq!(rank_ratio(25), 10);
    }
}
