# Security policy

Darkhorse is in early development. There is no supported production release yet.
Passing tests or a deployment example does not establish production readiness;
the remaining requirements are in [release qualification](docs/release-readiness.md).

## Report a vulnerability privately

Use GitHub's [private vulnerability reporting form](https://github.com/OneTesseractInMultiverse/darkhorse-identity/security/advisories/new).
Do not post suspected vulnerabilities, exploit details, or sensitive logs in public
issues or pull requests. If the form is unavailable, open an issue requesting a
private contact channel without including vulnerability details.

Include the affected commit/version, deployment mode, expected security boundary,
observed impact, and a minimal reproduction against a system you control. Use
synthetic accounts and redacted examples. Never send live passwords, tokens,
private keys, database dumps, personal records, or unrelated customer information.

Maintainers will assess the report, coordinate remediation and disclosure with the
reporter, and publish an advisory when appropriate. No response-time or patch-time
service commitment is currently offered. Please coordinate public disclosure
through the private report while remediation is being assessed.

## Scope and support

Authentication, authorization, protocol handling, credential/key lifecycle,
operator access, deployment isolation, and dependency vulnerabilities are within
scope. Ordinary bugs and feature requests belong in public issues after removing
sensitive information. Published upstream dependency advisories can be tracked
publicly without disclosing a new exploit against Darkhorse.

Reports should identify the tested revision. There are no maintained release
branches or backport commitments yet; support and upgrade commitments must be
defined before the first production release. Do not assume that an image tag,
source archive, or the current default branch is a supported deployment.
