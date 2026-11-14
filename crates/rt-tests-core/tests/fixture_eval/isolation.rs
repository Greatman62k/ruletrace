//! Isolation (Property 40, Req 20.6, 9.7): the Compiled_Rulebooks of different
//! fixtures are evaluated A then B then C, C then B then A, and interleaved per
//! Question in one process. Each result equals the solitary evaluation, and the
//! solitary evaluation of a fixture is repeatable.

use crate::load::{load, Fixture, EXECUTABLE};
use rt_engine::evaluate;

fn view(f: &Fixture, questions: &[rt_data::Question]) -> String {
    let r = evaluate(&f.compiled, &f.data, questions, None);
    format!("{:?}|{:?}|{:?}", r.outcomes, r.traces, r.diagnostics)
}

fn solo(f: &Fixture) -> String {
    view(f, &f.questions)
}

#[test]
fn fixtures_evaluate_equal_to_solitary_runs_in_any_order() {
    let fixtures: Vec<Fixture> = EXECUTABLE.iter().map(|n| load(n)).collect();
    let alone: Vec<String> = fixtures.iter().map(solo).collect();
    assert!(
        alone.windows(2).all(|w| w[0] != w[1]),
        "the fixtures must differ"
    );
    for order in [[0, 1, 2], [2, 1, 0], [1, 0, 2], [1, 2, 0]] {
        for i in order {
            assert_eq!(
                solo(&fixtures[i]),
                alone[i],
                "{} order {order:?}",
                fixtures[i].name
            );
        }
    }
}

#[test]
fn fixtures_evaluated_interleaved_per_question_equal_solitary_runs() {
    let fixtures: Vec<Fixture> = EXECUTABLE.iter().map(|n| load(n)).collect();
    let longest = fixtures.iter().map(|f| f.questions.len()).max().unwrap();
    let mut outcomes = vec![Vec::new(); fixtures.len()];
    let mut traces = vec![Vec::new(); fixtures.len()];
    for q in 0..longest {
        for (i, f) in fixtures.iter().enumerate() {
            if let Some(question) = f.questions.get(q) {
                let r = evaluate(&f.compiled, &f.data, std::slice::from_ref(question), None);
                outcomes[i].extend(r.outcomes);
                traces[i].extend(r.traces);
            }
        }
    }
    for (i, f) in fixtures.iter().enumerate() {
        let whole = evaluate(&f.compiled, &f.data, &f.questions, None);
        assert_eq!(outcomes[i], whole.outcomes, "{} outcomes", f.name);
        assert_eq!(traces[i], whole.traces, "{} traces", f.name);
    }
}
