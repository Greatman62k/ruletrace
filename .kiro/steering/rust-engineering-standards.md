# Rust Formatting and Comment Standards

All Rust code uses the official formatter, `rustfmt`. These rules apply to
every task and every agent.

## Formatting

1. Format with `cargo fmt --all`.
2. CI enforces `cargo fmt --all -- --check`.
3. Use the pinned stable Rust toolchain.
4. Do not use Prettier or third-party formatting plugins for `.rs` files.
5. Do not use unstable `rustfmt` configuration options.
6. Run formatting before submitting completed work.

## Comments

RuleTrace code stays clean, professional and readable.

1. No long inline or trailing comments after executable statements.
2. Put implementation comments on their own lines above the code.
3. Keep comments concise, generally one to three lines.
4. Explain why a decision, invariant, constraint or non-obvious operation
   exists. Do not narrate what the code visibly does.
5. Do not comment self-explanatory declarations, assignments, calls or
   control flow.
6. Do not repeat function names, type names or implementation details.
7. Use `///` for public function, type, trait and item documentation.
8. Use `//!` for crate-level and module-level documentation.
9. Use `//` for non-public implementation explanations.
10. Prefer line comments over block comments.
11. Keep ordinary comment lines within about 80 characters.
12. A longer explanation needs a genuinely complex invariant or algorithm.
13. Put detailed algorithm descriptions, architecture decisions and long
    examples in the documentation files, not in source.
14. Do not claim guarantees that the code or tests do not establish.
15. Remove obsolete, contradictory or speculative comments when refactoring.
16. Do not use comments in place of good names, types, function boundaries
    or tests.

## Review before completing a task

- Run `rustfmt` on the files you changed.
- Inspect the comments you added and remove redundant trailing ones.
- Check that public API documentation is accurate.
- Confirm comments explain non-obvious decisions rather than repeat code.

## Concurrent work

- Format only the files you completed, for example
  `rustfmt --edition 2021 <files>` or `cargo fmt -p <crate>`.
- Do not run `cargo fmt --all` while other agents are editing, and do not
  reformat files another agent is modifying.
- Each phase checkpoint runs one central workspace-wide
  `cargo fmt --all -- --check`.

## Principle

Code explains itself through names, types, function boundaries and tests.
Comments keep the reasoning that the code cannot show.
