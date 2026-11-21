//! Property 31, proposal identifiers (Req 29.9, 29.16).
//!
//! The expected bytes are built here from the request id text, a colon and
//! decimal digits produced by a local loop, and hashed with
//! `rt_hash::sha256`. Golden digests come from the external `sha256sum`.

use crate::proposal_id;
use crate::proptest_floor::property_tests;
use proptest::prelude::*;
use rt_hash::{sha256, Sha256Hex};

fn digits(mut n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        out.push(b'0' + u8::try_from(n % 10).unwrap());
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    out
}

fn oracle(request_id: &str, index: usize) -> Sha256Hex {
    let mut bytes = request_id.as_bytes().to_vec();
    bytes.push(b':');
    bytes.extend(digits(index));
    sha256(&bytes)
}

fn request_id() -> impl Strategy<Value = Sha256Hex> {
    proptest::collection::vec(0u32..16, 64..=64).prop_map(|nibbles| {
        let text: String = nibbles
            .into_iter()
            .map(|n| char::from_digit(n, 16).unwrap())
            .collect();
        Sha256Hex::parse(&text).unwrap()
    })
}

fn index() -> impl Strategy<Value = usize> {
    prop_oneof![0usize..40, 0usize..100_000, Just(usize::MAX)]
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn proposal_id_is_the_digest_of_request_id_colon_index(
        request in request_id(),
        i in index(),
    ) {
        prop_assert_eq!(proposal_id(&request, i), oracle(request.as_str(), i));
    }

    #[test]
    fn a_different_index_gives_a_different_id(
        request in request_id(),
        i in index(),
        j in index(),
    ) {
        prop_assume!(i != j);
        prop_assert_ne!(proposal_id(&request, i), proposal_id(&request, j));
    }

    #[test]
    fn a_different_request_gives_a_different_id(
        a in request_id(),
        b in request_id(),
        i in index(),
    ) {
        prop_assume!(a != b);
        prop_assert_ne!(proposal_id(&a, i), proposal_id(&b, i));
    }
}

#[test]
fn golden_proposal_ids_match_the_external_digest() {
    let request = sha256(b"abc");
    assert_eq!(
        request.as_str(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        proposal_id(&request, 0).as_str(),
        "b7b3f1b5326deebb03df441c26fa3a921a34c97913bcb07ba9a0782192650ca9"
    );
    assert_eq!(
        proposal_id(&request, 12).as_str(),
        "619665bddf2280d4b14f64d989d51f147da34a70e8e690ed765b4b9531d8045e"
    );
}
