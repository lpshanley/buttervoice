# Changesets

ButterVoice uses Changesets to drive release PRs on GitHub.

Add a changeset for any PR that changes shipped behavior, user-visible copy, bundled assets, or release artifacts:

```bash
pnpm changeset
```

Skip the requirement only for repo-only work such as CI, docs, or editor config updates. Those PRs should carry the `release:none` label so the changeset CI check can pass without a version note.

The GitHub release flow is:

1. Feature PRs merge into `main` with changesets.
2. The `release-plan` workflow opens or updates a release PR.
3. Merging the release PR updates app versions and changelog files.
4. A maintainer pushes a `vX.Y.Z` tag on that merge commit to publish the signed release.
