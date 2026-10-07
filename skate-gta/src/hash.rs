/// Lower-case Jenkins one-at-a-time hash, as GTA's GET_HASH_KEY computes it.
pub fn joaat(text: &str) -> u32 {
    let mut h: u32 = 0;
    for b in text.bytes() {
        h = h.wrapping_add(b.to_ascii_lowercase() as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

#[cfg(test)]
mod tests {
    #[test]
    fn matches_known_model_hashes() {
        assert_eq!(super::joaat("adder"), 0xB779A091);
        assert_eq!(super::joaat("ADDER"), 0xB779A091);
    }
}
