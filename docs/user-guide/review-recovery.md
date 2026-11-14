# Review commits and recovery

`ruletrace review approve` and `ruletrace review reject` change the
Approval_Ledger. Approving a Pending_Proposal changes the ledger and the
Rulebook together. For that two-file change the command writes a journal next to
the ledger, `<ledger>.journal` (for example `ledger.jsonl.journal`), and removes
it when the change is complete.

## What you can see

| State | Meaning | What to do |
| --- | --- | --- |
| No journal | No commit is pending. | Nothing. |
| Journal exists | A commit was interrupted, or it finished and its cleanup failed. The two files may not belong together. | Run `review approve` or `review reject`. Recovery runs first. |
| `<ledger>.journal.tmp` exists | The attempt marker. A crash or a full disk can leave it torn. | See "A torn marker". |

## Which commands refuse

While a journal exists, `validate`, `review list` and `evaluate` with
`--proof-report` refuse with `RT-APR-0214` and exit code 2. They read no
Rulebook or ledger, and they change nothing. `compile` reads only the result
file of `validate`, so it is not affected by the journal itself.

The same code and exit code appear when another review command is running, or
when the Rulebook or the ledger changed while the command was reading them.
Run the command again when no review command is running.

## Advisory locks are required

Every command that reads the Rulebook and the ledger together (`validate`,
`review list`, `evaluate` with `--proof-report`) and every review command
(`review approve`, `review reject` and the recovery they run) needs an advisory
file lock on `<ledger>.lock`. If the file system or the platform does not
support advisory locks, the command refuses with `RT-APR-0215` and exit code 70.
The message names the path and says that the operation refused because advisory
locks are required. The command reads nothing and changes nothing, and it does
not fall back to a lock file.

Some network file systems and some mounts do not support advisory locks. Move
the Rulebook and the ledger to a local file system that does. `RT-APR-0212`
(exit 70) is a different case: another review command holds the lock, so run
the command again later. Readers report that case as `RT-APR-0214`.

## How recovery runs

Every `review approve` and `review reject` takes the review lock and then
recovers an existing journal before it reads anything else. Recovery finishes or
undoes the interrupted commit and reports what it did in a warning
(`RT-APR-0211`):

- Both files already carry the new state: leftover files are removed.
- The ledger is new, the Rulebook is old, and the new Rulebook file exists: the
  commit is completed.
- The ledger is new, the Rulebook is old, and no new Rulebook file exists: the
  ledger is restored from its backup, and the commit is undone.
- Both files are old: leftover files are removed.

Any other state, a damaged backup, or a journal that does not name the files of
the command gives `RT-APR-0208` (exit 70) and changes nothing. A command that
does something else (for example one that is refused because the rule does not
exist) still runs recovery first.

If a commit fails and its rollback fails too, the message says which case holds:
the files were restored, both files carry the new state (the decision is
committed), or the files are in an intermediate state that must not be used. In
the last case the next `review approve` or `review reject` either completes the
commit or undoes it.

## A torn marker

A process kill or a full disk can leave `<ledger>.journal.tmp` incomplete. The
next review command refuses it, keeps it, and names it in the message.

Remove it by hand only when no review command is running and no
`<ledger>.journal` exists:

1. Delete `<ledger>.journal.tmp`.
2. Delete any `<ledger>.new`, `<ledger>.old`, `<rulebook>.new` and
   `<rulebook>.old` that you did not create yourself.
3. Run the review command again.

If `<ledger>.journal` exists, do not delete anything: run `review approve` or
`review reject` and let recovery handle it.
