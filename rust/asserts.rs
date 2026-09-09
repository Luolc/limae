//! Assertions for the crate's own tests.

/// Assert that a value matches a pattern, naming the value it actually got.
///
/// `assert!(matches!(actual, Pattern))` prints only the failed expression: it
/// says which variant was expected and nothing about which one arrived, so a
/// failure that is hard to reproduce leaves no material to attribute it with.
///
/// The value is matched behind a reference, so it stays usable afterwards, and
/// it must implement [`Debug`](std::fmt::Debug). Some success types deliberately
/// have none, so that a path or the user's prose cannot be printed by accident;
/// a `Result` carrying one is asserted through `.as_ref().err()`, which names
/// the error variant that arrived without printing what succeeded.
macro_rules! assert_matches {
    ($actual:expr, $($pattern:tt)+) => {
        match &$actual {
            $($pattern)+ => {}
            unexpected => panic!(
                "expected {}, got {unexpected:?}",
                stringify!($($pattern)+),
            ),
        }
    };
}
