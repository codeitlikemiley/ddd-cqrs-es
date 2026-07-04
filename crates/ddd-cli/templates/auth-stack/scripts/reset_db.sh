#!/bin/bash
set -euo pipefail

# reset_db.sh - auth-stack schema reset for supported SQL backends.

BACKEND="${AUTH_DB:-${db:-sqlite}}"

url_decode() {
  local input="${1//+/ }"
  printf '%b' "${input//%/\\x}"
}

query_param() {
  local query="$1"
  local key="$2"
  local pair
  local old_ifs="$IFS"
  IFS='&'
  for pair in $query; do
    case "$pair" in
      "$key="*) url_decode "${pair#*=}"; IFS="$old_ifs"; return 0 ;;
    esac
  done
  IFS="$old_ifs"
  return 1
}

write_mysql_option() {
  local key="$1"
  local value="$2"
  case "$value" in
    *$'\n'*|*$'\r'*)
      echo "Error: MySQL URL field for $key contains a newline." >&2
      exit 1
      ;;
  esac
  printf '%s=%s\n' "$key" "$value"
}

mysql_reset() {
  if [ -z "${DATABASE_URL:-}" ]; then
    echo "Error: DATABASE_URL environment variable is not set." >&2
    exit 1
  fi

  local url_no_scheme="${DATABASE_URL#mysql://}"
  if [ "$url_no_scheme" = "$DATABASE_URL" ]; then
    echo "Error: MySQL DATABASE_URL must start with mysql://." >&2
    exit 1
  fi

  local url_no_fragment="${url_no_scheme%%#*}"
  local query_string=""
  case "$url_no_fragment" in
    *\?*) query_string="${url_no_fragment#*\?}" ;;
  esac
  local url_no_query="${url_no_fragment%%\?*}"

  case "$url_no_query" in
    */*) ;;
    *)
      echo "Error: MySQL DATABASE_URL must include a database name." >&2
      exit 1
      ;;
  esac

  local user_pass_host_port="${url_no_query%%/*}"
  local db_name
  db_name="$(url_decode "${url_no_query#*/}")"
  local user_pass="${user_pass_host_port%@*}"
  local host_port="${user_pass_host_port##*@}"

  if [ "$user_pass" = "$user_pass_host_port" ]; then
    echo "Error: MySQL DATABASE_URL must include user credentials." >&2
    exit 1
  fi

  local db_user
  local db_pass
  if [[ "$user_pass" == *:* ]]; then
    db_user="$(url_decode "${user_pass%%:*}")"
    db_pass="$(url_decode "${user_pass#*:}")"
  else
    db_user="$(url_decode "$user_pass")"
    db_pass=""
  fi

  local db_host
  local db_port
  if [[ "$host_port" == \[*\]* ]]; then
    db_host="${host_port%%]*}"
    db_host="${db_host#[}"
    local host_port_remainder="${host_port#*]}"
    if [[ "$host_port_remainder" == :* ]]; then
      db_port="${host_port_remainder#:}"
    else
      db_port=3306
    fi
  else
    db_host="${host_port%%:*}"
    db_port="${host_port#*:}"
    if [ "$db_port" = "$host_port" ]; then
      db_port=3306
    fi
  fi

  if [ -z "$db_user" ] || [ -z "$db_host" ] || [ -z "$db_name" ]; then
    echo "Error: MySQL DATABASE_URL must include user, host, and database name." >&2
    exit 1
  fi
  if [ -z "$db_port" ]; then
    db_port=3306
  fi

  local mysql_ssl_mode
  mysql_ssl_mode="$(query_param "$query_string" "ssl-mode" || true)"
  if [ -z "$mysql_ssl_mode" ]; then
    mysql_ssl_mode="$(query_param "$query_string" "ssl_mode" || true)"
  fi

  local mysql_defaults_file
  mysql_defaults_file="$(mktemp)"
  trap 'rm -f "${mysql_defaults_file:-}"' EXIT
  chmod 600 "$mysql_defaults_file"
  {
    echo "[client]"
    write_mysql_option "user" "$db_user"
    if [ -n "$db_pass" ]; then
      write_mysql_option "password" "$db_pass"
    fi
    write_mysql_option "host" "$db_host"
    write_mysql_option "port" "$db_port"
    if [ -n "$mysql_ssl_mode" ]; then
      write_mysql_option "ssl-mode" "$mysql_ssl_mode"
    fi
  } > "$mysql_defaults_file"

  echo "Resetting auth-stack MySQL schema..."
  mysql --defaults-extra-file="$mysql_defaults_file" "$db_name" <<'SQL'
DROP TABLE IF EXISTS authz_check_audit;
DROP TABLE IF EXISTS authz_tuple_index_by_object;
DROP TABLE IF EXISTS authz_tuple_index_by_subject;
DROP TABLE IF EXISTS authz_relationship_tuples;
DROP TABLE IF EXISTS authz_active_model;
DROP TABLE IF EXISTS authz_models;
DROP TABLE IF EXISTS auth_redirect_allowlists;
DROP TABLE IF EXISTS auth_token_grants;
DROP TABLE IF EXISTS auth_passkey_credentials;
DROP TABLE IF EXISTS auth_provider_configs;
DROP TABLE IF EXISTS auth_jwks;
DROP TABLE IF EXISTS auth_signing_keys;
DROP TABLE IF EXISTS auth_refresh_token_hashes;
DROP TABLE IF EXISTS auth_sessions;
DROP TABLE IF EXISTS auth_password_credentials;
DROP TABLE IF EXISTS auth_external_identities;
DROP TABLE IF EXISTS auth_users_by_email;
DROP TABLE IF EXISTS auth_users;
DROP TABLE IF EXISTS checkpoints;
DROP TABLE IF EXISTS events;
DROP TABLE IF EXISTS auth_schema_migrations;

CREATE TABLE auth_schema_migrations (
    version VARCHAR(255) PRIMARY KEY,
    applied_at_ms BIGINT NOT NULL
);

CREATE TABLE events (
    sequence BIGINT AUTO_INCREMENT PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL UNIQUE,
    aggregate_id VARCHAR(255) NOT NULL,
    aggregate_type VARCHAR(255) NOT NULL,
    revision BIGINT NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    event_version BIGINT NOT NULL,
    payload LONGTEXT NOT NULL,
    metadata LONGTEXT NOT NULL,
    recorded_at_ms BIGINT NOT NULL,
    UNIQUE KEY events_aggregate_revision_unique (aggregate_type, aggregate_id, revision),
    INDEX idx_auth_events_aggregate (aggregate_type, aggregate_id),
    INDEX idx_auth_events_type_sequence (aggregate_type, sequence)
);

CREATE TABLE checkpoints (
    projection_name VARCHAR(255) PRIMARY KEY,
    last_sequence BIGINT NOT NULL
);

CREATE TABLE auth_users (
    user_id VARCHAR(255) PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    primary_email VARCHAR(320) NOT NULL,
    disabled TINYINT NOT NULL DEFAULT 0,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL
);

CREATE TABLE auth_users_by_email (
    tenant_id VARCHAR(255) NOT NULL,
    normalized_email VARCHAR(320) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    PRIMARY KEY (tenant_id, normalized_email)
);

CREATE TABLE auth_external_identities (
    tenant_id VARCHAR(255) NOT NULL,
    provider_id VARCHAR(255) NOT NULL,
    provider_subject VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    primary_email VARCHAR(320),
    profile_json LONGTEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, provider_id, provider_subject)
);

CREATE TABLE auth_password_credentials (
    tenant_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    password_hash TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    revoked_at_ms BIGINT,
    last_authenticated_at_ms BIGINT,
    PRIMARY KEY (tenant_id, user_id)
);

CREATE TABLE auth_sessions (
    session_id VARCHAR(255) PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    primary_email VARCHAR(320),
    expires_at_ms BIGINT NOT NULL,
    revoked_at_ms BIGINT,
    permissions_json LONGTEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL
);

CREATE TABLE auth_refresh_token_hashes (
    tenant_id VARCHAR(255) NOT NULL,
    token_hash VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    expires_at_ms BIGINT NOT NULL,
    rotated_at_ms BIGINT,
    revoked_at_ms BIGINT,
    created_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, token_hash)
);

CREATE TABLE auth_signing_keys (
    tenant_id VARCHAR(255) NOT NULL,
    kid VARCHAR(255) NOT NULL,
    alg VARCHAR(64),
    status VARCHAR(64) NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    activated_at_ms BIGINT,
    retired_at_ms BIGINT,
    revoked_at_ms BIGINT,
    PRIMARY KEY (tenant_id, kid)
);

CREATE TABLE auth_jwks (
    kid VARCHAR(255) PRIMARY KEY,
    kty VARCHAR(64) NOT NULL,
    alg VARCHAR(64) NOT NULL,
    use_value VARCHAR(64) NOT NULL,
    public_parameters_json LONGTEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    retired_at_ms BIGINT
);

CREATE TABLE auth_provider_configs (
    tenant_id VARCHAR(255) NOT NULL,
    provider_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    login_url TEXT NOT NULL,
    enabled TINYINT NOT NULL DEFAULT 0,
    issuer_url TEXT,
    client_id TEXT,
    secret_ref TEXT,
    scopes_json LONGTEXT NOT NULL,
    redirect_uris_json LONGTEXT NOT NULL,
    claim_mapping_json LONGTEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, provider_id)
);

CREATE TABLE auth_passkey_credentials (
    tenant_id VARCHAR(255) NOT NULL,
    credential_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    public_key_json LONGTEXT NOT NULL,
    transports_json LONGTEXT NOT NULL,
    sign_count BIGINT NOT NULL DEFAULT 0,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, credential_id),
    INDEX idx_auth_passkey_credentials_user (tenant_id, user_id)
);

CREATE TABLE auth_token_grants (
    grant_id VARCHAR(255) PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL,
    grant_type VARCHAR(255) NOT NULL,
    subject_hint VARCHAR(320),
    redirect_url TEXT NOT NULL,
    payload_json LONGTEXT NOT NULL,
    expires_at_ms BIGINT NOT NULL,
    consumed_at_ms BIGINT,
    created_at_ms BIGINT NOT NULL
);

CREATE TABLE auth_redirect_allowlists (
    tenant_id VARCHAR(255) PRIMARY KEY,
    redirects_json LONGTEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL
);

CREATE TABLE authz_models (
    tenant_id VARCHAR(255) NOT NULL,
    model_id VARCHAR(255) NOT NULL,
    schema_json LONGTEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, model_id)
);

CREATE TABLE authz_active_model (
    tenant_id VARCHAR(255) PRIMARY KEY,
    model_id VARCHAR(255) NOT NULL,
    activated_at_ms BIGINT NOT NULL
);

CREATE TABLE authz_relationship_tuples (
    tenant_id VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    subject_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    relation VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    object_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    condition_name VARCHAR(255),
    context_json LONGTEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, object_ref, relation, subject_ref),
    INDEX idx_authz_tuple_by_subject (tenant_id, subject_ref, relation, object_ref),
    INDEX idx_authz_tuple_by_object (tenant_id, object_ref, relation, subject_ref)
);

CREATE TABLE authz_tuple_index_by_subject (
    tenant_id VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    subject_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    relation VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    object_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    PRIMARY KEY (tenant_id, subject_ref, relation, object_ref)
);

CREATE TABLE authz_tuple_index_by_object (
    tenant_id VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    object_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    relation VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    subject_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    PRIMARY KEY (tenant_id, object_ref, relation, subject_ref)
);

CREATE TABLE authz_check_audit (
    tenant_id VARCHAR(255) NOT NULL,
    check_id VARCHAR(255) NOT NULL,
    subject_ref VARCHAR(512) NOT NULL,
    relation VARCHAR(255) NOT NULL,
    object_ref VARCHAR(512) NOT NULL,
    allowed TINYINT NOT NULL,
    reason TEXT,
    checked_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, check_id)
);
SQL
  echo "Auth-stack MySQL schema reset."
}

postgres_reset() {
  if [ -z "${DATABASE_URL:-}" ]; then
    echo "Error: DATABASE_URL environment variable is not set." >&2
    exit 1
  fi

  echo "Resetting auth-stack PostgreSQL schema..."
  psql "$DATABASE_URL" <<'SQL'
DROP TABLE IF EXISTS authz_check_audit;
DROP TABLE IF EXISTS authz_tuple_index_by_object;
DROP TABLE IF EXISTS authz_tuple_index_by_subject;
DROP TABLE IF EXISTS authz_relationship_tuples;
DROP TABLE IF EXISTS authz_active_model;
DROP TABLE IF EXISTS authz_models;
DROP TABLE IF EXISTS auth_redirect_allowlists;
DROP TABLE IF EXISTS auth_token_grants;
DROP TABLE IF EXISTS auth_passkey_credentials;
DROP TABLE IF EXISTS auth_provider_configs;
DROP TABLE IF EXISTS auth_jwks;
DROP TABLE IF EXISTS auth_signing_keys;
DROP TABLE IF EXISTS auth_refresh_token_hashes;
DROP TABLE IF EXISTS auth_sessions;
DROP TABLE IF EXISTS auth_password_credentials;
DROP TABLE IF EXISTS auth_external_identities;
DROP TABLE IF EXISTS auth_users_by_email;
DROP TABLE IF EXISTS auth_users;
DROP TABLE IF EXISTS checkpoints;
DROP TABLE IF EXISTS events;
DROP TABLE IF EXISTS auth_schema_migrations;

CREATE TABLE auth_schema_migrations (
    version TEXT PRIMARY KEY,
    applied_at_ms BIGINT NOT NULL
);

CREATE TABLE events (
    sequence BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    aggregate_id TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    revision BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    event_version INT NOT NULL,
    payload TEXT NOT NULL,
    metadata TEXT NOT NULL,
    recorded_at_ms BIGINT NOT NULL,
    UNIQUE (aggregate_type, aggregate_id, revision)
);
CREATE INDEX idx_auth_events_aggregate ON events (aggregate_type, aggregate_id);
CREATE INDEX idx_auth_events_type_sequence ON events (aggregate_type, sequence);

CREATE TABLE checkpoints (
    projection_name TEXT PRIMARY KEY,
    last_sequence BIGINT NOT NULL
);

CREATE TABLE auth_users (
    user_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    primary_email TEXT NOT NULL,
    disabled BIGINT NOT NULL DEFAULT 0,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL
);

CREATE TABLE auth_users_by_email (
    tenant_id TEXT NOT NULL,
    normalized_email TEXT NOT NULL,
    user_id TEXT NOT NULL,
    PRIMARY KEY (tenant_id, normalized_email)
);

CREATE TABLE auth_external_identities (
    tenant_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    provider_subject TEXT NOT NULL,
    user_id TEXT NOT NULL,
    primary_email TEXT,
    profile_json TEXT NOT NULL DEFAULT '{}',
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, provider_id, provider_subject)
);

CREATE TABLE auth_password_credentials (
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    revoked_at_ms BIGINT,
    last_authenticated_at_ms BIGINT,
    PRIMARY KEY (tenant_id, user_id)
);

CREATE TABLE auth_sessions (
    session_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    primary_email TEXT,
    expires_at_ms BIGINT NOT NULL,
    revoked_at_ms BIGINT,
    permissions_json TEXT NOT NULL DEFAULT '[]',
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL
);

CREATE TABLE auth_refresh_token_hashes (
    tenant_id TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    session_id TEXT NOT NULL,
    expires_at_ms BIGINT NOT NULL,
    rotated_at_ms BIGINT,
    revoked_at_ms BIGINT,
    created_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, token_hash)
);

CREATE TABLE auth_signing_keys (
    tenant_id TEXT NOT NULL,
    kid TEXT NOT NULL,
    alg TEXT,
    status TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    activated_at_ms BIGINT,
    retired_at_ms BIGINT,
    revoked_at_ms BIGINT,
    PRIMARY KEY (tenant_id, kid)
);

CREATE TABLE auth_jwks (
    kid TEXT PRIMARY KEY,
    kty TEXT NOT NULL,
    alg TEXT NOT NULL,
    use_value TEXT NOT NULL,
    public_parameters_json TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    retired_at_ms BIGINT
);

CREATE TABLE auth_provider_configs (
    tenant_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    login_url TEXT NOT NULL,
    enabled BIGINT NOT NULL DEFAULT 0,
    issuer_url TEXT,
    client_id TEXT,
    secret_ref TEXT,
    scopes_json TEXT NOT NULL DEFAULT '[]',
    redirect_uris_json TEXT NOT NULL DEFAULT '[]',
    claim_mapping_json TEXT NOT NULL DEFAULT '{}',
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, provider_id)
);

CREATE TABLE auth_passkey_credentials (
    tenant_id TEXT NOT NULL,
    credential_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    public_key_json TEXT NOT NULL,
    transports_json TEXT NOT NULL DEFAULT '[]',
    sign_count BIGINT NOT NULL DEFAULT 0,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, credential_id)
);
CREATE INDEX idx_auth_passkey_credentials_user ON auth_passkey_credentials (tenant_id, user_id);

CREATE TABLE auth_token_grants (
    grant_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    grant_type TEXT NOT NULL,
    subject_hint TEXT,
    redirect_url TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    expires_at_ms BIGINT NOT NULL,
    consumed_at_ms BIGINT,
    created_at_ms BIGINT NOT NULL
);

CREATE TABLE auth_redirect_allowlists (
    tenant_id TEXT PRIMARY KEY,
    redirects_json TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL
);

CREATE TABLE authz_models (
    tenant_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    schema_json TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, model_id)
);

CREATE TABLE authz_active_model (
    tenant_id TEXT PRIMARY KEY,
    model_id TEXT NOT NULL,
    activated_at_ms BIGINT NOT NULL
);

CREATE TABLE authz_relationship_tuples (
    tenant_id TEXT NOT NULL,
    subject_ref TEXT NOT NULL,
    relation TEXT NOT NULL,
    object_ref TEXT NOT NULL,
    condition_name TEXT,
    context_json TEXT NOT NULL DEFAULT '{}',
    created_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, object_ref, relation, subject_ref)
);
CREATE INDEX idx_authz_tuple_by_subject ON authz_relationship_tuples (tenant_id, subject_ref, relation, object_ref);
CREATE INDEX idx_authz_tuple_by_object ON authz_relationship_tuples (tenant_id, object_ref, relation, subject_ref);

CREATE TABLE authz_tuple_index_by_subject (
    tenant_id TEXT NOT NULL,
    subject_ref TEXT NOT NULL,
    relation TEXT NOT NULL,
    object_ref TEXT NOT NULL,
    PRIMARY KEY (tenant_id, subject_ref, relation, object_ref)
);

CREATE TABLE authz_tuple_index_by_object (
    tenant_id TEXT NOT NULL,
    object_ref TEXT NOT NULL,
    relation TEXT NOT NULL,
    subject_ref TEXT NOT NULL,
    PRIMARY KEY (tenant_id, object_ref, relation, subject_ref)
);

CREATE TABLE authz_check_audit (
    tenant_id TEXT NOT NULL,
    check_id TEXT NOT NULL,
    subject_ref TEXT NOT NULL,
    relation TEXT NOT NULL,
    object_ref TEXT NOT NULL,
    allowed BIGINT NOT NULL,
    reason TEXT,
    checked_at_ms BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, check_id)
);
SQL
  echo "Auth-stack PostgreSQL schema reset."
}

case "$BACKEND" in
  sqlite)
    echo "Resetting auth-stack local SQLite data..."
    rm -f .spin/sqlite_db.db
    rm -f .spin/sqlite_key_value.db
    echo "Auth-stack SQLite data reset."
    ;;
  postgres)
    postgres_reset
    ;;
  mysql)
    mysql_reset
    ;;
  *)
    echo "Error: unsupported AUTH_DB=$BACKEND. Use sqlite, postgres, or mysql." >&2
    exit 2
    ;;
esac
