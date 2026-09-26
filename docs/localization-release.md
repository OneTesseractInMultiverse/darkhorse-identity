# Localization release checklist

This checklist records evidence by user-facing channel. A language is not
release-qualified until the applicable functional, security-meaning, accessibility,
and fluent-review gates pass for the same source revision. Automated checks prove
bounded properties of the implementation; they do not certify translations.

## Channel qualification

| Channel                           | Implemented language behavior                                                                                                     | Evidence required for a release                                                                                                                                      | Current status                                                                                                                                  |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Web portal and management console | English/Spanish selection, persisted account preference, fallback, route language metadata, and localized console flows           | Static build and payload measurement; keyboard, narrow viewport and text zoom; success/error screens; fluent and accessibility review                                | Implemented; independent fluent and accessibility reviews remain open                                                                           |
| OIDC login and consent            | Bounded transaction-local `ui_locales`; selected language survives reload; unsupported hints fall back without changing authority | Two-client concurrent browser flow; consent decisions, redirects, PKCE/state/nonce and locale-independent protocol data; hostile text checks; fluent security review | Implemented; hosted boundary tests pass; independent review remains open                                                                        |
| Verification and invitation email | Language and template version are pinned with the delivery attempt                                                                | English/Spanish content, retry determinism, link purpose/expiry, output bounds, secret redaction, and hosted delivery boundary                                       | Implemented; release review remains open                                                                                                        |
| Operator CLI                      | English/Spanish human help, prompts and results; JSON records and exit codes remain stable                                        | Real process and terminal tests in both languages; cancellation, no-echo, redaction, closed streams, confirmation semantics, executable/startup measurement          | Hosted source, terminal and boundary checks passed for `2819db3` in run `36254940245`; independent fluent and accessibility reviews remain open |

The table describes implementation scope, not an approval. Keep review evidence
linked to the exact commit or issue that owns it. Do not attach credentials,
personal data, raw service logs, or unredacted screenshots to public evidence.

## Required release checks

- [ ] `make i18n-check` rejects missing/extra/duplicate keys, placeholder drift,
      invalid plural syntax, unsafe catalog text, invalid UTF-8, links, and oversized
      catalog inputs.
- [ ] `make test-i18n` exercises source-defined catalog failures, fallback,
      persisted choices, interpolation and the test-only expansion pseudolocale.
- [ ] `make check` and `make build-web` pass with no coverage gate lowered.
- [ ] Browser qualification covers both languages, keyboard-only operation, narrow
      viewports, text zoom, persisted selection, concurrent authorization transactions,
      success/failure flows, and security-sensitive confirmation wording.
- [ ] Email qualification covers both languages and retries while preserving the
      original template/version and purpose-bound link.
- [ ] CLI qualification covers both human languages and proves machine-readable
      output, exit status, confirmation and secret-handling contracts are unchanged.
- [ ] A fluent reviewer approves each language, and an accessibility reviewer
      checks names, focus, language metadata, contrast and expanded text.
- [ ] Security-sensitive translations receive a meaning review for authentication,
      consent, verification, recovery, revocation, destructive actions and uncertainty.
- [ ] Comparable web, email, CLI and server measurements are recorded. Browser
      payload stays within the existing 36 KiB localization regression budget; no
      budget is raised just to accommodate a regression.
- [ ] Third-party catalog and country metadata sources have compatible licensing
      and attribution recorded in the notices document.
- [ ] Remaining translation, accessibility, coverage, capacity and dependency gaps
      are linked and disclosed. Catalog validation alone is not release qualification.

Darkhorse currently has no production-qualified release. English and Spanish
implementation evidence exists across the web, OIDC, email and CLI channels.
Independent fluent and accessibility reviews remain required before describing
either language as release-qualified.
