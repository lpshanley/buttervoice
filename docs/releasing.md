# Releasing ButterVoice

ButterVoice releases are GitHub-driven and tag-triggered.

## Day-to-day development

1. Add a changeset in any PR that changes shipped behavior, packaging, or release artifacts.
2. Use the `release:none` label only for CI, docs, or other repo-only work that should not appear in release notes.
3. Merge approved PRs into `main`.

## Release planning

1. The `release-plan` workflow runs on pushes to `main`.
2. If releasable changesets exist, it opens or updates the Changesets release PR.
3. That release PR is the only place versions are finalized.

## Publishing a stable release

1. Review and merge the release PR.
2. Confirm the merged commit is the exact release commit you want to publish.
3. Create and push the matching tag:

```bash
git checkout main
git pull --ff-only
git tag vX.Y.Z
git push origin vX.Y.Z
```

4. The `publish` workflow builds signed artifacts for Apple Silicon and Intel macOS, notarizes them, uploads them to GitHub Releases, and publishes updater metadata.
5. Verify the published updater manifest at [latest.json](https://github.com/lpshanley/buttervoice/releases/latest/download/latest.json).

## Required GitHub secrets

Create a protected `production` environment and add these secrets before enabling public releases:

- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_ID`
- `APPLE_PASSWORD`
- `APPLE_TEAM_ID`
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

`TAURI_SIGNING_PRIVATE_KEY` should be the contents of the local private key file created for ButterVoice. The matching public key is committed in Tauri config so update signatures can be verified by the app.
