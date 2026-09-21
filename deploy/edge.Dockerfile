FROM caddy:2.11.4-alpine@sha256:de23def33b17fb5d1290b0f6c2add1d70780e52341896c00a4c8a2a2fe9d355e
# Remove the upstream file capability so execution works with an empty capability
# bounding set. The qualification topology uses unprivileged listening ports.
RUN setcap -r /usr/bin/caddy
USER 10001:10001
