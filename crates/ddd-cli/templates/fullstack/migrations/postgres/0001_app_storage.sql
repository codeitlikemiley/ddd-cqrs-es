CREATE SCHEMA IF NOT EXISTS fullstack_app;

CREATE TABLE IF NOT EXISTS fullstack_app.schema_migrations (
    version TEXT PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS fullstack_app.dashboard_layouts (
    organization_id UUID PRIMARY KEY REFERENCES auth_organizations(organization_id) ON DELETE CASCADE,
    payload JSONB NOT NULL,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS fullstack_app.dashboard_notifications (
    organization_id UUID NOT NULL REFERENCES auth_organizations(organization_id) ON DELETE CASCADE,
    notification_id TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (organization_id, notification_id)
);

CREATE TABLE IF NOT EXISTS fullstack_app.resources (
    organization_id UUID NOT NULL REFERENCES auth_organizations(organization_id) ON DELETE CASCADE,
    resource_id TEXT NOT NULL,
    payload JSONB NOT NULL,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (organization_id, resource_id)
);

CREATE TABLE IF NOT EXISTS fullstack_app.queries (
    organization_id UUID NOT NULL REFERENCES auth_organizations(organization_id) ON DELETE CASCADE,
    query_id TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    payload JSONB NOT NULL,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (organization_id, query_id),
    FOREIGN KEY (organization_id, resource_id)
        REFERENCES fullstack_app.resources(organization_id, resource_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS fullstack_app.vault_secrets (
    organization_id UUID NOT NULL REFERENCES auth_organizations(organization_id) ON DELETE CASCADE,
    secret_id TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    label TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    scope TEXT NOT NULL DEFAULT 'organization',
    ciphertext BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    key_version TEXT NOT NULL,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (organization_id, secret_id)
);

ALTER TABLE fullstack_app.vault_secrets
    ADD COLUMN IF NOT EXISTS secret_key TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS scope TEXT NOT NULL DEFAULT 'organization';

CREATE TABLE IF NOT EXISTS fullstack_app.query_secret_refs (
    organization_id UUID NOT NULL,
    query_id TEXT NOT NULL,
    secret_id TEXT NOT NULL,
    PRIMARY KEY (organization_id, query_id, secret_id),
    FOREIGN KEY (organization_id, query_id)
        REFERENCES fullstack_app.queries(organization_id, query_id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, secret_id)
        REFERENCES fullstack_app.vault_secrets(organization_id, secret_id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS fullstack_app.legacy_import_receipts (
    organization_id UUID NOT NULL REFERENCES auth_organizations(organization_id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    source_digest TEXT NOT NULL,
    imported_by UUID NOT NULL REFERENCES auth_users(user_id),
    imported_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    details JSONB NOT NULL DEFAULT '{}'::JSONB,
    PRIMARY KEY (organization_id, source_kind, source_digest)
);

CREATE TABLE IF NOT EXISTS fullstack_app.security_audit (
    audit_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    organization_id UUID REFERENCES auth_organizations(organization_id) ON DELETE SET NULL,
    actor_user_id UUID REFERENCES auth_users(user_id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT,
    outcome TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS security_audit_org_time_idx
    ON fullstack_app.security_audit (organization_id, occurred_at DESC);

INSERT INTO fullstack_app.schema_migrations (version)
VALUES ('0001_app_storage')
ON CONFLICT (version) DO NOTHING;
