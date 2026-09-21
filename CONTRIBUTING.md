# Contributing to Darkhorse

Use a short-lived branch and a pull request into `main` for each focused change. This follows [GitHub flow](https://docs.github.com/en/get-started/using-github/github-flow). Read the [engineering rules](docs/engineering.md) before changing behavior or architecture.

The repository is in early development. The project license remains undecided in [issue #1](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/1); this guide does not establish a license or a contributor agreement.

## Start with an issue

Search the [issues](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues) and open pull requests before starting. Reuse a matching issue or open one describing the problem, intended outcome, and observable completion criteria. For substantial features or changes to security, protocols, architecture, or public interfaces, agree on scope with a maintainer before implementation.

State which part you intend to work on so others can coordinate. Maintainers can assign accepted work; assignment and a project board are not prerequisites for opening a proposal. Split large issues into independently reviewable increments, record dependencies, and keep incomplete criteria visible. Public issues must include enough context to work without private planning files.

## Create a branch

Contributors without repository write access use a fork. Keep `origin` pointed at your fork and add the main repository as `upstream`:

```sh
git remote add upstream https://github.com/OneTesseractInMultiverse/darkhorse-identity.git
git fetch upstream
git switch -c docs/2-contribution-workflow upstream/main
```

The branch name above is an example: use `<type>/<issue-number>-<short-description>`, with a type such as `feat`, `fix`, `docs`, `test`, `refactor`, or `chore`. Maintainers may create the same kind of branch from an updated `origin/main` in the main repository. One issue may have several successive branches and pull requests for distinct increments. Keep unrelated work separate.

Update your branch from the current `main` before final validation. Merge the upstream changes into a shared branch. Rebase only a branch you own and nobody else depends on; coordinate with reviewers and use `--force-with-lease` if updating that branch after a rebase. Never force-push `main` or rewrite shared history.

## Implement and validate

Follow the toolchain and setup instructions in the [README](README.md#quick-start) and [development guide](docs/development.md). After dependency installation, the normal code-change check is:

```sh
make ci
```

This runs formatting, lint, type and architecture checks, self-contained unit tests, and release/static builds. The versioned [CI workflow](.github/workflows/ci.yaml) also runs source-package and release-tooling checks. A separate dependency qualification job runs strict live advisory scans. Record local validation and all hosted results in the pull request; a passing source check does not clear a blocked dependency gate. See [release qualification](docs/release-readiness.md) for commands, scope and outstanding requirements.

Start behavior changes with a meaningful failing test, then implement and refactor. Verify failures and security boundaries as well as successful results. Add checks at the affected boundary:

| Change                                       | Additional evidence                                                                      |
| -------------------------------------------- | ---------------------------------------------------------------------------------------- |
| PostgreSQL schema or transaction behavior    | `make test-postgres`; migration and rollback/failure behavior                            |
| Redis infrastructure or shared limiting      | `make test-redis`; relevant cross-process and failure scenarios                          |
| Login, authorization, or browser interaction | `make test-browser`; include screenshots for visible UI changes                          |
| Container packaging                          | `make docker-build` and relevant image smoke targets                                     |
| Coverage-sensitive executable logic          | Relevant coverage targets and their measured scope and limitations                       |
| Documentation or templates only              | `make fmt-check`, `git diff --check`, and changed links/commands; no full build required |

Integration and browser prerequisites are documented in the [persistence](docs/persistence.md), [Redis](docs/redis.md), and [authentication](docs/authentication.md) guides. Use `make help` for available commands. Unit tests must remain runnable without services, settings, or private files.

Include actual commands and results, including failures and skipped checks. Existing coverage qualification gaps are tracked in [issue #2](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/2); do not lower gates or describe a partial report as complete. Reviewers must assess affected uncovered paths and any regression before merging.

## Commit and open a pull request

Use your own Git author identity and a concise subject describing the change. Every commit must reference the owning issue, normally in the body:

```text
docs: explain contributor setup

Refs #2
```

In a fork, use `Refs OneTesseractInMultiverse/darkhorse-identity#2` to identify the upstream issue. Use the actual issue number. Inspect the staged diff; include only relevant files. Keep credentials, local state, and private planning out of commits. Commit messages describe the project change without generated attribution or tool branding.

Push your branch and open a pull request against this repository's `main`. Use a draft for early feedback. The [pull request template](.github/pull_request_template.md) asks for the problem, resulting behavior, validation, and remaining work. Explain security, compatibility, migration, or performance implications when relevant, and update public documentation with changed behavior.

Use `Refs #<number>` in partial pull requests. Use `Closes #<number>` only when merging the pull request satisfies every issue criterion and required check. GitHub interprets [closing keywords](https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/linking-a-pull-request-to-an-issue) when the change reaches the default branch. Do not use a closing link in the Development sidebar for partial work either. Keep the issue open and record what remains.

## Review and merge

A maintainer other than the author reviews the final change. Address review comments, resolve conversations, and rerun affected checks after changes. Code changes need passing `make ci`, applicable integration evidence, and all configured required checks. Document existing qualification gaps separately; new failures block merge.

The reviewer checks behavior, inward dependencies, separation of computations and coordination, security failures, test quality, and operational compatibility. A passing test suite alone does not establish protocol conformance or production readiness.

Prefer squash merge for one coherent increment. Before merging, inspect the final title, body, author, and issue reference; keep `Refs #<number>` or a justified closing reference in the resulting commit. Preserve the contributor's authorship. Maintainers may retain separate meaningful commits when that improves review or attribution. Delete the merged feature branch and update the issue with the result and any remaining criteria.

## Repository administration

Maintainers track hosted checks and repository enforcement in [issue #2](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/2) or a linked follow-up:

- Verify the hosted source/dependency checks for fork contributions and extend them with the agreed integration suites. Run untrusted changes without repository secrets or write credentials. Resolve qualification blockers and select stable, actually passing check names before making them required.
- Configure [`main` protection](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches) to require pull requests, at least one approval from another maintainer, review of subsequent changes, resolved conversations, and required checks against the current base. Appoint sufficient reviewers before requiring independent approval for maintainer changes.
- Block force pushes and deletion of `main` and apply the merge requirements to maintainers. Confirm the rules with a test pull request.
- Keep the contribution guide and open issue instructions aligned with the configured checks. Preserve issue and commit references throughout the workflow.

These are repository-administration steps, not settings enabled by this document. Boards, milestones, and ownership automation can be added when the contributor base needs them.
