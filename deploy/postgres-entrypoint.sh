#!/bin/sh
set -eu
mkdir -p /run/darkhorse
cp /run/secrets/database_key /run/darkhorse/server.key
cp /run/secrets/database_cert /run/darkhorse/server.crt
chown 26:26 /run/darkhorse /run/darkhorse/server.key /run/darkhorse/server.crt
chmod 700 /run/darkhorse
chmod 600 /run/darkhorse/server.key
exec /entrypoint.sh postgres -c ssl=on -c ssl_cert_file=/run/darkhorse/server.crt -c ssl_key_file=/run/darkhorse/server.key -c hba_file=/etc/darkhorse/pg_hba.conf -c password_encryption=scram-sha-256
