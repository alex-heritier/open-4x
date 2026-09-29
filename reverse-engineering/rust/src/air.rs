//! Air combat: move dispatch (`0x456840`).
//!
//! See `../air.md`. The four move sites push their log strings and share
//! one commit tail; the log call itself targets a bare `ret` stub.

/// `AirBombardMove 1..4` strings in move order (VAs `0x684A8C/78/64/50`).
pub const MOVE_STRINGS: &[&str] = &[
    "AirBombardMove 1",
    "AirBombardMove 2",
    "AirBombardMove 3",
    "AirBombardMove 4",
];

/// `0x5F98B0` is `C3` + 15 `90` (bare ret): the bombard-move log call is
/// a disabled no-op at every one of its 50 call sites.
pub fn log_is_noop(stub: &[u8]) -> bool {
    !stub.is_empty() && stub[0] == 0xC3 && stub[1..].iter().all(|&b| b == 0x90)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_moves_in_order() {
        assert_eq!(MOVE_STRINGS.len(), 4);
        assert!(MOVE_STRINGS[0].ends_with('1'));
        assert!(MOVE_STRINGS[3].ends_with('4'));
    }

    #[test]
    fn ret_stub_shape() {
        let mut stub = vec![0x90u8; 16];
        stub[0] = 0xC3;
        assert!(log_is_noop(&stub));
        assert!(!log_is_noop(&[0x90u8; 16]));
        assert!(!log_is_noop(&[]));
    }
}
