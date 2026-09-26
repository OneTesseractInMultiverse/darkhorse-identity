# Contributing translations

This guide covers Darkhorse's bundled English (`en`) and Spanish (`es`) messages.
Localization changes are ordinary source changes: they require a complete catalog,
review of the meaning, and the same security checks as the behavior that displays
them.

## Catalog workflow

1. Add or update the typed message contract in
   `apps/console/src/lib/i18n/contract.ts` or `admin-contract.ts`.
2. Add one complete message for every contract key to each matching source catalog:
   `en.json`, `es.json`, `admin-en.json` and `admin-es.json`. These catalogs are
   arrays of `[key, message]` pairs. Keep that representation so duplicate keys
   remain detectable.
3. Use named ICU placeholders with the types declared by the contract. Keep the
   same placeholder names and meanings in both languages. Plural messages need an
   `other` branch and must follow the selected locale's plural rules.
4. Translate the complete sentence or prompt. Do not join translated fragments to
   construct a sentence, and do not put HTML, Svelte markup, code, or executable
   expressions in a catalog.
5. Run `make i18n-check`, `make test-i18n`, and `make check`. For changes to a
   rendered flow, also run the relevant browser, email, or CLI boundary target.
   Record any environment-dependent qualification separately; isolated unit tests
   do not require services, private context, or external files.
6. Review the updated [translation inventory and measurements](localization.md),
   and verify the static build remains within its recorded payload budget.

The source contract defines which messages exist and the exact interpolation
contract. English is the fallback if an optional Spanish catalog cannot be used;
fallback does not make an incomplete Spanish catalog acceptable at review or
release time. The build-only checker reads four fixed production catalog files,
requires regular UTF-8 files, and caps each at 256 KiB before parsing. Catalogs
are data: validation parses them but never evaluates translator-supplied code.

## Language style and glossary

English is concise and uses sentence case. Spanish uses neutral Latin American
vocabulary, informal singular instructions, sentence case, and standard accents
and punctuation. Keep terms consistent with this glossary:

| English                   | Spanish                            |
| ------------------------- | ---------------------------------- |
| Sign in / Sign out        | Iniciar sesión / Cerrar sesión     |
| Application               | Aplicación                         |
| Session                   | Sesión                             |
| Role / Scope / Capability | Rol / Ámbito / Capacidad           |
| API key / Client secret   | Clave de API / Secreto del cliente |
| Management console        | Consola de administración          |
| Unavailable               | No disponible                      |

Prefer short, direct labels, but preserve the complete meaning of warnings and
confirmation prompts. Review narrow layouts and zoom because Spanish text can be
longer. Do not abbreviate security terms in a way that blurs whether an action is
pending, denied, committed, or uncertain.

## Security meaning and interpolation

Review paired messages for equal security meaning. In particular, an unconfirmed
logout is not a successful logout; a dependency outage is not an invalid password;
application registration does not grant a person access; and revocation or a
destructive action must not sound optional in one language only.

Keep protocol fields, JSON names and values, permission identifiers, role/scope/
capability IDs, audit event names, country codes, credentials, and command-line
flags unchanged. Translate only the human-facing presentation. A requested locale
never changes authorization decisions or token contents.

Treat every placeholder value as untrusted input. Keep it in a typed ICU argument;
render it through the existing text APIs. Do not concatenate it into markup, an
attribute containing executable content, a URL, a shell command, or a template
path. The catalog compiler rejects markup and directional controls, enforces
placeholder types and total syntax bounds, and checks plural branches. The UI must
continue to use escaped text rendering. Human CLI output must continue to pass
through its terminal-control filter, and JSON output must retain its existing
schema and values in both languages.

## Adding or removing a language

Do not copy authentication, authorization, persistence, or route logic to add a
language. Extend the locale allowlist/resolver, catalog contracts and complete
catalogs, browser metadata, email and CLI selection as applicable, plus boundary
tests and measurements. Review language-specific plural behavior and the meaning
of fallback before advertising the language. Removing a language must remove its
catalog and every advertised locale atomically; do not leave stale metadata.

For layout experiments, the test-only pseudolocale helper at
`apps/console/tests/unit/lib/i18n/pseudolocale.ts` accents and expands catalog
literals while preserving ICU arguments and plural behavior. Use it through the
existing formatter in tests. It lives under `tests/`, outside the production `src/`
tree, and must never be imported into a release build or added to the production
locale allowlist. This exercises expansion and catalog wiring without duplicating
business logic or claiming a third supported language.

## Source and review provenance

Current catalogs are maintained as first-party project text. Before importing a
third-party translation, glossary, country-name dataset, or other catalog content,
record its upstream source, version/date, license, and required attribution in
`docs/third-party-notices.md`; do not copy content whose license is unclear. Keep
country codes stable and translate only their display names. Translation review
must identify a fluent reviewer and a separate security-meaning review for
authentication, consent, recovery, verification, and destructive-action text.
Automated key/ICU checks cannot establish linguistic quality.

Use the [localization release checklist](localization-release.md) to record which
channels have implementation evidence, human review, accessibility review, and
release qualification. Do not call a language release-qualified on the strength
of catalog checks or test counts alone.
