# Approving proposals that depend on each other

`ruletrace-ai propose` checks each proposal on its own. It does not look at the
other proposals of the same batch or at proposals of other batches. A proposal
that reads a field declared by a sibling proposal is stored with these
Diagnostics:

- `RT-VAL-0002`, because the field is not declared in the Rulebook yet.
- `RT-AI-0305`, a Warning that the stored batch has proposals with blocking
  findings.

`ruletrace review approve` refuses such a proposal with `RT-APR-0205` and exit
code 3 until the sibling that declares the field is adopted. This is intended.
The refusal changes nothing: the Rulebook, the Approval_Ledger and the
Proposal_Store stay as they were.

## What to do

Approve the proposals in dependency order. Approve the proposal that declares
the field first, then run `review approve` again for the proposal that reads it.
The second approval validates the Rulebook as it is after the first one, so it
no longer finds the field missing.

If you reject the declaring proposal, the proposal that reads the field cannot
be approved. Reject it as well, or ask for a new proposal.

## Limit

The check is not aware of siblings. A later version might check a proposal
against the declarations of its siblings and report the dependency before
approval. This version does not.
