status: Confirmed

# UD-3: Exact_Number representation

Source: design §4.2.1 to §4.2.8 and §9.2; requirements UD-3 and Req 8.

## Decision

The user accepted the recommendation: **R-128, a bounded rational with decimal literals** and canonical serialization.

- Representation: sign plus magnitude, `num: u128`, `den: u128`, in lowest terms, positive denominator, zero is `0/1`.
- No dependency. No floating point anywhere in Rule_IR, compilation or execution.
- Rounding happens only at an explicit `round` node or `rounding` field (modes `half_even`, `half_away_from_zero`, `toward_zero`; places 0 to 28).
- Division is exact for a nonzero divisor. Zero divisor gives `DivisionByZero`.

## ExactNumber_Limits constants

| Constant | Value |
| --- | --- |
| Numerator magnitude, maximum | 2^127 - 1 (170141183460469231731687303715884105727) |
| Denominator, range | 1 to 2^127 - 1 |
| Literal significant digits, maximum | 38 |
| Literal fractional digits, maximum | 28 |
| Serialized decimal fractional digits, maximum | 126 |
| Rounding places | 0 to 28 |
| Checked intermediate width | 256 bits (the widest intermediate is 255 bits plus sign) |
| Overflow behaviour | Checked per operation. `Overflow` becomes an Indeterminate Outcome with `RT-EXE-0003` naming the operation, the rule and the canonical operands. |

A 38-digit literal with scale 28 fits: numerator below 10^38 < 2^127, denominator 10^28 < 2^94.

## Options compared (design §4.2.1)

Options: D = bounded decimal (38 digits, scale 0 to 28). R-128 = bounded rational, 127-bit. R-Big = bounded rational, 256-bit. H = hybrid (decimal at the boundary, rational inside one expression).

| Dimension | D | R-128 (chosen) | R-Big | H |
| --- | --- | --- | --- | --- |
| Exactness of + - x | Exact or Indeterminate | Exact or Indeterminate | Exact or Indeterminate | Exact inside, decimal at boundary |
| Division | Exact only if terminating, so each division declares scale and mode and rounds once | Exact always (nonzero divisor); no rounding | Exact always | Exact inside, rounded at boundary |
| `1/3 + 1/3 + 1/3` | Needs declared scale, result 0.999... or rounded | Exactly 1 | Exactly 1 | Exactly 1 inside one expression only |
| Weighted score with weights 1/3 | Weights are rounded decimals; sum of weights is not 1 | Exact | Exact | Exact only within one expression; a stored weight is decimal |
| Range and scale | 38 digits and scale 28; fixed, documented | Numerator and denominator at most 2^127 - 1 (about 1.7 x 10^38). No scale limit; value bound only | At most 2^256 - 1 (about 1.2 x 10^77) | Two limits |
| Decimal literal capacity | By definition | 38-digit literal with scale 28 fits | Fits | Fits |
| Rounding | Only at declared points; every division is one | Only at an explicit `round` node or `rounding` field | Same | Both |
| Overflow protection | Checked per operation; product needs 76 digits intermediate | Checked per operation; intermediate up to 255 bits | Unbounded intermediates, bound check on the normalized result | Two checks plus conversion rule |
| Canonical serialization | One decimal string | Decimal string when the denominator has only factors 2 and 5, else `n/d` (Req 8.24) | Same | Decimal in files, both forms in traces |
| Comparison with Published_Result decimals | Exact. Differences only if declared rounding differs | Exact. A non-terminating computed value never equals a decimal; report says so (Req 8.26) | Same | Mixed |
| Dependency cost | None (hand-rolled i128 plus 256-bit multiply) or a decimal crate. `rust_decimal` has a 96-bit coefficient (about 28 digits) and cannot hold 38 digits; unsuitable | None (hand-rolled) | `num-bigint` and `num-rational` (several crates), allocation, `no_std` and maintenance to verify | Both costs, largest test surface |
| Performance | Fast, fixed-size | Fast: binary gcd on 128-bit values, wide steps only for add and subtract numerators | Slower, allocates | Mixed |
| Determinism and portability | Integer only | Integer only | Integer only | Integer only |
| Test surface | Moderate | Moderate (wide helpers differentially tested) | Small (library) but dependency | Largest |
| Fit with published procedures that round each step | Natural | Needs explicit `round` nodes (same as D for steps) | Same | Both |

## Recommendation and accepted costs

R-128, for these reasons: (1) it matches the user's preference for rational; (2) division and weighted scores stay exact with no hidden rounding, so a rounding appears in a Rule only where a regulation states it; (3) no dependency; (4) the 38-digit literal limit of Req 8.27 fits inside the bound; (5) a differential test against a bignum model covers the narrow wide-arithmetic surface.

Accepted costs:

- Chains of unrelated denominators can reach the bound and yield an Indeterminate Outcome (R-7, R-D8).
- Non-terminating values print as fractions (`n/d`).
- The author must declare the rounding that a published decimal uses (R-16).

Fallback: R-Big needs only a swap inside `rt-numeric`, a new `ExactNumber_Limits` constant and a baseline change. D is fully specified in §4.2.8 and remains valid.

Re-examination trigger: a target rulebook whose denominators exceed the bound, or a user decision for D.

## Requirement 8 criteria that differ (design §4.2.7)

| Criterion | Under R-128 (chosen) | Under D (not chosen) |
| --- | --- | --- |
| 3.6, 4.4, 8.7, 8.27 literal syntax and limits | 38 digits, 28 fractional | Identical |
| 8.4 rounding declaration missing | Error only for a `round` node or `rounding` field without mode or places (`RT-VAL-0020`) | Also for any `div` node without scale and mode |
| 8.20 | Not applicable | `RT-VAL-0020` for a division without rounding |
| 8.5, 8.11 limits and range | Numerator and denominator bound; `Overflow` gives Indeterminate with `RT-EXE-0003` naming operation, rule and canonical operands | 38 digits, scale 28 after trailing-zero removal |
| 8.13 | Not applicable | Quotient rounded once at the declared scale and mode |
| 8.21 | Exact normalized quotient, no rounding at division | Not applicable |
| 8.16, 8.24, 8.25 serialization | Decimal if terminating, else fraction; fraction parsed only in serialized artifacts | One decimal string |
| 8.22, 8.23 | Lowest terms, positive denominator, 0/1; 256-bit checked intermediates | Not applicable |
| 8.26, 13.12 published comparison | Exact; non-terminating note for fraction values | Exact; no note needed |
| 13.11 replay | Parses decimals and fractions | Decimals |
| 20.22 differential test | Bounds at 2^127 - 1 | 38 digits, scale 28 |
| R-7 | Denominator growth | Digit and scale limits |
| Identical under both | 8.1 to 8.3, 8.6, 8.8 to 8.10, 8.12, 8.14, 8.15, 8.17, 8.18, 8.28 to 8.31 | same |

## Schema points that change with the choice (design §4.2.8)

1. The `binary` node with `op: "div"` gains required `rounding: {mode, places}` under D. Under R-128 it does not.
2. The `numeric` limits constant (the table above for R-128).
3. The canonical fraction form `n/d` exists only under R. The fraction literal `"1/3"` is not accepted in Rule_IR (Req 8.25, RN-21); a weight of 1/3 is written as a `div` node.

Everything else in §4.1 is common.

## Decision record (design §4.2.6)

| Item | Record |
| --- | --- |
| Decision | Bounded rational R-128 with decimal literals and canonical serialization |
| Status | Confirmed by the user (accepted the recommendation) |
| Options considered | D, R-128, R-Big, H, `rust_decimal`, `bigdecimal` |
| Deciding dimensions | Exactness of division and weights; range and scale; rounding visibility; overflow control; canonical serialization; dependency and performance cost; comparison with published decimals |
| Bounds | Numerator and denominator at most 2^127 - 1; literals at most 38 digits and 28 fractional digits |
| Reversibility | Swap inside `rt-numeric`; schema points above change IR_Version if D is chosen |
| Re-examination trigger | A target rulebook whose denominators exceed the bound, or a user decision for D |
| Test obligation | Differential property test against `num-bigint` (dev-only) at and beyond the bounds, 256 cases per operation (Req 20.22). The pending-period test against both D and R-128 is no longer required for the choice; keep D's test only if the user reopens the decision. |

## Follow-up

Steering item 3 of design §9.3 is a follow-up for task 8.3.
