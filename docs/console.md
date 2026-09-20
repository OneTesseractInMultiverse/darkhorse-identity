# Management console and user directory

Sign in at `https://localhost:8443`, then choose **Open console**, or open
`/console/users`. Run the prepared local deployment with `make dev-login`;
see [development](development.md) for database, migration, bootstrap, Redis,
and HTTPS setup. Only a currently active platform administrator can read or
change the directory. The static page and its navigation carry no authority.

## Workflows

The console uses a top navigation bar, a left menu, dark surfaces, restrained
green accents, and translucent dialogs. The user table contains focused actions:

- **View** opens the profile and its current name, email, account status,
  verification status, and administrator membership.
- **Edit name** changes mandatory first and last names in a separate form.
- **Deactivate** and **Reactivate** require confirmation. Deactivation retains
  the profile and assignments while invalidating access. Reactivation preserves
  the revocation epoch; old sessions and tokens remain invalid.
- **Access** loads an application's existing roles and assigns or removes one
  role at a time. Removing a role from an inactive application is allowed.

Email, credentials, and platform administrator membership are read-only here.
Application, client, resource, scope, role, and capability workflows are available
from the sidebar; see [catalog administration](catalog-administration.md).
Extended profile fields and picture uploads belong to the profile slice. This
console does not imply those workflows are available.

Search matches literal, case-insensitive prefixes of email, full name, or last
name. Status filters support active and inactive accounts. The table follows
immutable creation time and ID order using cursor pagination, with 25 users per
UI page. Filters reset the cursor. Concurrent changes may alter which records
match subsequent pages; this is a live directory, not a snapshot export.

## Rust boundary

| Endpoint                           | Behavior                                                                              |
| ---------------------------------- | ------------------------------------------------------------------------------------- |
| `GET /api/admin/users`             | Bounded user projection; optional `search`, `status`, `limit`, `after`                |
| `GET /api/admin/users/{id}`        | Fresh profile and revision                                                            |
| `GET /api/admin/users/{id}/access` | Application summaries plus roles for one selected application; optional `application` |
| `POST /api/admin/users/{id}`       | One name, status, or role-assignment change                                           |

IDs are canonical nonzero UUIDs. Revisions are decimal strings, preserving the
full database counter range in JavaScript. A mutation supplies `revision` and a
`change` object whose `kind` is `names`, `status`, or `role`. Role changes also
supply `application_id`, `role_id`, `assigned`, and `policy_revision`.
Unexpected fields, duplicate filters, offset pagination, and unsupported query
operators are rejected. Search values are bound SQL parameters with wildcard
characters escaped; restqs translates the allowlisted status and limit filters.
The limit defaults to 25 and cannot exceed 100. Responses contain no credential
verifiers, bearer material, internal eligibility, or credential epochs.

Every endpoint checks a live Rust-owned session against primary PostgreSQL state.
Writes additionally require authentication within five minutes, exact same-origin
CSRF protection, and fresh target revisions. Signing in again creates a fresh
session; directory reads do not extend idle time. Headers prohibit caching.
The HTTP boundary limits request bodies to 4 KiB, query lengths, concurrent
requests, and request duration.

Transactions acquire the common security fence before reading authority and
locking targets. Reads share the fence; writes hold it exclusively. Authority is
checked again after target locks, including current time. The existing account
policy protects the last eligible administrator. Role changes check the global
policy revision and the exact application/role binding, and cannot grant roles
through an inactive application. Assignments increase the user's revision;
live authorization continues to recompute effective capabilities.

Actor/session/target audit records commit with every effective change. Existing
account and authorization-policy audits remain in the same transaction. An audit
failure rolls everything back. No-op requests create no new audit or revision.
Audit history is immutable and contains no secrets or copies of profile names.

The access picker supports at most 100 application summaries and 128 roles for
the selected application. Overflow fails explicitly rather than returning partial
authority. Catalog pagination is required before lifting these limits. No positive
authorization result is cached in Redis or the browser. Directory performance at
production scale still needs workload measurements.

## Failure and accessibility behavior

Loading, empty, unavailable, denied, and successful states are explicit. Dialogs
keep editing outside the table, contain keyboard focus, support Escape when idle,
and return focus on cancellation. Name editing focuses the first input. Tables
scroll within their region on narrow screens; labels and action names identify
the affected user. Profile values are rendered as text.

Denied reads clear rendered private data. A failed, stale, or uncertain mutation
requires a fresh read before another change. Mutations are never retried
automatically. A timeout can occur after commit; refreshing reconciles that case.

`make test-directory` runs isolated policy, transport, and component tests without
services or settings. `make test-postgres` exercises migrations, real transactions,
role binding, revocation, pagination, races, and audit rollback.
`make test-browser` exercises the built static route through verified HTTPS,
including forms, confirmation, stale writes, focus, and narrow-screen behavior.
These checks are not a complete accessibility audit or production qualification.
Coverage remains subject to the unchanged authored-code target in
[engineering](engineering.md).
