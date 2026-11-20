//! Source document and rulebook of the application golden cases that need a
//! ranking clause and a qualification threshold (task 9.16 follow-up).
//!
//! Each numbered clause states exactly one rule. The `slice-basic` source has
//! no ranking clause and no 7.7 threshold, so these rules cite this document.
//! The rulebook is the `slice-basic` rulebook with the document, its hash and
//! the spans changed, plus the test-local rule `elig.qual`.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

/// The document id of the application source.
pub const DOCUMENT_ID: &str = "app-rules.md";

/// The document text, four clauses of one rule each.
pub const TEXT: &str = "# Spring Open Rules\n\
\n\
1. A competitor is eligible if age is at least 16 and holds a licence.\n\
2. The score is 0.6 times the judges score plus 0.4 times the technical score.\n\
3. Competitors are ranked by score, highest first. Equal scores are separated by the higher judges score. Competitors equal on both share the same position, and the next position after a shared position advances by the number of competitors sharing it (for example 1, 1, 3).\n\
4. A competitor qualifies if the score is at least 7.7.\n";

/// One clause span: start, end (exclusive, without the newline), line and
/// column. Written by hand from the text above, not computed.
pub type Span = (usize, usize, u32, u32);

/// The spans of clauses 1 to 4, in order.
pub const SPANS: [Span; 4] = [
    (21, 91, 3, 1),
    (92, 170, 4, 1),
    (171, 445, 5, 1),
    (446, 501, 6, 1),
];

/// The rule ids in clause order.
pub const RULES: [&str; 4] = ["elig.basic", "score.weighted", "rank.standing", "elig.qual"];

/// The spans found from the text: each numbered line without its newline.
/// Offsets are bytes; the line and column are 1-based. Used to check the
/// hand-written table, never to replace it.
pub fn found_spans() -> Vec<Span> {
    let mut out = Vec::new();
    let mut offset = 0;
    for (index, line) in TEXT.split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches('\n');
        if body.starts_with(|c: char| c.is_ascii_digit()) {
            out.push((
                offset,
                offset + body.len(),
                u32::try_from(index + 1).unwrap(),
                1,
            ));
        }
        offset += line.len();
    }
    out
}

fn span_json(span: Span) -> String {
    format!(
        r#"{{"document":"{DOCUMENT_ID}","start":{},"end":{},"line":{},"column":{}}}"#,
        span.0, span.1, span.2, span.3
    )
}

/// The Rulebook JSON for a document with the given SHA-256. The rules and
/// their expressions equal those of `slice-basic` and of the former
/// test-local `elig.qual`; only the document, hash and spans differ.
pub fn rulebook_json(sha256: &str) -> String {
    let s = SPANS.map(span_json);
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"spring-open","version":"2026.1","documents":[{{"id":"{DOCUMENT_ID}","sha256":"{sha256}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}},{{"name":"licence","type":"boolean"}},{{"name":"judges","type":"number"}},{{"name":"technical","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{{"id":"elig.basic","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"and","t":"boolean","args":[{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"16"}}}},{{"node":"field","t":"boolean","name":"licence"}}]}},"markers":[],"spans":[{s0}]}},{{"id":"score.weighted","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"weighted_score","target":"score","type":"number","terms":[{{"weight":{{"node":"num","t":"number","value":"0.6"}},"value":{{"node":"field","t":"number","name":"judges"}}}},{{"weight":{{"node":"num","t":"number","value":"0.4"}},"value":{{"node":"field","t":"number","name":"technical"}}}}],"rounding":null,"markers":[],"spans":[{s1}]}},{{"id":"rank.standing","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"ranking","target":"standing","type":"ranking","key":{{"node":"ref","t":"number","target":"score"}},"direction":"desc","tie_breakers":[{{"expr":{{"node":"field","t":"number","name":"judges"}},"direction":"desc"}}],"ties":"group","markers":[],"spans":[{s2}]}},{{"id":"elig.qual","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"eligibility","target":"qualified","type":"boolean","expr":{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"ref","t":"number","target":"score"}},"right":{{"node":"num","t":"number","value":"7.7"}}}},"markers":[],"spans":[{s3}]}}],"precedence":[]}}"#,
        s0 = s[0],
        s1 = s[1],
        s2 = s[2],
        s3 = s[3],
    )
}
