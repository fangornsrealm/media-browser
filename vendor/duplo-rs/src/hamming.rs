/// Hamming distance between two u64 bit vectors: the number of positions at
/// which the bits differ.
///
/// This compiles down to a single `POPCNT` on x86, and never allocates —
/// important because `search_matches` calls it millions of times per query.
pub fn hamming_distance(left: u64, right: u64) -> i64 {
    (left ^ right).count_ones() as i64
}
