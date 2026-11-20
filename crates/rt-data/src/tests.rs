use super::*;
use crate::proptest_floor::property_tests;
use proptest::prelude::*;
use rt_foundation::{Code, InputId, Location};
use std::collections::{BTreeMap, BTreeSet};

fn input() -> InputId {
    InputId::new("data.json").unwrap()
}

fn pid(s: &str) -> ParticipantId {
    ParticipantId::new(s).unwrap()
}

fn tn(s: &str) -> TargetName {
    TargetName::new(s).unwrap()
}

fn data(text: &str) -> Result<CompetitionData, rt_foundation::DiagnosticState> {
    parse_data(&input(), text.as_bytes(), DataLimits::default())
}

fn codes(r: Result<impl std::fmt::Debug, rt_foundation::DiagnosticState>) -> Vec<Code> {
    r.unwrap_err().as_slice().iter().map(|d| d.code()).collect()
}

fn wrap(participants: &str) -> String {
    format!("{{\"format\":\"ruletrace-data\",\"version\":1,\"participants\":[{participants}]}}")
}

#[test]
fn missing_zero_and_empty_string_are_three_distinct_values() {
    let d = data(&wrap(
        r#"{"id":"a","fields":{"x":null}},{"id":"b","fields":{"x":"0"}},{"id":"c","fields":{"x":""}},{"id":"d"}"#,
    ))
    .unwrap();
    let x = |id: &str| d.participant(&pid(id)).unwrap().field("x").cloned();
    assert_eq!(x("a"), None);
    assert_eq!(x("b"), Some(DataValue::Text("0".into())));
    assert_eq!(x("c"), Some(DataValue::Text(String::new())));
    assert_eq!(x("d"), None);
    // false is present too
    let d = data(&wrap(r#"{"id":"a","fields":{"x":false}}"#)).unwrap();
    assert_eq!(
        d.participant(&pid("a")).unwrap().field("x"),
        Some(&DataValue::Bool(false))
    );
}

#[test]
fn absent_collection_differs_from_empty_collection() {
    let d = data(&wrap(r#"{"id":"a","collections":{"r":[]}},{"id":"b"}"#)).unwrap();
    assert_eq!(
        d.participant(&pid("a")).unwrap().collection("r"),
        Some(&[][..])
    );
    assert_eq!(d.participant(&pid("b")).unwrap().collection("r"), None);
}

#[test]
fn canonical_bytes_are_order_independent_and_round_trip() {
    let a = data(&wrap(
        r#"{"id":"b","fields":{"y":"1","x":true},"collections":{"c":[{"k":"2"},{"k":"1"}]}},{"id":"a"}"#,
    ))
    .unwrap();
    let b = data(&wrap(
        r#"{"id":"a","fields":{},"collections":{}},{"id":"b","collections":{"c":[{"k":"1"},{"k":"2"}]},"fields":{"x":true,"y":"1"}}"#,
    ))
    .unwrap();
    assert_eq!(serialize_data(&a), serialize_data(&b));
    let bytes = serialize_data(&a);
    let again = parse_data(&input(), &bytes, DataLimits::default()).unwrap();
    assert_eq!(again, a);
    assert_eq!(serialize_data(&again), bytes);
}

#[test]
fn json_numbers_and_nested_values_are_refused_with_a_location() {
    let state = data(&wrap(r#"{"id":"a","fields":{"x":5}}"#)).unwrap_err();
    let d = &state.as_slice()[0];
    assert_eq!(d.code(), Code::DataMalformed);
    match d.primary() {
        Location::Input(l) => {
            assert_eq!(l.line, Some(1));
            assert_eq!(l.field.as_deref(), Some("participants[0].fields.x"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        codes(data(&wrap(r#"{"id":"a","fields":{"x":1.5}}"#))),
        vec![Code::DataMalformed]
    );
    assert_eq!(
        codes(data(&wrap(r#"{"id":"a","fields":{"x":["1"]}}"#))),
        vec![Code::DataMalformed]
    );
}

#[test]
fn duplicate_participant_names_the_first_as_related() {
    let state = data(&wrap(r#"{"id":"a"},{"id":"b"},{"id":"a"}"#)).unwrap_err();
    assert_eq!(state.as_slice().len(), 1);
    let d = &state.as_slice()[0];
    assert_eq!(d.code(), Code::DataDuplicateParticipant);
    assert_eq!(d.related().len(), 1);
}

#[test]
fn malformed_shapes_report_data_malformed() {
    assert_eq!(codes(data("{")), vec![Code::DataMalformed]);
    assert_eq!(
        codes(data(r#"{"format":"other","version":1,"participants":[]}"#)),
        vec![Code::DataMalformed]
    );
    assert_eq!(
        codes(data(&wrap(r#"{"id":""}"#))),
        vec![Code::DataMalformed]
    );
    assert_eq!(
        codes(data(&wrap(r#"{"id":"a","extra":1}"#))),
        vec![Code::DataMalformed]
    );
}

#[test]
fn flag_keys_are_rejected_before_anything_else() {
    let c = codes(data(&wrap(r#"{"id":"a","fields":{"has_errors":false}}"#)));
    assert_eq!(c, vec![Code::DiagnosticStateContradiction]);
    let c = codes(data(&wrap(r#"{"id":"a","fields":{"is_approved":true}}"#)));
    assert_eq!(c, vec![Code::LifecycleFlagContradiction]);
}

#[test]
fn limits_accept_equal_and_reject_greater() {
    let limits = DataLimits {
        max_participants: 2,
        max_elements_per_participant: 2,
        ..DataLimits::default()
    };
    let parse = |s: &str| parse_data(&input(), s.as_bytes(), limits);
    assert!(parse(&wrap(r#"{"id":"a"},{"id":"b"}"#)).is_ok());
    assert_eq!(
        codes(parse(&wrap(r#"{"id":"a"},{"id":"b"},{"id":"c"}"#))),
        vec![Code::LimitCountExceeded]
    );
    assert!(parse(&wrap(r#"{"id":"a","collections":{"c":[{},{}]}}"#)).is_ok());
    assert_eq!(
        codes(parse(&wrap(
            r#"{"id":"a","collections":{"c":[{}],"d":[{},{}]}}"#
        ))),
        vec![Code::LimitCountExceeded]
    );
}

#[test]
fn file_size_and_depth_limits_keep_their_codes() {
    let tiny = DataLimits {
        parse: rt_json::ParseLimits::new(10, 64),
        ..DataLimits::default()
    };
    assert_eq!(
        codes(parse_data(&input(), wrap("").as_bytes(), tiny)),
        vec![Code::LimitFileTooLarge]
    );
    let shallow = DataLimits {
        parse: rt_json::ParseLimits::new(1 << 20, 2),
        ..DataLimits::default()
    };
    assert_eq!(
        codes(parse_data(
            &input(),
            wrap(r#"{"id":"a"}"#).as_bytes(),
            shallow
        )),
        vec![Code::LimitNestingTooDeep]
    );
}

#[test]
fn questions_round_trip_and_reject_bad_input() {
    let qs = vec![
        Question::Value {
            target: tn("eligible"),
            participant: pid("p1"),
        },
        Question::Rank {
            target: tn("rank.total"),
            participants: None,
        },
        Question::Rank {
            target: tn("rank.total"),
            participants: Some([pid("p2"), pid("p1")].into_iter().collect()),
        },
    ];
    let bytes = serialize_questions(&qs);
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.find("\"p1\"").unwrap() < text.find("\"p2\"").unwrap());
    let back = parse_questions(&input(), &bytes, default_parse_limits()).unwrap();
    assert_eq!(back, qs);
    let bad = |s: &str| {
        codes(parse_questions(
            &input(),
            s.as_bytes(),
            default_parse_limits(),
        ))
    };
    let head = r#"{"format":"ruletrace-questions","version":1,"questions":"#;
    assert_eq!(
        bad(&format!(r#"{head}[{{"kind":"sum"}}]}}"#)),
        vec![Code::DataMalformed]
    );
    assert_eq!(
        bad(&format!(
            r#"{head}[{{"kind":"rank","target":"t","participants":["a","a"]}}]}}"#
        )),
        vec![Code::DataMalformed]
    );
    assert_eq!(
        bad(&format!(r#"{head}[{{"kind":"value","target":"t"}}]}}"#)),
        vec![Code::DataMalformed]
    );
}

#[test]
fn default_questions_follow_the_documented_order() {
    let d = data(&wrap(r#"{"id":"p2"},{"id":"p1"}"#)).unwrap();
    let values: BTreeSet<_> = [tn("score"), tn("eligible")].into_iter().collect();
    let ranks: BTreeSet<_> = [tn("rank")].into_iter().collect();
    let qs = default_questions(&d, &values, &ranks);
    let names: Vec<String> = qs
        .iter()
        .map(|q| match q {
            Question::Value {
                target,
                participant,
            } => format!("v:{participant}:{target}"),
            Question::Rank { target, .. } => format!("r:{target}"),
        })
        .collect();
    assert_eq!(
        names,
        [
            "v:p1:eligible",
            "v:p1:score",
            "v:p2:eligible",
            "v:p2:score",
            "r:rank"
        ]
    );
}

#[test]
fn published_round_trip_and_exactness() {
    let text = r#"{"format":"ruletrace-published","version":1,"entries":[
        {"participant":"p2","target":"score","value":"7.50"},
        {"participant":"p1","target":"eligible","value":false},
        {"participant":"p1","target":"rank","value":"1"}]}"#;
    let p = parse_published(&input(), text.as_bytes(), default_parse_limits()).unwrap();
    let keys: Vec<_> = p
        .entries()
        .keys()
        .map(|(a, b)| format!("{a}/{b}"))
        .collect();
    assert_eq!(keys, ["p1/eligible", "p1/rank", "p2/score"]);
    let bytes = serialize_published(&p);
    assert!(String::from_utf8(bytes.clone())
        .unwrap()
        .contains("\"7.5\""));
    let again = parse_published(&input(), &bytes, default_parse_limits()).unwrap();
    assert_eq!(again, p);
    assert_eq!(serialize_published(&again), bytes);
}

#[test]
fn published_rejects_bad_numbers_and_repeats() {
    let bad = |value: &str| {
        let text = format!(
            r#"{{"format":"ruletrace-published","version":1,"entries":[{{"participant":"p","target":"t","value":{value}}}]}}"#
        );
        codes(parse_published(
            &input(),
            text.as_bytes(),
            default_parse_limits(),
        ))
    };
    for v in [
        r#""1e3""#, r#""+1""#, r#""NaN""#, r#""1/3""#, "5", r#""""#, "null",
    ] {
        assert_eq!(bad(v), vec![Code::DataMalformed], "{v}");
    }
    let dup = r#"{"format":"ruletrace-published","version":1,"entries":[
        {"participant":"p","target":"t","value":true},{"participant":"p","target":"t","value":false}]}"#;
    assert_eq!(
        codes(parse_published(
            &input(),
            dup.as_bytes(),
            default_parse_limits()
        )),
        vec![Code::DataMalformed]
    );
}

// Property 20 (parse side): distinct present values stay distinct, and
// canonical bytes are stable under participant and element order.
fn value_strategy() -> impl Strategy<Value = Option<DataValue>> {
    prop_oneof![
        Just(None),
        any::<bool>().prop_map(|b| Some(DataValue::Bool(b))),
        "[a-z0-9.\"\\\\ ]{0,6}".prop_map(|s| Some(DataValue::Text(s))),
    ]
}

fn fields_strategy() -> impl Strategy<Value = Element> {
    proptest::collection::btree_map("[a-z]{1,3}", value_strategy(), 0..4).prop_map(|m| {
        m.into_iter()
            .filter_map(|(k, v)| v.map(|v| (k, v)))
            .collect()
    })
}

fn participant_strategy() -> impl Strategy<Value = (Participant, Vec<Element>)> {
    (
        fields_strategy(),
        proptest::collection::vec(fields_strategy(), 0..4),
    )
        .prop_map(|(fields, elements)| {
            let mut collections = BTreeMap::new();
            collections.insert("c".to_owned(), elements.clone());
            (Participant::new(fields, collections), elements)
        })
}

property_tests! {
    #[test]
    fn data_round_trips_and_ignores_input_order(
        ps in proptest::collection::btree_map("[a-z0-9]{1,4}", participant_strategy(), 0..5)
    ) {
        let mut map = BTreeMap::new();
        let mut reversed = BTreeMap::new();
        for (id, (p, elements)) in &ps {
            map.insert(pid(id), p.clone());
            let mut rev = elements.clone();
            rev.reverse();
            let mut c = BTreeMap::new();
            c.insert("c".to_owned(), rev);
            reversed.insert(pid(id), Participant::new(p.fields().clone(), c));
        }
        let data = CompetitionData::new(map);
        let bytes = serialize_data(&data);
        prop_assert_eq!(&bytes, &serialize_data(&CompetitionData::new(reversed)));
        let back = parse_data(&input(), &bytes, DataLimits::default()).unwrap();
        prop_assert_eq!(&back, &data);
        prop_assert_eq!(serialize_data(&back), bytes);
    }
}
