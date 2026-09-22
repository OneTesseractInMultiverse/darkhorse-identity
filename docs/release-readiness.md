# Release qualification

Darkhorse has no qualified production release. [Issue #21](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/21)
tracks the integrated release criteria. A working console, test count, benchmark,
or container build is evidence for its measured scope only.

## Reproducible source checks

Install the pinned toolchains and locked dependencies as described in
[development](development.md), then run:

```sh
make ci
make test-release-tools
make source-package-check
make audit-tools
make audit-dependencies
```

`make ci` runs service-free format, lint, type, architecture and unit checks, plus
Rust release and static frontend builds. `test-release-tools` separately exercises
real Git archives and subprocess failures in a disposable directory. Neither
requires identity credentials, services, a browser, or local planning files.

`source-package-check` examines the committed `HEAD` tree and its actual Git tar
export. It rejects private/generated directories, credential-shaped file names,
links/submodules, unsafe paths, missing essential source, and inventory changes
caused by export rules. `SOURCE_REF=<commit-or-tree>` checks another explicit Git
object, including `SOURCE_REF=$(git write-tree)` for a reviewed staged tree before
commit. The output records the object ID and archive digest. Uncommitted files
are not part of that evidence. This is an inventory check, not content secret
scanning, a build from the exported archive, or license approval. Source containing
embedded credentials still requires review and dedicated release scanning.

`audit-tools` explicitly installs cargo-audit 0.22.2. `audit-dependencies` uses that
version and pnpm 11.19.0, contacts the official RustSec Git repository and npm
registry, and checks the complete lockfiles including development dependencies
and platform-specific packages. Vulnerabilities and informational/yanked warnings
block the command; no target filter, ignore list, severity exclusion, automatic
fix, or registry-error bypass is configured. Failed tools, unexpected report
formats, inconsistent counts, and incomplete scans cannot produce a passing
result. Advisory data changes over time: rerun immediately before release.

The private `.local/security/report.json` records the revision, tracked-change
flag, input hashes, auditor versions, timestamps, RustSec database revision, and
findings. A modified working tree is not qualification of the recorded commit.
Network/tool failures may produce only an explicit incomplete/error record.
Inspect the findings and create focused remediation issues; do not add ignores
merely to make the gate pass. See the dated [dependency review](dependencies.md#release-advisory-review).
These scanners do not review licenses, container OS packages, source secrets,
compiler provenance, deployment configuration, or independent protocol security.

## Hosted checks

[CI](../.github/workflows/ci.yaml) runs on pushes to `main`, pull requests targeting
`main`, and manual dispatch. **Source verification** runs the source checks above
plus POSIX CLI subprocess/terminal tests (Python 3 on the hosted runner),
without advisory installation/scanning. **Dependency qualification** runs the
strict live advisory gate independently and retains its summary for 14 days.
A known advisory warning remains a failed qualification check until resolved;
it does not become a pass because another job succeeds.

The workflow uses read-only repository permission, immutable official action
commit references, no persisted checkout credentials, no dependency caches,
and no repository secrets. Untrusted pull requests never use `pull_request_target`.
The workflow does not publish packages, container images or releases. Artifact
uploads contain only the dependency summary, not local configuration, credentials
or service logs. [GitHub's workflow security guidance](https://docs.github.com/en/actions/reference/security/secure-use).

Branch protection, fork-policy behavior and an independent approval process must
be verified separately; adding a workflow does not configure those settings.
Select required checks only after observing their actual behavior and resolving
their blockers. CI currently omits service/browser/deployment suites, coverage
qualification, and independent review. Their evidence is still required at the
affected boundary and before a production release.

## Open release gates

| Gate                                                | Required evidence / remaining work                                                                                                                                                                                                                                                                                                                                                           |
| --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Release contracts and licensing                     | Resolve [#1](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/1): project license, supported versions/platforms, deployment and administrator assurance contracts; review all bundled third-party licenses/notices. Public source visibility does not grant an open-source license.                                                                                     |
| Authored-code coverage and contribution enforcement | Complete [#2](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/2). Existing 100% gates remain unchanged and are not satisfied by all unit/integration reports. Include tooling/entrypoints and explain instrumentation/branch limits; keep observed numbers tied to their revision and suite.                                                                           |
| Protocol and security assurance                     | Run selected OIDC/OAuth conformance and negative interoperability suites against supported deployments; obtain independent threat-model/security review, and address findings. Application tests do not constitute certification.                                                                                                                                                            |
| Operator and recovery authority                     | Complete applicable [#23–#27](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/23) CLI/authority, secret-handling, audit, concurrency, emergency access and recovery criteria. Container access alone must not be treated as qualified authorization.                                                                                                                   |
| Deployment operation and performance                | Complete remaining [Compose #19](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/19) and [Kubernetes #20](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/20) topology, overload, post-commit revocation, failover, limiter recovery, restore and upgrade evidence. Local fixtures do not establish production capacity or CNI failure behavior. |
| Supply chain and support                            | Resolve dependency findings; scan final images and source contents, inventory third-party licenses, establish signed/reproducible release artifacts and provenance, publish install/upgrade/recovery/support guidance, and verify the private reporting channel in [SECURITY.md](../SECURITY.md).                                                                                            |

For each release candidate, retain the exact source revision and artifact/image
digests; command and tool versions; UTC date; topology, architecture and resource
limits; sanitized results; failures/skips; and links to open findings. Record
workload/arrival assumptions for performance and restore points/key handling for
recovery. Review artifact contents before publication. Revalidate affected evidence
after changes. Do not publish a production claim while its gate remains unresolved.
