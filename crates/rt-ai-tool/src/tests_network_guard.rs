//! The runtime half of the network guard (see `tests/network_guard.rs`).
//!
//! A test that builds the real hosted adapter for a non-loopback endpoint
//! panics in `hosted::responses_adapter` before any connection exists.

use crate::tests::MapEnv;
use crate::tests_e2e_responses::{Rig, CANARY, DEFAULT};
use crate::tests_gate_rig::run_providers;
use crate::{BuiltInProviders, StdFs};

#[test]
#[should_panic(expected = "non-loopback endpoint")]
fn the_real_providers_refuse_a_real_host_in_a_test_run() {
    let rig = Rig::new("guard-real-host", "https://api.meta.ai", &DEFAULT);
    let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
    let _ = run_providers(&rig.fixture.consented(), &env, &BuiltInProviders, &StdFs);
}
