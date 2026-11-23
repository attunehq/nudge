# Sweep new rules over registry crates by copying, not symlinking

## What went wrong

To sweep a new starter rule for false positives, I symlinked crate sources from
`~/.cargo/registry/src/*/` into a scratch repo and ran `nudge check src`. It
failed with "check path contains no files": the file walker does not follow
symlinked directories.

## Fix

Copy the files instead of linking them. Put a `.nudge.yaml` holding only the
`version:` header and the rule under test in a scratch dir, run `git init`, then
copy the newest version of each registry crate's `.rs` files with
`rsync -aL --include='*/' --include='*.rs' --exclude='*' --prune-empty-dirs`.
`nudge check crates` then reports `path:line [rule]` per hit. About 16k files
take around three minutes with a debug build. Review a random sample of hits by
hand for whether the suggested rewrite is correct, and list the arguable ones in
the rule's Limits comment.

Also run the full `.nudge.yaml` with the new rule over this repo and compare the
issue count against `origin/main`, so the dogfood config adds no new findings.

## Verification

The `prefer-collect-over-push-loop` sweep (PR #101) matched 113 times across
604 crates this way.
