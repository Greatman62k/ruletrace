//! The catalogue body mapping (Req 35.18). Expected lists are written by
//! hand and the cases follow the shapes of the OpenAI-style model list.

use crate::catalog::parse_catalogue;

#[test]
fn identifiers_come_back_in_the_order_reported_with_repeats_kept() {
    let body = br#"{"object":"list","data":[{"id":"b","created":2},{"id":"a"},{"id":"b"}]}"#;
    assert_eq!(parse_catalogue(body).unwrap(), ["b", "a", "b"]);
}

#[test]
fn an_empty_list_is_a_catalogue_with_no_models() {
    assert_eq!(
        parse_catalogue(br#"{"data":[]}"#).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn unknown_members_at_every_level_are_ignored() {
    let body = br#"{"extra":1,"data":[{"id":"m","owned_by":"x","nested":{"k":[1]}}],"more":null}"#;
    assert_eq!(parse_catalogue(body).unwrap(), ["m"]);
}

#[test]
fn every_other_shape_is_malformed_and_gives_no_partial_list() {
    let cases: [(&[u8], &str); 7] = [
        (b"", "the catalogue is not valid JSON"),
        (b"{not json", "the catalogue is not valid JSON"),
        (b"[]", "the catalogue is not an object"),
        (b"{}", "the catalogue has no data array"),
        (br#"{"data":{"id":"m"}}"#, "the catalogue has no data array"),
        (
            br#"{"data":[{"id":"ok"},{"name":"x"}]}"#,
            "a catalogue entry has no string id",
        ),
        (
            br#"{"data":[{"id":7}]}"#,
            "a catalogue entry has no string id",
        ),
    ];
    for (body, why) in cases {
        assert_eq!(parse_catalogue(body), Err(why), "{body:?}");
    }
}
