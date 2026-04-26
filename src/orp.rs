pub fn anchor_index(word_len: usize) -> usize {
    match word_len {
        0..=2 => 0,
        3..=5 => 1,
        6..=9 => 2,
        10..=13 => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_positions() {
        assert_eq!(anchor_index(0), 0);
        assert_eq!(anchor_index(1), 0);
        assert_eq!(anchor_index(2), 0);
        assert_eq!(anchor_index(3), 1);
        assert_eq!(anchor_index(5), 1);
        assert_eq!(anchor_index(6), 2);
        assert_eq!(anchor_index(9), 2);
        assert_eq!(anchor_index(10), 3);
        assert_eq!(anchor_index(13), 3);
        assert_eq!(anchor_index(14), 4);
        assert_eq!(anchor_index(42), 4);
    }
}
