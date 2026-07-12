transport ?= both
listen ?= 127.0.0.1:3008

-include .env
export

db ?= $(if $(DATABASE_BACKEND),$(DATABASE_BACKEND),sqlite)

AUTH_PRODUCTION_MODE ?= false
AUTH_ENABLE_PASSWORD_LOGIN ?= true
AUTH_PASSWORD_MIN_LENGTH ?= 15
AUTH_JWT_ISSUER ?= http://127.0.0.1:3008
AUTH_JWT_AUDIENCE ?= fullstack-app
AUTH_JWT_KID ?= fullstack-app-dev-hs256
AUTH_JWT_SECRET ?= dev-fullstack-app-secret-change-me
AUTH_JWT_ALGORITHM ?= HS256
AUTH_JWT_PRIVATE_KEY_DER_BASE64 ?=
AUTH_JWT_PUBLIC_JWKS_JSON ?=
AUTH_JWT_KEY_RING_JSON ?=
AUTH_BOOTSTRAP_ADMIN_EMAILS ?=
AUTH_CSRF_SECRET ?=
AUTH_VAULT_KEY_BASE64 ?=
AUTH_RECOVERY_CODE_PEPPER_BASE64 ?=
AUTH_MFA_ISSUER ?= wasi-auth
AUTH_DEV_TOOLS ?= true
AUTH_MAIL_TRANSPORT ?= capture
AUTH_SMTP_URL ?=
AUTH_MAIL_HTTP_URL ?=
AUTH_MAIL_HTTP_TOKEN ?=
AUTH_SPICEDB_ENABLED ?= false
AUTH_SPICEDB_CHECK_URL ?=
AUTH_SPICEDB_WRITE_URL ?=
AUTH_SPICEDB_TOKEN ?=
AUTH_SPICEDB_MEMBERSHIP_PERMISSION ?= member
AUTH_SESSION_TTL_SECONDS ?= 3600
AUTH_ACCESS_TOKEN_TTL_SECONDS ?= 900
AUTH_REFRESH_TOKEN_TTL_SECONDS ?= 2592000
AUTH_STORAGE_AUTO_CATCH_UP ?= true
AUTH_COOKIE_SECURE ?= false
AUTH_PUBLIC_BASE_URL ?= http://127.0.0.1:3008
AUTH_ENABLE_OAUTH ?= false
AUTH_OAUTH_DEVELOPMENT_CALLBACK_BYPASS ?= false
AUTH_ENABLE_PASSKEYS ?= false
AUTH_PASSKEY_RP_ID ?= localhost
AUTH_PASSKEY_RP_NAME ?= wasi-auth
AUTH_PASSKEY_ORIGIN ?= http://localhost:3008
AUTH_PASSKEY_REQUIRE_USER_VERIFICATION ?= true
AUTH_PASSKEY_REQUIRE_USER_HANDLE ?= false
AUTH_PASSKEY_STRICT_BASE64 ?= true
AUTH_PASSKEY_AUTHENTICATOR_ATTACHMENT ?= platform
AUTH_PASSKEY_CHALLENGE_TTL_SECONDS ?= 300
AUTH_GOOGLE_ENABLED ?= false
AUTH_GOOGLE_CLIENT_ID ?=
AUTH_GOOGLE_CLIENT_SECRET ?=
AUTH_GOOGLE_ISSUER ?= https://accounts.google.com
AUTH_GOOGLE_AUTHORIZATION_URL ?= https://accounts.google.com/o/oauth2/v2/auth
AUTH_GOOGLE_TOKEN_URL ?= https://oauth2.googleapis.com/token
AUTH_GOOGLE_JWKS_URL ?= https://www.googleapis.com/oauth2/v3/certs
AUTH_GOOGLE_JWKS_JSON ?=
AUTH_GOOGLE_USERINFO_URL ?= https://openidconnect.googleapis.com/v1/userinfo
AUTH_GOOGLE_SCOPES ?= openid email profile
AUTH_GOOGLE_REDIRECT_URI ?= http://127.0.0.1:3008/api/auth/oauth/google/callback
AUTH_FACEBOOK_ENABLED ?= false
AUTH_FACEBOOK_CLIENT_ID ?=
AUTH_FACEBOOK_CLIENT_SECRET ?=
AUTH_FACEBOOK_ISSUER ?= https://www.facebook.com
AUTH_FACEBOOK_AUTHORIZATION_URL ?= https://www.facebook.com/v20.0/dialog/oauth
AUTH_FACEBOOK_TOKEN_URL ?= https://graph.facebook.com/v20.0/oauth/access_token
AUTH_FACEBOOK_USERINFO_URL ?= https://graph.facebook.com/v20.0/me?fields=id,email,name
AUTH_FACEBOOK_SCOPES ?= email public_profile
AUTH_FACEBOOK_REDIRECT_URI ?= http://127.0.0.1:3008/api/auth/oauth/facebook/callback
AUTH_APPLE_ENABLED ?= false
AUTH_APPLE_CLIENT_ID ?=
AUTH_APPLE_GENERATED_CLIENT_SECRET ?=
AUTH_APPLE_TEAM_ID ?=
AUTH_APPLE_KEY_ID ?=
AUTH_APPLE_PRIVATE_KEY ?=
AUTH_APPLE_CLIENT_SECRET_TTL_SECONDS ?= 86400
AUTH_APPLE_ISSUER ?= https://appleid.apple.com
AUTH_APPLE_AUTHORIZATION_URL ?= https://appleid.apple.com/auth/authorize
AUTH_APPLE_TOKEN_URL ?= https://appleid.apple.com/auth/token
AUTH_APPLE_JWKS_URL ?= https://appleid.apple.com/auth/keys
AUTH_APPLE_JWKS_JSON ?=
AUTH_APPLE_SCOPES ?= openid email name
AUTH_APPLE_REDIRECT_URI ?= http://127.0.0.1:3008/api/auth/oauth/apple/callback
POSTGRES_URL ?=

AUTH_DATABASE_URL :=
ifeq ($(db),postgres)
AUTH_DATABASE_URL := $(POSTGRES_URL)
endif

MAIL_FEATURE := mail-$(AUTH_MAIL_TRANSPORT)
OPTIONAL_SPICEDB_FEATURE :=
ifeq ($(AUTH_SPICEDB_ENABLED),true)
OPTIONAL_SPICEDB_FEATURE := ,spicedb
endif
BASE_FEATURES := ssr,$(db),$(MAIL_FEATURE)$(OPTIONAL_SPICEDB_FEATURE)
GRPC_FEATURES := ssr,$(db),spin-grpc,$(MAIL_FEATURE)$(OPTIONAL_SPICEDB_FEATURE)
SPIN_VARIABLE_ARGS := \
	--variable database_backend=$(db) \
	--variable database_url='$(AUTH_DATABASE_URL)' \
	--variable auth_transport=$(transport) \
	--variable auth_production_mode=$(AUTH_PRODUCTION_MODE) \
	--variable auth_enable_password_login=$(AUTH_ENABLE_PASSWORD_LOGIN) \
	--variable auth_password_min_length=$(AUTH_PASSWORD_MIN_LENGTH) \
	--variable auth_jwt_issuer=$(AUTH_JWT_ISSUER) \
	--variable auth_jwt_audience=$(AUTH_JWT_AUDIENCE) \
	--variable auth_jwt_kid=$(AUTH_JWT_KID) \
	--variable auth_jwt_secret=$(AUTH_JWT_SECRET) \
	--variable auth_jwt_algorithm=$(AUTH_JWT_ALGORITHM) \
	--variable auth_jwt_private_key_der_base64=$(AUTH_JWT_PRIVATE_KEY_DER_BASE64) \
	--variable auth_jwt_public_jwks_json='$(AUTH_JWT_PUBLIC_JWKS_JSON)' \
	--variable auth_jwt_key_ring_json='$(AUTH_JWT_KEY_RING_JSON)' \
	--variable auth_bootstrap_admin_emails='$(AUTH_BOOTSTRAP_ADMIN_EMAILS)' \
	--variable auth_csrf_secret='$(AUTH_CSRF_SECRET)' \
	--variable auth_vault_key_base64='$(AUTH_VAULT_KEY_BASE64)' \
	--variable auth_recovery_code_pepper_base64='$(AUTH_RECOVERY_CODE_PEPPER_BASE64)' \
	--variable auth_mfa_issuer='$(AUTH_MFA_ISSUER)' \
	--variable auth_session_ttl_seconds=$(AUTH_SESSION_TTL_SECONDS) \
	--variable auth_access_token_ttl_seconds=$(AUTH_ACCESS_TOKEN_TTL_SECONDS) \
	--variable auth_refresh_token_ttl_seconds=$(AUTH_REFRESH_TOKEN_TTL_SECONDS) \
	--variable auth_storage_auto_catch_up=$(AUTH_STORAGE_AUTO_CATCH_UP) \
	--variable auth_cookie_secure=$(AUTH_COOKIE_SECURE) \
	--variable auth_public_base_url=$(AUTH_PUBLIC_BASE_URL) \
	--variable auth_dev_tools=$(AUTH_DEV_TOOLS) \
	--variable auth_mail_transport=$(AUTH_MAIL_TRANSPORT) \
	--variable auth_smtp_url='$(AUTH_SMTP_URL)' \
	--variable auth_mail_http_url='$(AUTH_MAIL_HTTP_URL)' \
	--variable auth_mail_http_token='$(AUTH_MAIL_HTTP_TOKEN)' \
	--variable auth_spicedb_enabled=$(AUTH_SPICEDB_ENABLED) \
	--variable auth_spicedb_check_url='$(AUTH_SPICEDB_CHECK_URL)' \
	--variable auth_spicedb_write_url='$(AUTH_SPICEDB_WRITE_URL)' \
	--variable auth_spicedb_token='$(AUTH_SPICEDB_TOKEN)' \
	--variable auth_spicedb_membership_permission='$(AUTH_SPICEDB_MEMBERSHIP_PERMISSION)' \
	--variable auth_enable_oauth=$(AUTH_ENABLE_OAUTH) \
	--variable auth_oauth_development_callback_bypass=$(AUTH_OAUTH_DEVELOPMENT_CALLBACK_BYPASS) \
	--variable auth_enable_passkeys=$(AUTH_ENABLE_PASSKEYS) \
	--variable auth_passkey_rp_id=$(AUTH_PASSKEY_RP_ID) \
	--variable auth_passkey_rp_name=$(AUTH_PASSKEY_RP_NAME) \
	--variable auth_passkey_origin=$(AUTH_PASSKEY_ORIGIN) \
	--variable auth_passkey_require_user_verification=$(AUTH_PASSKEY_REQUIRE_USER_VERIFICATION) \
	--variable auth_passkey_require_user_handle=$(AUTH_PASSKEY_REQUIRE_USER_HANDLE) \
	--variable auth_passkey_strict_base64=$(AUTH_PASSKEY_STRICT_BASE64) \
	--variable auth_passkey_authenticator_attachment=$(AUTH_PASSKEY_AUTHENTICATOR_ATTACHMENT) \
	--variable auth_passkey_challenge_ttl_seconds=$(AUTH_PASSKEY_CHALLENGE_TTL_SECONDS) \
	--variable auth_google_enabled=$(AUTH_GOOGLE_ENABLED) \
	--variable auth_google_client_id=$(AUTH_GOOGLE_CLIENT_ID) \
	--variable auth_google_client_secret=$(AUTH_GOOGLE_CLIENT_SECRET) \
	--variable auth_google_issuer=$(AUTH_GOOGLE_ISSUER) \
	--variable auth_google_authorization_url=$(AUTH_GOOGLE_AUTHORIZATION_URL) \
	--variable auth_google_token_url=$(AUTH_GOOGLE_TOKEN_URL) \
	--variable auth_google_jwks_url=$(AUTH_GOOGLE_JWKS_URL) \
	--variable auth_google_jwks_json='$(AUTH_GOOGLE_JWKS_JSON)' \
	--variable auth_google_userinfo_url=$(AUTH_GOOGLE_USERINFO_URL) \
	--variable auth_google_scopes='$(AUTH_GOOGLE_SCOPES)' \
	--variable auth_google_redirect_uri=$(AUTH_GOOGLE_REDIRECT_URI) \
	--variable auth_facebook_enabled=$(AUTH_FACEBOOK_ENABLED) \
	--variable auth_facebook_client_id=$(AUTH_FACEBOOK_CLIENT_ID) \
	--variable auth_facebook_client_secret=$(AUTH_FACEBOOK_CLIENT_SECRET) \
	--variable auth_facebook_issuer=$(AUTH_FACEBOOK_ISSUER) \
	--variable auth_facebook_authorization_url=$(AUTH_FACEBOOK_AUTHORIZATION_URL) \
	--variable auth_facebook_token_url=$(AUTH_FACEBOOK_TOKEN_URL) \
	--variable auth_facebook_userinfo_url='$(AUTH_FACEBOOK_USERINFO_URL)' \
	--variable auth_facebook_scopes='$(AUTH_FACEBOOK_SCOPES)' \
	--variable auth_facebook_redirect_uri=$(AUTH_FACEBOOK_REDIRECT_URI) \
	--variable auth_apple_enabled=$(AUTH_APPLE_ENABLED) \
	--variable auth_apple_client_id=$(AUTH_APPLE_CLIENT_ID) \
	--variable auth_apple_generated_client_secret=$(AUTH_APPLE_GENERATED_CLIENT_SECRET) \
	--variable auth_apple_team_id=$(AUTH_APPLE_TEAM_ID) \
	--variable auth_apple_key_id=$(AUTH_APPLE_KEY_ID) \
	--variable auth_apple_private_key='$(AUTH_APPLE_PRIVATE_KEY)' \
	--variable auth_apple_client_secret_ttl_seconds=$(AUTH_APPLE_CLIENT_SECRET_TTL_SECONDS) \
	--variable auth_apple_issuer=$(AUTH_APPLE_ISSUER) \
	--variable auth_apple_authorization_url=$(AUTH_APPLE_AUTHORIZATION_URL) \
	--variable auth_apple_token_url=$(AUTH_APPLE_TOKEN_URL) \
	--variable auth_apple_jwks_url=$(AUTH_APPLE_JWKS_URL) \
	--variable auth_apple_jwks_json='$(AUTH_APPLE_JWKS_JSON)' \
	--variable auth_apple_scopes='$(AUTH_APPLE_SCOPES)' \
	--variable auth_apple_redirect_uri=$(AUTH_APPLE_REDIRECT_URI)

.PHONY: help validate-toolchain validate-mail check grpc-check spin fresh smoke oauth-credentials oauth-preflight oauth-evidence oauth-callback oauth-browser-smoke oauth-dev-browser-smoke browser-smoke passkey-browser-smoke

help:
	@echo "Fullstack targets:"
	@echo "  make check                 Compile Spin HTTP/Leptos shell with db=$(db)"
	@echo "  make grpc-check            Compile Spin gRPC services with db=$(db)"
	@echo "  make spin transport=both   Build and run the Spin fullstack app on $(listen)"
	@echo "  make fresh db=sqlite       Erase local data; migrate from wasi-auth on next startup"
	@echo "  make fresh db=postgres     Erase PostgreSQL data using POSTGRES_URL"
	@echo "  make smoke                 Run REST/web route smoke checks against BASE_URL"
	@echo "  make oauth-credentials     Check required live OAuth credential variables"
	@echo "  make oauth-preflight       Validate live OAuth provider start URLs against BASE_URL"
	@echo "  make oauth-evidence        Report redacted OAuth storage evidence"
	@echo "  make oauth-callback        Verify live OAuth callback session and storage evidence"
	@echo "  make oauth-browser-smoke   Open browser, complete live OAuth, and verify evidence"
	@echo "  make oauth-dev-browser-smoke Verify OAuth UI with development callback bypass"
	@echo "  make browser-smoke         Run Playwright page and middleware checks"
	@echo "  make passkey-browser-smoke Run Playwright WebAuthn virtual authenticator checks"

validate-toolchain:
	bash scripts/verify_toolchain.sh

validate-mail:
	@case "$(AUTH_MAIL_TRANSPORT)" in \
		capture|smtp|http) ;; \
		*) echo "Error: unsupported AUTH_MAIL_TRANSPORT=$(AUTH_MAIL_TRANSPORT). Use capture, smtp, or http."; exit 2 ;; \
	esac

check: validate-toolchain validate-mail
	WASI_RUNTIME=spin AUTH_TRANSPORT=$(transport) cargo check --target wasm32-wasip2 --no-default-features --features $(BASE_FEATURES)

grpc-check: validate-toolchain validate-mail
	WASI_RUNTIME=spin AUTH_TRANSPORT=grpc cargo check --target wasm32-wasip2 --no-default-features --features $(GRPC_FEATURES)

spin: validate-toolchain validate-mail
	@case "$(db)" in \
		sqlite) ;; \
		postgres) \
			if [ -z "$$POSTGRES_URL" ]; then \
				echo "Error: POSTGRES_URL is required for make spin db=postgres."; \
				exit 1; \
			fi ;; \
		*) echo "Error: unsupported db=$(db). Use sqlite or postgres."; exit 2 ;; \
	esac
	FULLSTACK_FEATURES=$(GRPC_FEATURES) AUTH_TRANSPORT=$(transport) spin up --build --listen $(listen) $(SPIN_VARIABLE_ARGS)

fresh:
	@case "$(db)" in \
		sqlite) AUTH_DB=sqlite bash scripts/reset_db.sh ;; \
		postgres) \
			if [ -z "$$POSTGRES_URL" ]; then \
				echo "Error: POSTGRES_URL is required for db=postgres."; \
				exit 1; \
			fi; \
			AUTH_DB=postgres DATABASE_URL="$$POSTGRES_URL" bash scripts/reset_db.sh ;; \
		*) echo "Error: unsupported db=$(db). Use sqlite or postgres."; exit 2 ;; \
	esac

smoke:
	bash scripts/verify_fullstack.sh

oauth-credentials:
	bash scripts/verify_oauth_credentials.sh

oauth-preflight: oauth-credentials
	bash scripts/verify_live_oauth_preflight.sh

oauth-evidence:
	bash scripts/report_oauth_evidence.sh

oauth-callback: oauth-credentials
	bash scripts/verify_live_oauth_callback.sh

oauth-browser-smoke: oauth-preflight
	npm install
	npm exec -- playwright install chromium
	npm run live-oauth-smoke

oauth-dev-browser-smoke:
	npm install
	npm exec -- playwright install chromium
	npm run oauth-dev-smoke

browser-smoke:
	npm install
	npm exec -- playwright install chromium
	npm run browser-smoke

passkey-browser-smoke:
	npm install
	npm exec -- playwright install chromium
	npm run passkey-smoke
