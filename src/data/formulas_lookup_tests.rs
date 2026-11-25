//! Focused lookup complexity checks.
use super::*;
use std::cell::Cell;

#[test]
fn sorted_catalogue_probes_remain_logarithmic() {
    let probes = Cell::new(0);
    let count = 100_000;
    // Each even catalogue ID occurs twice. Exercise both paths that
    // require finding the front of a duplicate threshold group.
    for descending in [false, true] {
        let key_at = |index| {
            probes.set(probes.get() + 1);
            let pair = index / 2;
            let tier = if descending {
                count / 2 - 1 - pair
            } else {
                pair
            };
            Some(Key::Integer(2 * tier as i64))
        };
        let mut total_probes = 0;
        for i in (0..count / 2 - 1).step_by(13) {
            let expected = if descending { count - 2 - 2 * i } else { 2 * i };
            let side = if descending {
                Ordering::Greater
            } else {
                Ordering::Less
            };
            let query = if descending {
                2 * i as i64 - 1
            } else {
                2 * i as i64 + 1
            };
            probes.set(0);
            assert_eq!(
                binary_numeric(count, Key::Integer(query), Some(side), descending, key_at),
                Some(Some(expected))
            );
            assert!(
                probes.get() <= 37,
                "{} probes for {count} keys",
                probes.get()
            );
            total_probes += probes.get();
        }
        assert!(total_probes <= 143_000);
    }
}
