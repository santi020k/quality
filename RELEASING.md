# Releasing

1. Merge user-visible changes with a Changeset.
2. The Release PR workflow maintains `chore(release): version packages`.
3. Review the generated versions and changelogs, then merge that pull request.
4. The Release workflow validates the repository, builds and smoke-tests every
   native archive, publishes the GitHub release, and verifies the Action.
5. Confirm the installer and Action examples against the published tag.

Do not create release tags manually. The workflow creates `vX.Y.Z` only after
all native archives pass their smoke tests.
