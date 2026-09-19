# Percona PostgreSQL containers

Development Compose and both integration runners use
`percona/percona-distribution-postgresql:18.6`, pinned to the multi-platform digest
`sha256:dae47360e8137cafc1e8d66f9a1be348f1405e3cf51daa383b94e6c277e6b256`.
The image runs Percona Server for PostgreSQL 18.6.1, based on PostgreSQL 18.6,
and provides Linux AMD64 and ARM64 manifests. See the vendor's
[release notes](https://docs.percona.com/postgresql/18/release-notes/release-notes-v18.6.1.html)
and [Docker documentation](https://docs.percona.com/postgresql/18/docker.html).

SQLx, schema migrations, transaction boundaries and application behavior remain
PostgreSQL-based. No Percona-specific extension is required by Darkhorse. The image
bundles additional tools and extensions, but changing the image does not enable
transparent data encryption, database audit logging, query monitoring, pooling,
backups or high availability. Those features need their own configuration and
qualification. Query capture must not expose identity credentials or sensitive
parameters; encryption needs an external key-management and recovery design.

## Container contract

- Compose stores the cluster at `/data/db` in the workspace's `percona-data` volume.
  The former upstream image used `/var/lib/postgresql/18/docker` inside
  `database-data`. These directories are not interchangeable container mounts.
- The owner-only `.local/database-password` remains a read-only Compose secret;
  credentials and the loopback endpoint remain unchanged.
- Compose starts the vendor entrypoint as root to read that protected secret and
  prepare volume ownership. It drops to the image's `postgres` user, UID/GID 26,
  before initialization and database service execution. Integration containers
  use the image's default UID 26 with generated, disposable credentials.
- Readiness checks use `pg_isready -h 127.0.0.1`. Initialization temporarily serves
  a Unix socket before the final network listener is ready; checking that socket
  alone can report readiness too early.
- Resource limits and the loopback-only published port remain in
  `config/compose.dev.yaml`. The database TLS exception applies only to this local
  topology. Production database transport and runtime/migration role separation
  still require the deployment work described in [persistence](persistence.md).

Fresh workspaces use the existing commands:

```sh
make db-setup
make db-up
make db-migrate
make bootstrap
```

`make db-down` preserves the new volume and credentials. Unit tests still require
no running database, Docker engine or local configuration.

## Existing upstream development volumes

Use a logical dump and restore into a separate volume. This preserves the old
cluster and rebuilds indexes under the target image's locale libraries. Do not
point the Percona image at the old directory or reuse its raw files solely because
the PostgreSQL major version matches. The base images and operating-system users
differ.

`make db-up` refuses to create a new empty Percona cluster when this workspace has
an old `database-data` volume and no `percona-data` volume. This is a guard against
an accidental empty-directory cutover, not a validation of a manually created
restore target. Finish and verify the restore before using `make db-up`.

The following is the local single-database migration procedure; it is not a
production online-migration or disaster-recovery runbook:

1. Stop application writers and the workspace's database container. Identify the
   exact Compose project (`darkhorse-` plus the first ten SHA-256 hexadecimal
   characters of the absolute workspace path). `docker volume ls` lists its
   `<project>_database-data` source volume. Preserve `.local/database-password`
   and `.local/database.env`. Confirm no container still mounts the source for
   writing. Inventory non-template databases and additional roles before proceeding.
2. Create an owner-only backup directory with `umask 077` under `.local/`. Start a
   temporary source container using the previous pinned image
   `postgres:18.6@sha256:4ef4dbc939d61acea57712655ddb4b4ab27419c913f94cca0cd57cb3ea3c2280`,
   mounting the source volume at `/var/lib/postgresql`. Use `--network none`, no
   published ports, and `postgres -c default_transaction_read_only=on`. Wait for
   its TCP readiness check through `docker exec`.
3. Export globals and the application database from that container. For the
   default local role/database, the commands are:

   ```sh
   docker exec "$migration_source" pg_dumpall -U darkhorse --globals-only > "$percona_backup/globals.sql"
   docker exec "$migration_source" pg_dump -U darkhorse -d darkhorse --format=custom --create > "$percona_backup/darkhorse.dump"
   ```

   Here `migration_source` is the temporary source container name and
   `percona_backup` is the protected directory from step 2. Dumps include sensitive
   records and role verifiers; do not publish them or print their contents.
   Back up every additional application database separately if the inventory finds
   more than `darkhorse` and the standard `postgres` database.

4. Create a new `<project>_percona-data` volume, with labels
   `com.docker.compose.project=<project>` and
   `com.docker.compose.volume=percona-data`. Start a uniquely named temporary
   Percona container with that volume at `/data/db`, no network exposure, user
   `0:0`, `POSTGRES_USER=darkhorse`, **`POSTGRES_DB=postgres`**, and the existing
   password file mounted read-only at `/run/secrets/database_password` with
   `POSTGRES_PASSWORD_FILE` pointing there. Using `postgres` avoids creating an
   empty application database that would conflict with the archive's creation.
5. Wait for TCP readiness. Restore globals with `psql -X -v ON_ERROR_STOP=1`,
   omitting only the exact `CREATE ROLE darkhorse;` statement for the role already
   created by initialization. Keep its subsequent `ALTER ROLE` statements and
   all other roles and grants. Restore the untouched custom archive:

   ```sh
   sed '/^CREATE ROLE darkhorse;$/d' "$percona_backup/globals.sql" |
     docker exec -i "$migration_target" psql -X -U darkhorse -d postgres -v ON_ERROR_STOP=1
   docker exec -i "$migration_target" pg_restore -U darkhorse --dbname=postgres --create --exit-on-error < "$percona_backup/darkhorse.dump"
   ```

   `migration_target` is the temporary Percona container from step 4. Stop on any
   error; an existing target volume alone does not mean restoration succeeded.

6. Verify all table contents and sequence values against the quiesced source,
   including migration history, principal IDs, password verifiers, administrator
   membership, credential epochs, revocations and audit. Check database owners,
   successful password authentication over TCP and absence of collation-version
   mismatches. Database service execution must use UID 26. Do not initialize a
   replacement administrator or reset security state to bypass a restore failure.
7. Stop and remove only the two temporary containers, including their anonymous
   image volumes, with `docker rm --force --volumes`. Retain both named volumes
   and protected dumps. Start the restored cluster through `make db-up`, verify
   persistence across a stop/start, then run `make db-migrate` if the application
   version requires newer schema migrations. Resume applications only after
   validation.

A failed restore must not be used by applications. Recreate only its newly created
target volume when retrying; never delete the source volume or the protected dump
as part of a retry. After applications resume writes on Percona, the old volume is
stale. Restoring it can revive revoked access or discard new data, so it is not an
automatic rollback path. Production backup/restore and credential fencing remain
in the deployment qualification issues.

## Verification

`make test-postgres` runs migrations, authority/concurrency scenarios and operator
smoke checks against Percona. `make test-browser` uses Percona alongside the
separate Redis services for real login, SSO, token and resource-introspection
checks. `make docker-smoke` uses the same database image for the packaged server.
The startup guard has a service-free test using source-defined volume names.

The image change does not establish a performance improvement, extension security
audit or production readiness. Evaluate optional Percona features through focused
changes with failure, recovery and workload evidence.
