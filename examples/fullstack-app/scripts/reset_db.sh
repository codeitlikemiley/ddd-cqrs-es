#!/usr/bin/env bash
set -euo pipefail

# Erase local fullstack data. The application reapplies the canonical,
# checksum-verified migration embedded by wasi-auth on its next startup.

BACKEND="${AUTH_DB:-${db:-postgres}}"

reset_postgres() {
  if [[ -z "${DATABASE_URL:-}" ]]; then
    echo "Error: DATABASE_URL environment variable is not set." >&2
    exit 1
  fi

  echo "Erasing fullstack-app PostgreSQL data..."
  psql "$DATABASE_URL" <<'SQL'
BEGIN;
DROP TABLE IF EXISTS
    auth_application_redirects,
    auth_flows,
    auth_one_time_tokens,
    auth_passwords,
    auth_rate_limit_buckets,
    auth_redirect_uris,
    auth_refresh_tokens,
    auth_system_administrators,
    auth_totp_factors,
    auth_audit_log,
    auth_outbox,
    auth_idempotency,
    auth_policy_bundles,
    auth_jwks,
    auth_signing_keys,
    auth_redirect_allowlists,
    auth_redirect_allowlist,
    auth_provider_configs,
    auth_token_grants,
    auth_one_time_grants,
    auth_refresh_token_hashes,
    auth_sessions,
    auth_recovery_codes,
    auth_mfa_totp,
    auth_mfa_factors,
    auth_passkey_credentials,
    auth_passkeys,
    auth_webauthn_challenges,
    auth_oauth_transactions,
    auth_password_credentials,
    auth_external_identities,
    auth_secret_records,
    auth_credentials,
    auth_policy_versions,
    auth_audit_events,
    auth_invitations,
    auth_membership_roles,
    auth_memberships,
    auth_role_permissions,
    auth_roles,
    auth_organizations,
    auth_users_by_email,
    auth_users,
    auth_projection_records,
    auth_events,
    checkpoints,
    events,
    auth_schema_migrations
CASCADE;
DROP FUNCTION IF EXISTS
    auth_adjust_owner_count(),
    auth_notify_context_invalidation(),
    auth_track_membership_authorization(),
    auth_validate_owner_count()
CASCADE;
COMMIT;
SQL
  echo "PostgreSQL data erased; wasi-auth will install its canonical schema on next startup."
}

case "$BACKEND" in
  postgres)
    reset_postgres
    ;;
  *)
    echo "Error: unsupported AUTH_DB=$BACKEND. The fullstack template requires postgres." >&2
    exit 2
    ;;
esac
