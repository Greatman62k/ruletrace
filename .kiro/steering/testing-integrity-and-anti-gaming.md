# RuleTrace — Testing Integrity and Anti-Gaming Policy

## 1. Purpose

RuleTrace is a correctness-critical compiler. Its tests are evidence that its guarantees hold.

A green test result is not sufficient evidence of correctness if the test is weak, circular, misleading, incomplete or deliberately altered to accommodate a defective implementation.

All human and AI coding agents must follow this policy.

Task completion must be based on genuine satisfaction of requirements and verified behavior, not merely on obtaining passing test output.

## 2. Tests Must Detect Defects

1. Every test must contain meaningful assertions about observable behavior or a documented invariant.
2. Empty tests, placeholder tests and tests with no meaningful assertions are prohibited.
3. Tautological assertions such as `assert!(true)` and `assert_eq!(x, x)` are prohibited unless the equality itself is independently derived from a property being tested.
4. A test must not pass merely because an operation did not panic or returned a non-empty value when more specific behavior is required.
5. Tests must verify exact values, diagnostic codes, severity, locations, exit codes and relevant state changes where specified by the requirements.
6. Tests must exercise production code through the intended public API or executable workflow wherever practical.
7. A test must not duplicate the production implementation to calculate its expected output.
8. Each test must be traceable to an acceptance criterion, invariant, regression or documented risk.

## 3. Never Weaken Tests to Obtain a Passing Result

Agents must not:

- Remove or relax an assertion merely because implementation fails it.
- Replace exact expected values with broad conditions without an approved specification change.
- Reduce required property-test case counts.
- Reduce generated input diversity to avoid a failure.
- Change an expected diagnostic code or severity to accommodate incorrect behavior.
- Delete difficult negative tests.
- Add ignored, skipped or conditional test behavior to hide a failure.
- Modify golden fixtures automatically without review.
- Convert an unexpected error into a successful default value.
- Swallow panics, errors or failed assertions.
- Mark a test as optional to avoid fixing an implementation defect.

If a requirement is wrong or inconsistent, the agent must report the specification defect and request an explicit decision. It must not silently weaken either the requirement or its test.

## 4. Regression Testing — Red/Green Requirement

Every defect fix must include a regression test.

The required process is:

1. Identify and document the original defect.
2. Write a test that reproduces the defect.
3. Demonstrate that the test fails against the defective implementation.
4. Implement the correction.
5. Demonstrate that the same test passes against the corrected implementation.
6. Run relevant existing tests to identify regressions.

When reproducing the original implementation is impractical, use a controlled mutant or equivalent negative test to demonstrate that the regression test detects the defect.

A regression test is incomplete if it has never been shown to detect the behavior it claims to prevent.

## 5. Independent Expected Results

Expected outputs must have an independent basis.

Acceptable sources include:

- Independently calculated expected values.
- Hand-verified mathematical examples.
- Official source documents.
- Independently implemented reference models.
- Reviewed golden fixtures.
- Published standards and documented invariants.

The implementation under test must not generate its own expected result.

For numeric operations, use an independent reference implementation with broader or different internal arithmetic where practical.

For compiler and execution behavior, assert the expected decisions, calculations, diagnostics and traces independently of the production pipeline.

## 6. Negative and Adversarial Testing

Every supported operation must have meaningful negative tests where applicable.

RuleTrace must test:

- Malformed and truncated input.
- Invalid types and undeclared references.
- Unsupported constructs.
- Ambiguous and incomplete interpretations.
- Missing values and three-valued logic.
- Conflicting rules.
- Numeric overflow and division by zero.
- Invalid source spans and changed source hashes.
- Invalid and stale approval records.
- Rejected and Proposed revisions.
- Concurrent review operations.
- Transaction interruption and recovery.
- Malformed, oversized and adversarial AI responses.
- Prompt-injection instructions inside source documents.
- Credential leakage and unexpected network behavior.

Tests must assert the expected failure behavior, not merely that something failed.

## 7. Property-Based Testing Integrity

Property tests must:

1. Generate meaningful, varied inputs.
2. Cover valid, invalid and boundary cases.
3. Check the actual invariant rather than a weaker substitute.
4. Preserve the required minimum case count.
5. Report the seed and minimized failing case.
6. Avoid assumptions that exclude difficult but valid inputs.
7. Include negative mutations that demonstrate the property would detect a violation.

The required minimum of 256 generated cases must not be reduced without an explicit specification decision.

## 8. Mutation Testing

Critical components must be subjected to mutation testing.

Mutations should include changes such as:

- Reversing comparisons.
- Removing validation checks.
- Omitting provenance verification.
- Treating missing values as defaults.
- Changing exact arithmetic behavior.
- Removing tie-breaking criteria.
- Allowing invalid approval transitions.
- Changing diagnostic severity.
- Permitting unsupported rules to compile.

A mutant is killed when a relevant test detects the changed behavior.

A surviving mutant must be investigated and classified as:

- A genuine test coverage gap.
- An equivalent mutation.
- A mutation outside the component's contract.
- An unresolved test-quality concern.

Do not claim mutation coverage that has not actually been measured.

## 9. Golden Fixture Integrity

Golden outputs are reviewed specifications, not disposable generated files.

When a golden output changes, the agent must:

1. Identify the reason for the change.
2. Show the old and new output.
3. Identify affected acceptance criteria.
4. Demonstrate that the new output is correct.
5. Preserve unrelated fixture contents.
6. Obtain approval for intentional changes to authoritative expected results.

Automatic snapshot updating must never be used as a substitute for reviewing a behavioral change.

## 10. Mock and Test Double Integrity

Mocks must simulate external behavior, not make production logic automatically pass.

- Mocks must have explicit contracts.
- Failure injection must exercise real production error-handling paths.
- Provider mocks must support malformed responses, timeouts and authentication failures.
- Filesystem mocks must simulate partial writes, crashes, locking and recovery.
- Test doubles must not bypass approval, validation or provenance requirements.

## 11. Test Failure Reporting

Tests must distinguish:

- Passed.
- Failed.
- Skipped.
- Ignored.
- Inconclusive.
- Not run.
- Blocked by an unresolved decision.

A skipped or ignored test is never a passing test.

A task cannot be marked complete when a required test is failing, skipped without authorization or not executed.

When tests cannot be run, the agent must report exactly what was not verified and why.

## 12. Independent Review Before Task Completion

Before reporting a task complete, the agent must report:

- Requirements addressed.
- Production files changed.
- Tests added or modified.
- Evidence that new regression tests detect the relevant defect.
- Commands executed and their exit codes.
- Relevant test results.
- Any skipped or inconclusive tests.
- Any assertions or golden files changed.
- Remaining limitations.

Test modifications must receive particular scrutiny.

A task that changes production code and weakens its tests must be treated as incomplete until independently justified.

A separate reviewer agent should inspect tests and acceptance criteria after implementation agents finish. The implementation agent must not be the sole judge of its own work.

## 13. Task Completion Rule

An agent must never mark a task complete merely because its tests pass. It must demonstrate that the tests meaningfully detect the behavior being tested. Any weakened assertion, skipped test, changed golden output or changed expected result must be disclosed and justified.

## 14. Final Principle

The objective is not to maximize green checkmarks.

The objective is to make it difficult for incorrect behavior to survive testing.

A smaller number of strong, independently justified tests is more valuable than a large number of superficial passing tests.

Never trade correctness for task completion.

For RuleTrace, green tests are evidence, not a reward.
