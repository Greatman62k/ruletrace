# Requirement amendments awaiting user approval

Each entry gives the exact text proposed for the user to approve. Until the user
approves it, the requirement file is unchanged and the design behaviour is
implemented under the entry's reference.

## RN-26: Requirement 13 criterion 12 (published comparison of rankings)

Status: **proposed, not yet approved by the user.** Raised by task 9.2.
Design reference: §3.6.3 "Published comparison for rankings", §9.4 (RN-26).
Affected criteria: 13.12, 13.20, 31.7, 32.6.

Reason: Requirement 32 criterion 6 needs the engine's comparison to detect a
published tie that the computed result does not have and a computed Tie_Group
that the published file splits. Criterion 13.12 states only a consistency check
of values.

Proposed addition to criterion 12 (inserted after its first sentence):

> For a ranking Target the published value of a participant is a Rank_Position,
> and THE Rules_Engine SHALL compare it exactly with the computed Rank_Position
> of that participant. IF the published result gives different positions to
> participants of one computed Tie_Group, THEN THE Rules_Engine SHALL list one
> difference, of kind `tie_broken_without_declared_criterion`, for each
> participant of that Tie_Group whose published position differs from the
> shared computed position, with the computed Tie_Group members attached. IF
> the published result gives one position to participants that the computed
> result separates, THEN THE Rules_Engine SHALL list one difference, of kind
> `tie_not_in_computed_result`, for each such participant whose published
> position differs from its computed position, with the published tie members
> attached. Every other differing value is of kind `value_differs`. A
> participant that the published result does not list is not compared, and a
> computed position that is Indeterminate is never equal to a published value
> and never classifies a tie. Each difference SHALL carry the published value,
> the computed value and the reference to the Decision_Trace entry of the
> computed position.

Behaviour implemented (crate `rt-engine`, `compare.rs`):

- An entry gets one kind. A split computed Tie_Group takes precedence over a
  published tie.
- A split needs at least two distinct published positions among the listed
  members of the computed group. Members whose published position equals the
  computed position are not reported.
- A published tie is "separated" when another participant with the same
  published position has a Known computed position different from this
  participant's.
- Differences keep the participant then Target order. No tolerance, no
  rounding.

Open points for the user (the design text does not settle them):

1. Whether a participant whose published position equals its computed position
   but who is in a published tie that the computed result separates should also
   be reported. The implementation reports only participants whose value
   differs, which matches the wording "each member whose published position
   differs" of design §3.6.3.
2. The kind name for a plain value difference. The design names only the two
   new kinds, and the implementation uses `value_differs`.
3. No new catalogue code was added: each difference still uses
   `RT-EXE-0011` (Warning). The design names no further code.
