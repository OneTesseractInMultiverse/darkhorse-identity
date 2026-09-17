#!/bin/sh
set -eu
# The image entrypoint drops privileges after preparing the protected ACL copy.
mkdir -p /run/redis
cp /run/secrets/redis_acl /run/redis/users.acl
chmod 600 /run/redis/users.acl
chown redis:redis /run/redis/users.acl
exec /usr/local/bin/docker-entrypoint.sh redis-server /usr/local/etc/redis/redis.conf
