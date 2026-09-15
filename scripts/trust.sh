#!/usr/bin/env bash
set -euo pipefail
certificate='.local/pki/pki/authorities/local/root.crt'
if [[ ! -f "$certificate" ]]; then
  printf 'Run make https-setup first.\n' >&2
  exit 1
fi
if [[ "$(uname -s)" != Darwin ]]; then
  printf 'Host trust automation currently supports macOS only. See docs/development.md for explicit client trust.\n' >&2
  exit 1
fi
openssl x509 -in "$certificate" -noout -subject -fingerprint -sha256
case "${1:-}" in
  trust)
    security add-trusted-cert -r trustRoot -p ssl -k "$HOME/Library/Keychains/login.keychain-db" "$certificate"
    ;;
  untrust)
    security remove-trusted-cert "$certificate"
    ;;
  *) printf 'Usage: trust.sh trust|untrust\n' >&2; exit 1 ;;
esac
