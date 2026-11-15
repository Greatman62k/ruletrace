//! The credential type (Req 28.5, 28.9).

use crate::credential::{Credential, CredentialError, EnvReader};
use std::collections::BTreeMap;

struct Map(BTreeMap<&'static str, &'static str>);

impl EnvReader for Map {
    fn get(&self, name: &str) -> Option<String> {
        self.0.get(name).map(|v| (*v).to_owned())
    }
}

#[test]
fn reads_only_the_named_variable() {
    let env = Map(BTreeMap::from([("A_KEY", "va"), ("B_KEY", "vb")]));
    assert_eq!(Credential::from_env("B_KEY", &env).unwrap().expose(), "vb");
}

#[test]
fn unset_and_empty_are_missing() {
    let env = Map(BTreeMap::from([("EMPTY", "")]));
    assert_eq!(
        Credential::from_env("EMPTY", &env).unwrap_err(),
        CredentialError::Missing
    );
    assert_eq!(
        Credential::from_env("ABSENT", &env).unwrap_err(),
        CredentialError::Missing
    );
}

#[test]
fn debug_prints_no_value() {
    let c = Credential::new("canary-value-123".to_owned()).unwrap();
    let shown = format!("{c:?} {:#?}", Some(&c));
    assert!(!shown.contains("canary"), "{shown}");
    assert!(shown.contains("[REDACTED]"));
}
