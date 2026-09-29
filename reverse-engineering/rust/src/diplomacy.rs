//! Diplomacy: relation matrix (`0x43FB90` tech-trade counter).
//!
//! See `../diplomacy.md`. The matrix at `0xA53070` uses the engine's
//! standard 2105-dword row stride with 19-dword columns.

/// Dwords per matrix row (`esi*2105`, byte stride 8420).
pub const ROW_STRIDE: u32 = 2105;
/// Dwords per matrix column (`eax*19`, byte stride 76).
pub const COL_STRIDE: u32 = 19;

/// Dword index of cell `(row, col)` in the relation matrix.
pub fn relation_index(row: u32, col: u32) -> u32 {
    row * ROW_STRIDE + col * COL_STRIDE
}

/// Tech-trade bump: increment the cell and return the new value.
pub fn bump_relation(matrix: &mut [u32], row: u32, col: u32) -> u32 {
    let i = relation_index(row, col) as usize;
    matrix[i] = matrix[i].wrapping_add(1);
    matrix[i]
}

/// Deal-response selector (`0x517B70`): the `0x440EE0` score maps to a
/// `DIPLOADVICETRADE_DEAL_*` script key; any other value falls through
/// with no dialog.
pub fn deal_response(score: u32) -> Option<&'static str> {
    match score {
        36 => Some("DIPLOADVICETRADE_DEAL_ACCEPT"),
        37 => Some("DIPLOADVICETRADE_DEAL_WEAKREJECT"),
        38 => Some("DIPLOADVICETRADE_DEAL_NEUTRALREJECT"),
        39 => Some("DIPLOADVICETRADE_DEAL_STRONGREJECT"),
        _ => None,
    }
}

/// Threshold ladder (`0x44198B-0x4419D6`): side-A total vs side-B total
/// yields the 36..39 verdict. Divisions are the exact `cdq`-and-shift
/// truncating sequences, which match Rust `/` on `i32`.
pub fn deal_threshold_verdict(side_a: i32, side_b: i32) -> u32 {
    if side_a >= side_b {
        36
    } else if side_a > side_b.wrapping_mul(7) / 8 {
        37
    } else if side_a > side_b / 2 {
        38
    } else {
        39
    }
}

/// Attitude scaling (`0x441901-0x441931`): side-B total is multiplied
/// by `4*T+1` where `T` comes from the table at `0xA5304C` (lookup open;
/// caller supplies `t`).
pub fn attitude_scaled(base: i32, t: i32) -> i32 {
    base.wrapping_mul(t.wrapping_mul(4).wrapping_add(1))
}

/// Scorer epilogue deltas (`0x441AA3-0x441AC5`): the `[esp+0x58]`
/// out-param takes offer-minus-ask, `[esp+0x5C]` takes the gated delta
/// (each write null-guarded in the binary; the arithmetic is exact).
pub fn scorer_deltas(offer: i32, ask: i32, gated_a: i32, gated_b: i32) -> (i32, i32) {
    (offer.wrapping_sub(ask), gated_a.wrapping_sub(gated_b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strides_match_disassembly() {
        // Byte strides from the lea/shl immediates.
        assert_eq!(ROW_STRIDE * 4, 8420);
        assert_eq!(COL_STRIDE * 4, 76);
        assert_eq!(relation_index(0, 0), 0);
        assert_eq!(relation_index(1, 0), 2105);
        assert_eq!(relation_index(0, 1), 19);
        assert_eq!(relation_index(2, 3), 2 * 2105 + 3 * 19);
    }

    #[test]
    fn deal_response_words_match_selector() {
        assert_eq!(deal_response(36), Some("DIPLOADVICETRADE_DEAL_ACCEPT"));
        assert_eq!(deal_response(37), Some("DIPLOADVICETRADE_DEAL_WEAKREJECT"));
        assert_eq!(
            deal_response(38),
            Some("DIPLOADVICETRADE_DEAL_NEUTRALREJECT")
        );
        assert_eq!(deal_response(39), Some("DIPLOADVICETRADE_DEAL_STRONGREJECT"));
        assert_eq!(deal_response(35), None);
        assert_eq!(deal_response(40), None);
        assert_eq!(deal_response(0), None);
    }

    #[test]
    fn threshold_ladder_matches_disassembly() {
        // A >= B accepts, even at zero.
        assert_eq!(deal_threshold_verdict(100, 100), 36);
        assert_eq!(deal_threshold_verdict(100, 90), 36);
        assert_eq!(deal_threshold_verdict(0, 0), 36);
        // Boundary trunc(B*7/8) = trunc(700/8) = 87.
        assert_eq!(deal_threshold_verdict(90, 100), 37);
        assert_eq!(deal_threshold_verdict(88, 100), 37);
        assert_eq!(deal_threshold_verdict(87, 100), 38);
        // Boundary trunc(B/2) = 50.
        assert_eq!(deal_threshold_verdict(51, 100), 38);
        assert_eq!(deal_threshold_verdict(50, 100), 39);
        assert_eq!(deal_threshold_verdict(0, 100), 39);
        // Odd B truncates: trunc(7*7/8) = 6.
        assert_eq!(deal_threshold_verdict(7, 7), 36);
        assert_eq!(deal_threshold_verdict(6, 7), 38);
        // Full chain: ladder output selects the dialog word.
        let v = deal_threshold_verdict(90, 100);
        assert_eq!(deal_response(v), Some("DIPLOADVICETRADE_DEAL_WEAKREJECT"));
        assert_eq!(deal_response(40), None);
        assert_eq!(deal_response(44), None);
    }

    #[test]
    fn attitude_scale_is_four_t_plus_one() {
        assert_eq!(attitude_scaled(100, 0), 100);
        assert_eq!(attitude_scaled(100, 2), 900);
        assert_eq!(attitude_scaled(100, 1), 500);
    }

    #[test]
    fn epilogue_writes_both_deltas() {
        // 0x441AA3: [esp+0x58] = offer - ask; 0x441ABB: [esp+0x5C] = gated_a - gated_b.
        assert_eq!(scorer_deltas(120, 100, 30, 10), (20, 20));
        assert_eq!(scorer_deltas(50, 100, 0, 25), (-50, -25));
    }

    #[test]
    fn bump_increments_one_cell() {
        let mut m = vec![0u32; (2 * ROW_STRIDE + COL_STRIDE) as usize];
        assert_eq!(bump_relation(&mut m, 1, 1), 1);
        assert_eq!(bump_relation(&mut m, 1, 1), 2);
        assert_eq!(m[relation_index(1, 0) as usize], 0);
        assert_eq!(m[relation_index(0, 1) as usize], 0);
    }
}
