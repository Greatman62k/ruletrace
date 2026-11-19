//! The case floor of every property test (Req 20.2).
//!
//! The `proptest!` macro applies `PROPTEST_CASES` to the configured case
//! count, so `PROPTEST_CASES=3` makes a property test run three cases and
//! pass. [`property_tests!`] takes the same syntax and counts the cases that
//! actually passed. The count of a run is the larger of the configured count
//! and the environment value, and a run with fewer passing cases fails.
//!
//! The file is shared by source inclusion (`#[path]` in each test crate) so
//! that no crate gains a dependency edge. It needs only the `proptest`
//! dev-dependency of the including crate.

use proptest::test_runner::{contextualize_config, Config};

/// The least number of generated cases of a property test (Req 20.2).
pub const FLOOR: u32 = 256;

/// Builds the configuration of one property test.
///
/// Every `PROPTEST_*` variable except `PROPTEST_CASES` keeps its meaning.
/// `PROPTEST_CASES` can only raise the count. A configured count below
/// [`FLOOR`] fails unless `below_floor` is set, which the macro sets only for
/// the listed tests of `tools/proptest-below-floor.v1`.
pub fn effective_config(configured: Config, below_floor: bool, name: &str) -> Config {
    assert!(
        below_floor || configured.cases >= FLOOR,
        "property test {name} is configured with {} cases, below the floor of {FLOOR} (Req 20.2)",
        configured.cases
    );
    let mut config = contextualize_config(configured.clone());
    config.cases = config.cases.max(configured.cases);
    config
}

/// Fails when fewer cases passed than the configuration asked for.
pub fn assert_cases_ran(name: &str, wanted: u32, ran: u32) {
    assert!(
        ran >= wanted,
        "property test {name} generated {ran} passing cases, fewer than the {wanted} required (Req 20.2)"
    );
}

/// Declares property tests with the syntax of `proptest!`.
///
/// Without `#![proptest_config(..)]` the count is [`FLOOR`]. The
/// `#![below_floor_config(..)]` form is for the tests listed in
/// `tools/proptest-below-floor.v1` only.
macro_rules! property_tests {
    (@each $below:tt ($config:expr)
     $(
        $(#[$meta:meta])*
        fn $name:ident($($parm:pat in $strategy:expr),+ $(,)?) $body:block
     )*) => {
        $(
            $(#[$meta])*
            fn $name() {
                let name = ::core::concat!(::core::module_path!(), "::", ::core::stringify!($name));
                let mut config = $crate::proptest_floor::effective_config($config, $below, name);
                config.test_name = ::core::option::Option::Some(name);
                let wanted = config.cases;
                let ran = ::core::cell::Cell::new(0u32);
                ::proptest::proptest_helper!(
                    @_BODY config ($($parm in $strategy),+) [] {
                        $body;
                        ran.set(ran.get() + 1);
                    }
                );
                $crate::proptest_floor::assert_cases_ran(name, wanted, ran.get());
            }
        )*
    };
    (@each $below:tt ($config:expr)
     $(
        $(#[$meta:meta])*
        fn $name:ident($($arg:tt)+) $body:block
     )*) => {
        $(
            $(#[$meta])*
            fn $name() {
                let name = ::core::concat!(::core::module_path!(), "::", ::core::stringify!($name));
                let mut config = $crate::proptest_floor::effective_config($config, $below, name);
                config.test_name = ::core::option::Option::Some(name);
                let wanted = config.cases;
                let ran = ::core::cell::Cell::new(0u32);
                ::proptest::proptest_helper!(
                    @_BODY2 config ($($arg)+) [] {
                        $body;
                        ran.set(ran.get() + 1);
                    }
                );
                $crate::proptest_floor::assert_cases_ran(name, wanted, ran.get());
            }
        )*
    };
    (#![proptest_config($config:expr)] $($rest:tt)*) => {
        $crate::proptest_floor::property_tests!(@each false ($config) $($rest)*);
    };
    (#![below_floor_config($config:expr)] $($rest:tt)*) => {
        $crate::proptest_floor::property_tests!(@each true ($config) $($rest)*);
    };
    ($($rest:tt)*) => {
        $crate::proptest_floor::property_tests!(
            @each false (::proptest::test_runner::Config::with_cases(
                $crate::proptest_floor::FLOOR)) $($rest)*);
    };
}

pub(crate) use property_tests;
