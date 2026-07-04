use crate::{
    AuthError, AuthProviderConfig, AuthProviderId, ExternalSubjectId, PasskeyCredentialId,
    SessionId, SigningKeyId, TenantId, UserId,
};
use ddd_cqrs_es::{Aggregate, DomainEvent};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserCommand {
    RegisterUser {
        user_id: UserId,
        tenant_id: TenantId,
        primary_email: String,
    },
    DisableUser,
    EnableUser,
    ChangePrimaryEmail {
        primary_email: String,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserEvent {
    UserRegistered {
        user_id: UserId,
        tenant_id: TenantId,
        primary_email: String,
    },
    UserDisabled,
    UserEnabled,
    PrimaryEmailChanged {
        primary_email: String,
    },
}

impl DomainEvent for UserEvent {
    fn event_type(&self) -> &'static str {
        match self {
            Self::UserRegistered { .. } => "auth_user_registered",
            Self::UserDisabled => "auth_user_disabled",
            Self::UserEnabled => "auth_user_enabled",
            Self::PrimaryEmailChanged { .. } => "auth_user_primary_email_changed",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct User {
    pub user_id: Option<UserId>,
    pub tenant_id: Option<TenantId>,
    pub primary_email: Option<String>,
    pub disabled: bool,
    revision: u64,
}

impl Aggregate for User {
    type Id = UserId;
    type Command = UserCommand;
    type Event = UserEvent;
    type Error = AuthError;

    fn aggregate_type() -> &'static str {
        "auth_user"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            UserEvent::UserRegistered {
                user_id,
                tenant_id,
                primary_email,
            } => {
                self.user_id = Some(user_id.clone());
                self.tenant_id = Some(tenant_id.clone());
                self.primary_email = Some(primary_email.clone());
                self.disabled = false;
            }
            UserEvent::UserDisabled => {
                self.disabled = true;
            }
            UserEvent::UserEnabled => {
                self.disabled = false;
            }
            UserEvent::PrimaryEmailChanged { primary_email } => {
                self.primary_email = Some(primary_email.clone());
            }
        }
        self.revision += 1;
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            UserCommand::RegisterUser {
                user_id,
                tenant_id,
                primary_email,
            } => {
                validate_email(&primary_email)?;
                if self.user_id.is_some() {
                    return Err(AuthError::AlreadyRegistered);
                }
                Ok(vec![UserEvent::UserRegistered {
                    user_id,
                    tenant_id,
                    primary_email,
                }])
            }
            UserCommand::DisableUser => {
                self.ensure_registered()?;
                if self.disabled {
                    return Ok(Vec::new());
                }
                Ok(vec![UserEvent::UserDisabled])
            }
            UserCommand::EnableUser => {
                self.ensure_registered()?;
                if !self.disabled {
                    return Ok(Vec::new());
                }
                Ok(vec![UserEvent::UserEnabled])
            }
            UserCommand::ChangePrimaryEmail { primary_email } => {
                self.ensure_registered()?;
                if self.disabled {
                    return Err(AuthError::UserDisabled);
                }
                validate_email(&primary_email)?;
                if self.primary_email.as_deref() == Some(primary_email.as_str()) {
                    return Ok(Vec::new());
                }
                Ok(vec![UserEvent::PrimaryEmailChanged { primary_email }])
            }
        }
    }

    fn new() -> Self {
        Self::default()
    }
}

impl User {
    fn ensure_registered(&self) -> Result<(), AuthError> {
        if self.user_id.is_some() {
            Ok(())
        } else {
            Err(AuthError::UserNotRegistered)
        }
    }
}

fn validate_email(value: &str) -> Result<(), AuthError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || !trimmed.contains('@') {
        Err(AuthError::validation(
            "primary email must be a valid email address",
        ))
    } else {
        Ok(())
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasswordCredentialCommand {
    SetPasswordHash {
        user_id: UserId,
        tenant_id: TenantId,
        password_hash: String,
        hash_algorithm: String,
        changed_at_ms: u64,
    },
    MarkAuthenticated {
        authenticated_at_ms: u64,
    },
    Revoke {
        revoked_at_ms: u64,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasswordCredentialEvent {
    PasswordHashSet {
        user_id: UserId,
        tenant_id: TenantId,
        password_hash: String,
        hash_algorithm: String,
        changed_at_ms: u64,
    },
    PasswordAuthenticated {
        authenticated_at_ms: u64,
    },
    PasswordCredentialRevoked {
        revoked_at_ms: u64,
    },
}

impl DomainEvent for PasswordCredentialEvent {
    fn event_type(&self) -> &'static str {
        match self {
            Self::PasswordHashSet { .. } => "auth_password_hash_set",
            Self::PasswordAuthenticated { .. } => "auth_password_authenticated",
            Self::PasswordCredentialRevoked { .. } => "auth_password_credential_revoked",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PasswordCredential {
    pub user_id: Option<UserId>,
    pub tenant_id: Option<TenantId>,
    pub password_hash: Option<String>,
    pub hash_algorithm: Option<String>,
    pub revoked_at_ms: Option<u64>,
    pub last_authenticated_at_ms: Option<u64>,
    revision: u64,
}

impl Aggregate for PasswordCredential {
    type Id = UserId;
    type Command = PasswordCredentialCommand;
    type Event = PasswordCredentialEvent;
    type Error = AuthError;

    fn aggregate_type() -> &'static str {
        "auth_password_credential"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            PasswordCredentialEvent::PasswordHashSet {
                user_id,
                tenant_id,
                password_hash,
                hash_algorithm,
                ..
            } => {
                self.user_id = Some(user_id.clone());
                self.tenant_id = Some(tenant_id.clone());
                self.password_hash = Some(password_hash.clone());
                self.hash_algorithm = Some(hash_algorithm.clone());
                self.revoked_at_ms = None;
            }
            PasswordCredentialEvent::PasswordAuthenticated {
                authenticated_at_ms,
            } => {
                self.last_authenticated_at_ms = Some(*authenticated_at_ms);
            }
            PasswordCredentialEvent::PasswordCredentialRevoked { revoked_at_ms } => {
                self.revoked_at_ms = Some(*revoked_at_ms);
            }
        }
        self.revision += 1;
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            PasswordCredentialCommand::SetPasswordHash {
                user_id,
                tenant_id,
                password_hash,
                hash_algorithm,
                changed_at_ms,
            } => {
                validate_non_empty("password_hash", &password_hash)?;
                validate_non_empty("hash_algorithm", &hash_algorithm)?;
                Ok(vec![PasswordCredentialEvent::PasswordHashSet {
                    user_id,
                    tenant_id,
                    password_hash,
                    hash_algorithm,
                    changed_at_ms,
                }])
            }
            PasswordCredentialCommand::MarkAuthenticated {
                authenticated_at_ms,
            } => {
                self.ensure_exists()?;
                self.ensure_not_revoked()?;
                Ok(vec![PasswordCredentialEvent::PasswordAuthenticated {
                    authenticated_at_ms,
                }])
            }
            PasswordCredentialCommand::Revoke { revoked_at_ms } => {
                self.ensure_exists()?;
                if self.revoked_at_ms.is_some() {
                    return Ok(Vec::new());
                }
                Ok(vec![PasswordCredentialEvent::PasswordCredentialRevoked {
                    revoked_at_ms,
                }])
            }
        }
    }

    fn new() -> Self {
        Self::default()
    }
}

impl PasswordCredential {
    fn ensure_exists(&self) -> Result<(), AuthError> {
        if self.password_hash.is_some() {
            Ok(())
        } else {
            Err(AuthError::UserNotRegistered)
        }
    }

    fn ensure_not_revoked(&self) -> Result<(), AuthError> {
        if self.revoked_at_ms.is_none() {
            Ok(())
        } else {
            Err(AuthError::SessionRevoked)
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalIdentityCommand {
    Link {
        provider_id: AuthProviderId,
        provider_subject: ExternalSubjectId,
        user_id: UserId,
        tenant_id: TenantId,
        primary_email: Option<String>,
        profile_json: Option<String>,
        linked_at_ms: u64,
    },
    UpdateProfile {
        primary_email: Option<String>,
        profile_json: Option<String>,
        updated_at_ms: u64,
    },
    Unlink {
        unlinked_at_ms: u64,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalIdentityEvent {
    ExternalIdentityLinked {
        provider_id: AuthProviderId,
        provider_subject: ExternalSubjectId,
        user_id: UserId,
        tenant_id: TenantId,
        primary_email: Option<String>,
        profile_json: Option<String>,
        linked_at_ms: u64,
    },
    ExternalIdentityProfileUpdated {
        primary_email: Option<String>,
        profile_json: Option<String>,
        updated_at_ms: u64,
    },
    ExternalIdentityUnlinked {
        unlinked_at_ms: u64,
    },
}

impl DomainEvent for ExternalIdentityEvent {
    fn event_type(&self) -> &'static str {
        match self {
            Self::ExternalIdentityLinked { .. } => "auth_external_identity_linked",
            Self::ExternalIdentityProfileUpdated { .. } => "auth_external_identity_profile_updated",
            Self::ExternalIdentityUnlinked { .. } => "auth_external_identity_unlinked",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalIdentity {
    pub provider_id: Option<AuthProviderId>,
    pub provider_subject: Option<ExternalSubjectId>,
    pub user_id: Option<UserId>,
    pub tenant_id: Option<TenantId>,
    pub primary_email: Option<String>,
    pub profile_json: Option<String>,
    pub unlinked_at_ms: Option<u64>,
    revision: u64,
}

impl Aggregate for ExternalIdentity {
    type Id = ExternalSubjectId;
    type Command = ExternalIdentityCommand;
    type Event = ExternalIdentityEvent;
    type Error = AuthError;

    fn aggregate_type() -> &'static str {
        "auth_external_identity"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            ExternalIdentityEvent::ExternalIdentityLinked {
                provider_id,
                provider_subject,
                user_id,
                tenant_id,
                primary_email,
                profile_json,
                ..
            } => {
                self.provider_id = Some(provider_id.clone());
                self.provider_subject = Some(provider_subject.clone());
                self.user_id = Some(user_id.clone());
                self.tenant_id = Some(tenant_id.clone());
                self.primary_email = primary_email.clone();
                self.profile_json = profile_json.clone();
                self.unlinked_at_ms = None;
            }
            ExternalIdentityEvent::ExternalIdentityProfileUpdated {
                primary_email,
                profile_json,
                ..
            } => {
                self.primary_email = primary_email.clone();
                self.profile_json = profile_json.clone();
            }
            ExternalIdentityEvent::ExternalIdentityUnlinked { unlinked_at_ms } => {
                self.unlinked_at_ms = Some(*unlinked_at_ms);
            }
        }
        self.revision += 1;
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            ExternalIdentityCommand::Link {
                provider_id,
                provider_subject,
                user_id,
                tenant_id,
                primary_email,
                profile_json,
                linked_at_ms,
            } => {
                validate_non_empty("provider_id", provider_id.as_str())?;
                validate_non_empty("provider_subject", provider_subject.as_str())?;
                if let Some(email) = primary_email.as_deref() {
                    validate_email(email)?;
                }
                if self.user_id.is_some() && self.unlinked_at_ms.is_none() {
                    return Err(AuthError::AlreadyRegistered);
                }
                Ok(vec![ExternalIdentityEvent::ExternalIdentityLinked {
                    provider_id,
                    provider_subject,
                    user_id,
                    tenant_id,
                    primary_email,
                    profile_json,
                    linked_at_ms,
                }])
            }
            ExternalIdentityCommand::UpdateProfile {
                primary_email,
                profile_json,
                updated_at_ms,
            } => {
                self.ensure_linked()?;
                if let Some(email) = primary_email.as_deref() {
                    validate_email(email)?;
                }
                Ok(vec![
                    ExternalIdentityEvent::ExternalIdentityProfileUpdated {
                        primary_email,
                        profile_json,
                        updated_at_ms,
                    },
                ])
            }
            ExternalIdentityCommand::Unlink { unlinked_at_ms } => {
                self.ensure_linked()?;
                if self.unlinked_at_ms.is_some() {
                    return Ok(Vec::new());
                }
                Ok(vec![ExternalIdentityEvent::ExternalIdentityUnlinked {
                    unlinked_at_ms,
                }])
            }
        }
    }

    fn new() -> Self {
        Self::default()
    }
}

impl ExternalIdentity {
    fn ensure_linked(&self) -> Result<(), AuthError> {
        if self.user_id.is_some() && self.unlinked_at_ms.is_none() {
            Ok(())
        } else {
            Err(AuthError::UserNotRegistered)
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasskeyCredentialCommand {
    Register {
        credential_id: PasskeyCredentialId,
        user_id: UserId,
        tenant_id: TenantId,
        public_key_json: String,
        transports: Vec<String>,
        sign_count: u64,
        registered_at_ms: u64,
    },
    UpdateSignCount {
        sign_count: u64,
        authenticated_at_ms: u64,
    },
    Revoke {
        revoked_at_ms: u64,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasskeyCredentialEvent {
    PasskeyCredentialRegistered {
        credential_id: PasskeyCredentialId,
        user_id: UserId,
        tenant_id: TenantId,
        public_key_json: String,
        transports: Vec<String>,
        sign_count: u64,
        registered_at_ms: u64,
    },
    PasskeySignCountUpdated {
        sign_count: u64,
        authenticated_at_ms: u64,
    },
    PasskeyCredentialRevoked {
        revoked_at_ms: u64,
    },
}

impl DomainEvent for PasskeyCredentialEvent {
    fn event_type(&self) -> &'static str {
        match self {
            Self::PasskeyCredentialRegistered { .. } => "auth_passkey_credential_registered",
            Self::PasskeySignCountUpdated { .. } => "auth_passkey_sign_count_updated",
            Self::PasskeyCredentialRevoked { .. } => "auth_passkey_credential_revoked",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PasskeyCredential {
    pub credential_id: Option<PasskeyCredentialId>,
    pub user_id: Option<UserId>,
    pub tenant_id: Option<TenantId>,
    pub public_key_json: Option<String>,
    pub transports: Vec<String>,
    pub sign_count: u64,
    pub revoked_at_ms: Option<u64>,
    pub last_authenticated_at_ms: Option<u64>,
    revision: u64,
}

impl Aggregate for PasskeyCredential {
    type Id = PasskeyCredentialId;
    type Command = PasskeyCredentialCommand;
    type Event = PasskeyCredentialEvent;
    type Error = AuthError;

    fn aggregate_type() -> &'static str {
        "auth_passkey_credential"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            PasskeyCredentialEvent::PasskeyCredentialRegistered {
                credential_id,
                user_id,
                tenant_id,
                public_key_json,
                transports,
                sign_count,
                ..
            } => {
                self.credential_id = Some(credential_id.clone());
                self.user_id = Some(user_id.clone());
                self.tenant_id = Some(tenant_id.clone());
                self.public_key_json = Some(public_key_json.clone());
                self.transports = transports.clone();
                self.sign_count = *sign_count;
                self.revoked_at_ms = None;
            }
            PasskeyCredentialEvent::PasskeySignCountUpdated {
                sign_count,
                authenticated_at_ms,
            } => {
                self.sign_count = *sign_count;
                self.last_authenticated_at_ms = Some(*authenticated_at_ms);
            }
            PasskeyCredentialEvent::PasskeyCredentialRevoked { revoked_at_ms } => {
                self.revoked_at_ms = Some(*revoked_at_ms);
            }
        }
        self.revision += 1;
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            PasskeyCredentialCommand::Register {
                credential_id,
                user_id,
                tenant_id,
                public_key_json,
                transports,
                sign_count,
                registered_at_ms,
            } => {
                validate_non_empty("credential_id", credential_id.as_str())?;
                validate_non_empty("public_key_json", &public_key_json)?;
                if self.credential_id.is_some() && self.revoked_at_ms.is_none() {
                    return Err(AuthError::AlreadyRegistered);
                }
                Ok(vec![PasskeyCredentialEvent::PasskeyCredentialRegistered {
                    credential_id,
                    user_id,
                    tenant_id,
                    public_key_json,
                    transports,
                    sign_count,
                    registered_at_ms,
                }])
            }
            PasskeyCredentialCommand::UpdateSignCount {
                sign_count,
                authenticated_at_ms,
            } => {
                self.ensure_registered()?;
                self.ensure_not_revoked()?;
                if sign_count < self.sign_count {
                    return Err(AuthError::validation(
                        "passkey sign count must not move backwards",
                    ));
                }
                if sign_count == self.sign_count {
                    return Ok(Vec::new());
                }
                Ok(vec![PasskeyCredentialEvent::PasskeySignCountUpdated {
                    sign_count,
                    authenticated_at_ms,
                }])
            }
            PasskeyCredentialCommand::Revoke { revoked_at_ms } => {
                self.ensure_registered()?;
                if self.revoked_at_ms.is_some() {
                    return Ok(Vec::new());
                }
                Ok(vec![PasskeyCredentialEvent::PasskeyCredentialRevoked {
                    revoked_at_ms,
                }])
            }
        }
    }

    fn new() -> Self {
        Self::default()
    }
}

impl PasskeyCredential {
    fn ensure_registered(&self) -> Result<(), AuthError> {
        if self.credential_id.is_some() {
            Ok(())
        } else {
            Err(AuthError::UserNotRegistered)
        }
    }

    fn ensure_not_revoked(&self) -> Result<(), AuthError> {
        if self.revoked_at_ms.is_none() {
            Ok(())
        } else {
            Err(AuthError::SessionRevoked)
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionCommand {
    Issue {
        session_id: SessionId,
        user_id: UserId,
        tenant_id: TenantId,
        provider_id: Option<AuthProviderId>,
        expires_at_ms: u64,
        issued_at_ms: u64,
    },
    RotateRefreshToken {
        refresh_token_hash: String,
        refresh_token_expires_at_ms: u64,
        rotated_at_ms: u64,
    },
    Revoke {
        revoked_at_ms: u64,
    },
    Expire {
        expired_at_ms: u64,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionEvent {
    SessionIssued {
        session_id: SessionId,
        user_id: UserId,
        tenant_id: TenantId,
        provider_id: Option<AuthProviderId>,
        expires_at_ms: u64,
        issued_at_ms: u64,
    },
    RefreshTokenRotated {
        refresh_token_hash: String,
        refresh_token_expires_at_ms: u64,
        rotated_at_ms: u64,
    },
    SessionRevoked {
        revoked_at_ms: u64,
    },
    SessionExpired {
        expired_at_ms: u64,
    },
}

impl DomainEvent for SessionEvent {
    fn event_type(&self) -> &'static str {
        match self {
            Self::SessionIssued { .. } => "auth_session_issued",
            Self::RefreshTokenRotated { .. } => "auth_refresh_token_rotated",
            Self::SessionRevoked { .. } => "auth_session_revoked",
            Self::SessionExpired { .. } => "auth_session_expired",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub session_id: Option<SessionId>,
    pub user_id: Option<UserId>,
    pub tenant_id: Option<TenantId>,
    pub provider_id: Option<AuthProviderId>,
    pub expires_at_ms: Option<u64>,
    pub refresh_token_hash: Option<String>,
    pub refresh_token_expires_at_ms: Option<u64>,
    pub revoked_at_ms: Option<u64>,
    pub expired_at_ms: Option<u64>,
    revision: u64,
}

impl Aggregate for Session {
    type Id = SessionId;
    type Command = SessionCommand;
    type Event = SessionEvent;
    type Error = AuthError;

    fn aggregate_type() -> &'static str {
        "auth_session"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            SessionEvent::SessionIssued {
                session_id,
                user_id,
                tenant_id,
                provider_id,
                expires_at_ms,
                ..
            } => {
                self.session_id = Some(session_id.clone());
                self.user_id = Some(user_id.clone());
                self.tenant_id = Some(tenant_id.clone());
                self.provider_id = provider_id.clone();
                self.expires_at_ms = Some(*expires_at_ms);
                self.revoked_at_ms = None;
                self.expired_at_ms = None;
            }
            SessionEvent::RefreshTokenRotated {
                refresh_token_hash,
                refresh_token_expires_at_ms,
                ..
            } => {
                self.refresh_token_hash = Some(refresh_token_hash.clone());
                self.refresh_token_expires_at_ms = Some(*refresh_token_expires_at_ms);
            }
            SessionEvent::SessionRevoked { revoked_at_ms } => {
                self.revoked_at_ms = Some(*revoked_at_ms);
            }
            SessionEvent::SessionExpired { expired_at_ms } => {
                self.expired_at_ms = Some(*expired_at_ms);
            }
        }
        self.revision += 1;
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            SessionCommand::Issue {
                session_id,
                user_id,
                tenant_id,
                provider_id,
                expires_at_ms,
                issued_at_ms,
            } => {
                validate_non_empty("session_id", session_id.as_str())?;
                if self.session_id.is_some() && self.revoked_at_ms.is_none() {
                    return Err(AuthError::AlreadyRegistered);
                }
                Ok(vec![SessionEvent::SessionIssued {
                    session_id,
                    user_id,
                    tenant_id,
                    provider_id,
                    expires_at_ms,
                    issued_at_ms,
                }])
            }
            SessionCommand::RotateRefreshToken {
                refresh_token_hash,
                refresh_token_expires_at_ms,
                rotated_at_ms,
            } => {
                self.ensure_active()?;
                validate_non_empty("refresh_token_hash", &refresh_token_hash)?;
                Ok(vec![SessionEvent::RefreshTokenRotated {
                    refresh_token_hash,
                    refresh_token_expires_at_ms,
                    rotated_at_ms,
                }])
            }
            SessionCommand::Revoke { revoked_at_ms } => {
                self.ensure_issued()?;
                if self.revoked_at_ms.is_some() {
                    return Ok(Vec::new());
                }
                Ok(vec![SessionEvent::SessionRevoked { revoked_at_ms }])
            }
            SessionCommand::Expire { expired_at_ms } => {
                self.ensure_issued()?;
                if self.expired_at_ms.is_some() {
                    return Ok(Vec::new());
                }
                Ok(vec![SessionEvent::SessionExpired { expired_at_ms }])
            }
        }
    }

    fn new() -> Self {
        Self::default()
    }
}

impl Session {
    fn ensure_issued(&self) -> Result<(), AuthError> {
        if self.session_id.is_some() {
            Ok(())
        } else {
            Err(AuthError::SessionRevoked)
        }
    }

    fn ensure_active(&self) -> Result<(), AuthError> {
        self.ensure_issued()?;
        if self.revoked_at_ms.is_some() {
            return Err(AuthError::SessionRevoked);
        }
        if self.expired_at_ms.is_some() {
            return Err(AuthError::SessionExpired);
        }
        Ok(())
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigningKeyStatus {
    Active,
    Next,
    Retired,
    Revoked,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigningKeyState {
    pub kid: SigningKeyId,
    pub algorithm: String,
    pub status: SigningKeyStatus,
    pub public_jwk_json: Option<String>,
    pub private_key_ref: Option<String>,
    pub created_at_ms: u64,
    pub activated_at_ms: Option<u64>,
    pub retired_at_ms: Option<u64>,
    pub revoked_at_ms: Option<u64>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigningKeySetCommand {
    ProvisionKey {
        tenant_id: TenantId,
        kid: SigningKeyId,
        algorithm: String,
        public_jwk_json: Option<String>,
        private_key_ref: Option<String>,
        created_at_ms: u64,
    },
    ActivateKey {
        kid: SigningKeyId,
        retire_previous: bool,
        activated_at_ms: u64,
    },
    RetireKey {
        kid: SigningKeyId,
        retired_at_ms: u64,
    },
    RevokeKey {
        kid: SigningKeyId,
        revoked_at_ms: u64,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigningKeySetEvent {
    SigningKeyProvisioned {
        tenant_id: TenantId,
        kid: SigningKeyId,
        algorithm: String,
        public_jwk_json: Option<String>,
        private_key_ref: Option<String>,
        created_at_ms: u64,
    },
    SigningKeyActivated {
        kid: SigningKeyId,
        retire_previous: bool,
        activated_at_ms: u64,
    },
    SigningKeyRetired {
        kid: SigningKeyId,
        retired_at_ms: u64,
    },
    SigningKeyRevoked {
        kid: SigningKeyId,
        revoked_at_ms: u64,
    },
}

impl DomainEvent for SigningKeySetEvent {
    fn event_type(&self) -> &'static str {
        match self {
            Self::SigningKeyProvisioned { .. } => "auth_signing_key_provisioned",
            Self::SigningKeyActivated { .. } => "auth_signing_key_activated",
            Self::SigningKeyRetired { .. } => "auth_signing_key_retired",
            Self::SigningKeyRevoked { .. } => "auth_signing_key_revoked",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SigningKeySet {
    pub tenant_id: Option<TenantId>,
    pub keys: Vec<SigningKeyState>,
    revision: u64,
}

impl Aggregate for SigningKeySet {
    type Id = TenantId;
    type Command = SigningKeySetCommand;
    type Event = SigningKeySetEvent;
    type Error = AuthError;

    fn aggregate_type() -> &'static str {
        "auth_signing_key_set"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            SigningKeySetEvent::SigningKeyProvisioned {
                tenant_id,
                kid,
                algorithm,
                public_jwk_json,
                private_key_ref,
                created_at_ms,
            } => {
                self.tenant_id = Some(tenant_id.clone());
                if let Some(key) = self.key_mut(kid) {
                    key.algorithm = algorithm.clone();
                    key.public_jwk_json = public_jwk_json.clone();
                    key.private_key_ref = private_key_ref.clone();
                    key.status = SigningKeyStatus::Next;
                    key.created_at_ms = *created_at_ms;
                    key.activated_at_ms = None;
                    key.retired_at_ms = None;
                    key.revoked_at_ms = None;
                } else {
                    self.keys.push(SigningKeyState {
                        kid: kid.clone(),
                        algorithm: algorithm.clone(),
                        status: SigningKeyStatus::Next,
                        public_jwk_json: public_jwk_json.clone(),
                        private_key_ref: private_key_ref.clone(),
                        created_at_ms: *created_at_ms,
                        activated_at_ms: None,
                        retired_at_ms: None,
                        revoked_at_ms: None,
                    });
                }
            }
            SigningKeySetEvent::SigningKeyActivated {
                kid,
                retire_previous,
                activated_at_ms,
            } => {
                if *retire_previous {
                    for key in &mut self.keys {
                        if key.status == SigningKeyStatus::Active && key.kid != *kid {
                            key.status = SigningKeyStatus::Retired;
                            key.retired_at_ms = Some(*activated_at_ms);
                        }
                    }
                }
                if let Some(key) = self.key_mut(kid) {
                    key.status = SigningKeyStatus::Active;
                    key.activated_at_ms = Some(*activated_at_ms);
                    key.retired_at_ms = None;
                    key.revoked_at_ms = None;
                }
            }
            SigningKeySetEvent::SigningKeyRetired { kid, retired_at_ms } => {
                if let Some(key) = self.key_mut(kid) {
                    key.status = SigningKeyStatus::Retired;
                    key.retired_at_ms = Some(*retired_at_ms);
                }
            }
            SigningKeySetEvent::SigningKeyRevoked { kid, revoked_at_ms } => {
                if let Some(key) = self.key_mut(kid) {
                    key.status = SigningKeyStatus::Revoked;
                    key.revoked_at_ms = Some(*revoked_at_ms);
                }
            }
        }
        self.revision += 1;
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            SigningKeySetCommand::ProvisionKey {
                tenant_id,
                kid,
                algorithm,
                public_jwk_json,
                private_key_ref,
                created_at_ms,
            } => {
                validate_non_empty("kid", kid.as_str())?;
                validate_non_empty("algorithm", &algorithm)?;
                Ok(vec![SigningKeySetEvent::SigningKeyProvisioned {
                    tenant_id,
                    kid,
                    algorithm,
                    public_jwk_json,
                    private_key_ref,
                    created_at_ms,
                }])
            }
            SigningKeySetCommand::ActivateKey {
                kid,
                retire_previous,
                activated_at_ms,
            } => {
                let Some(key) = self.key(&kid) else {
                    return Err(AuthError::validation("signing key is not provisioned"));
                };
                if key.status == SigningKeyStatus::Revoked {
                    return Err(AuthError::validation(
                        "revoked signing key cannot be activated",
                    ));
                }
                if key.status == SigningKeyStatus::Active {
                    return Ok(Vec::new());
                }
                Ok(vec![SigningKeySetEvent::SigningKeyActivated {
                    kid,
                    retire_previous,
                    activated_at_ms,
                }])
            }
            SigningKeySetCommand::RetireKey { kid, retired_at_ms } => {
                let Some(key) = self.key(&kid) else {
                    return Err(AuthError::validation("signing key is not provisioned"));
                };
                if key.status == SigningKeyStatus::Retired {
                    return Ok(Vec::new());
                }
                Ok(vec![SigningKeySetEvent::SigningKeyRetired {
                    kid,
                    retired_at_ms,
                }])
            }
            SigningKeySetCommand::RevokeKey { kid, revoked_at_ms } => {
                let Some(key) = self.key(&kid) else {
                    return Err(AuthError::validation("signing key is not provisioned"));
                };
                if key.status == SigningKeyStatus::Revoked {
                    return Ok(Vec::new());
                }
                Ok(vec![SigningKeySetEvent::SigningKeyRevoked {
                    kid,
                    revoked_at_ms,
                }])
            }
        }
    }

    fn new() -> Self {
        Self::default()
    }
}

impl SigningKeySet {
    fn key(&self, kid: &SigningKeyId) -> Option<&SigningKeyState> {
        self.keys.iter().find(|key| key.kid == *kid)
    }

    fn key_mut(&mut self, kid: &SigningKeyId) -> Option<&mut SigningKeyState> {
        self.keys.iter_mut().find(|key| key.kid == *kid)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthProviderConfigCommand {
    Configure {
        config: AuthProviderConfig,
        configured_at_ms: u64,
    },
    Enable {
        enabled_at_ms: u64,
    },
    Disable {
        disabled_at_ms: u64,
    },
    AddRedirectUri {
        redirect_uri: String,
        updated_at_ms: u64,
    },
    RemoveRedirectUri {
        redirect_uri: String,
        updated_at_ms: u64,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthProviderConfigEvent {
    AuthProviderConfigured {
        config: AuthProviderConfig,
        configured_at_ms: u64,
    },
    AuthProviderEnabled {
        enabled_at_ms: u64,
    },
    AuthProviderDisabled {
        disabled_at_ms: u64,
    },
    AuthProviderRedirectUriAdded {
        redirect_uri: String,
        updated_at_ms: u64,
    },
    AuthProviderRedirectUriRemoved {
        redirect_uri: String,
        updated_at_ms: u64,
    },
}

impl DomainEvent for AuthProviderConfigEvent {
    fn event_type(&self) -> &'static str {
        match self {
            Self::AuthProviderConfigured { .. } => "auth_provider_configured",
            Self::AuthProviderEnabled { .. } => "auth_provider_enabled",
            Self::AuthProviderDisabled { .. } => "auth_provider_disabled",
            Self::AuthProviderRedirectUriAdded { .. } => "auth_provider_redirect_uri_added",
            Self::AuthProviderRedirectUriRemoved { .. } => "auth_provider_redirect_uri_removed",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthProviderConfigAggregate {
    pub provider_id: Option<AuthProviderId>,
    pub config: Option<AuthProviderConfig>,
    pub enabled: bool,
    revision: u64,
}

impl Aggregate for AuthProviderConfigAggregate {
    type Id = AuthProviderId;
    type Command = AuthProviderConfigCommand;
    type Event = AuthProviderConfigEvent;
    type Error = AuthError;

    fn aggregate_type() -> &'static str {
        "auth_provider_config"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            AuthProviderConfigEvent::AuthProviderConfigured { config, .. } => {
                self.provider_id = Some(config.provider_id.clone());
                self.config = Some(config.clone());
            }
            AuthProviderConfigEvent::AuthProviderEnabled { .. } => {
                self.enabled = true;
            }
            AuthProviderConfigEvent::AuthProviderDisabled { .. } => {
                self.enabled = false;
            }
            AuthProviderConfigEvent::AuthProviderRedirectUriAdded { redirect_uri, .. } => {
                if let Some(config) = &mut self.config {
                    if !config.redirect_uri_allowlist.contains(redirect_uri) {
                        config.redirect_uri_allowlist.push(redirect_uri.clone());
                    }
                }
            }
            AuthProviderConfigEvent::AuthProviderRedirectUriRemoved { redirect_uri, .. } => {
                if let Some(config) = &mut self.config {
                    config
                        .redirect_uri_allowlist
                        .retain(|candidate| candidate != redirect_uri);
                }
            }
        }
        self.revision += 1;
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            AuthProviderConfigCommand::Configure {
                config,
                configured_at_ms,
            } => {
                validate_provider_config(&config)?;
                Ok(vec![AuthProviderConfigEvent::AuthProviderConfigured {
                    config,
                    configured_at_ms,
                }])
            }
            AuthProviderConfigCommand::Enable { enabled_at_ms } => {
                self.ensure_configured()?;
                if self.enabled {
                    return Ok(Vec::new());
                }
                Ok(vec![AuthProviderConfigEvent::AuthProviderEnabled {
                    enabled_at_ms,
                }])
            }
            AuthProviderConfigCommand::Disable { disabled_at_ms } => {
                self.ensure_configured()?;
                if !self.enabled {
                    return Ok(Vec::new());
                }
                Ok(vec![AuthProviderConfigEvent::AuthProviderDisabled {
                    disabled_at_ms,
                }])
            }
            AuthProviderConfigCommand::AddRedirectUri {
                redirect_uri,
                updated_at_ms,
            } => {
                self.ensure_configured()?;
                validate_non_empty("redirect_uri", &redirect_uri)?;
                if self
                    .config
                    .as_ref()
                    .map(|config| config.redirect_uri_allowlist.contains(&redirect_uri))
                    .unwrap_or(false)
                {
                    return Ok(Vec::new());
                }
                Ok(vec![
                    AuthProviderConfigEvent::AuthProviderRedirectUriAdded {
                        redirect_uri,
                        updated_at_ms,
                    },
                ])
            }
            AuthProviderConfigCommand::RemoveRedirectUri {
                redirect_uri,
                updated_at_ms,
            } => {
                self.ensure_configured()?;
                if !self
                    .config
                    .as_ref()
                    .map(|config| config.redirect_uri_allowlist.contains(&redirect_uri))
                    .unwrap_or(false)
                {
                    return Ok(Vec::new());
                }
                Ok(vec![
                    AuthProviderConfigEvent::AuthProviderRedirectUriRemoved {
                        redirect_uri,
                        updated_at_ms,
                    },
                ])
            }
        }
    }

    fn new() -> Self {
        Self::default()
    }
}

impl AuthProviderConfigAggregate {
    fn ensure_configured(&self) -> Result<(), AuthError> {
        if self.config.is_some() {
            Ok(())
        } else {
            Err(AuthError::InvalidProvider)
        }
    }
}

fn validate_provider_config(config: &AuthProviderConfig) -> Result<(), AuthError> {
    validate_non_empty("provider_id", config.provider_id.as_str())?;
    validate_non_empty("issuer", &config.issuer)?;
    validate_non_empty("authorization_endpoint", &config.authorization_endpoint)?;
    validate_non_empty("token_endpoint", &config.token_endpoint)?;
    validate_non_empty("client_id_env", &config.client_id_env)?;
    validate_non_empty("client_secret_ref", &config.client_secret_ref)?;
    if config.scopes.is_empty() {
        return Err(AuthError::validation("provider scopes must not be empty"));
    }
    Ok(())
}

fn validate_non_empty(label: &str, value: &str) -> Result<(), AuthError> {
    if value.trim().is_empty() {
        Err(AuthError::validation(format!("{label} must not be empty")))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_user_returns_registered_event() {
        let user = User::new();
        let events = user
            .handle(UserCommand::RegisterUser {
                user_id: UserId::new("user_1"),
                tenant_id: TenantId::new("tenant_1"),
                primary_email: "alice@example.com".to_string(),
            })
            .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type(), "auth_user_registered");
    }

    #[test]
    fn disabled_user_cannot_change_primary_email() {
        let loaded = User::replay_raw_events_from_zero(&[
            UserEvent::UserRegistered {
                user_id: UserId::new("user_1"),
                tenant_id: TenantId::new("tenant_1"),
                primary_email: "alice@example.com".to_string(),
            },
            UserEvent::UserDisabled,
        ]);

        let error = loaded
            .state
            .handle(UserCommand::ChangePrimaryEmail {
                primary_email: "alice2@example.com".to_string(),
            })
            .unwrap_err();

        assert_eq!(error, AuthError::UserDisabled);
    }

    #[test]
    fn revoked_password_credential_cannot_be_authenticated() {
        let loaded = PasswordCredential::replay_raw_events_from_zero(&[
            PasswordCredentialEvent::PasswordHashSet {
                user_id: UserId::new("user_1"),
                tenant_id: TenantId::new("tenant_1"),
                password_hash: "hash".to_string(),
                hash_algorithm: "pbkdf2-sha256".to_string(),
                changed_at_ms: 1,
            },
            PasswordCredentialEvent::PasswordCredentialRevoked { revoked_at_ms: 2 },
        ]);

        let error = loaded
            .state
            .handle(PasswordCredentialCommand::MarkAuthenticated {
                authenticated_at_ms: 3,
            })
            .unwrap_err();

        assert_eq!(error, AuthError::SessionRevoked);
    }

    #[test]
    fn external_identity_updates_profile_until_unlinked() {
        let linked = ExternalIdentity::replay_raw_events_from_zero(&[
            ExternalIdentityEvent::ExternalIdentityLinked {
                provider_id: AuthProviderId::new("google"),
                provider_subject: ExternalSubjectId::new("google-subject"),
                user_id: UserId::new("user_1"),
                tenant_id: TenantId::new("tenant_1"),
                primary_email: Some("alice@example.com".to_string()),
                profile_json: None,
                linked_at_ms: 1,
            },
        ]);

        let events = linked
            .state
            .handle(ExternalIdentityCommand::UpdateProfile {
                primary_email: Some("alice@work.example".to_string()),
                profile_json: Some("{\"name\":\"Alice\"}".to_string()),
                updated_at_ms: 2,
            })
            .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event_type(),
            "auth_external_identity_profile_updated"
        );

        let unlinked = ExternalIdentity::replay_raw_events_from_zero(&[
            ExternalIdentityEvent::ExternalIdentityLinked {
                provider_id: AuthProviderId::new("google"),
                provider_subject: ExternalSubjectId::new("google-subject"),
                user_id: UserId::new("user_1"),
                tenant_id: TenantId::new("tenant_1"),
                primary_email: Some("alice@example.com".to_string()),
                profile_json: None,
                linked_at_ms: 1,
            },
            ExternalIdentityEvent::ExternalIdentityUnlinked { unlinked_at_ms: 3 },
        ]);

        let error = unlinked
            .state
            .handle(ExternalIdentityCommand::UpdateProfile {
                primary_email: Some("alice@work.example".to_string()),
                profile_json: None,
                updated_at_ms: 4,
            })
            .unwrap_err();

        assert_eq!(error, AuthError::UserNotRegistered);
    }

    #[test]
    fn passkey_sign_count_cannot_move_backwards() {
        let loaded = PasskeyCredential::replay_raw_events_from_zero(&[
            PasskeyCredentialEvent::PasskeyCredentialRegistered {
                credential_id: PasskeyCredentialId::new("credential_1"),
                user_id: UserId::new("user_1"),
                tenant_id: TenantId::new("tenant_1"),
                public_key_json: "{\"kty\":\"OKP\"}".to_string(),
                transports: vec!["internal".to_string()],
                sign_count: 10,
                registered_at_ms: 1,
            },
        ]);

        let error = loaded
            .state
            .handle(PasskeyCredentialCommand::UpdateSignCount {
                sign_count: 9,
                authenticated_at_ms: 2,
            })
            .unwrap_err();

        assert_eq!(error.public_code(), "validation");
        assert!(error.public_message().contains("must not move backwards"));
    }

    #[test]
    fn revoked_session_rejects_refresh_rotation() {
        let loaded = Session::replay_raw_events_from_zero(&[
            SessionEvent::SessionIssued {
                session_id: SessionId::new("session_1"),
                user_id: UserId::new("user_1"),
                tenant_id: TenantId::new("tenant_1"),
                provider_id: None,
                expires_at_ms: 10_000,
                issued_at_ms: 1,
            },
            SessionEvent::SessionRevoked { revoked_at_ms: 2 },
        ]);

        let error = loaded
            .state
            .handle(SessionCommand::RotateRefreshToken {
                refresh_token_hash: "refresh_hash".to_string(),
                refresh_token_expires_at_ms: 20_000,
                rotated_at_ms: 3,
            })
            .unwrap_err();

        assert_eq!(error, AuthError::SessionRevoked);
    }

    #[test]
    fn signing_key_activation_retires_prior_active_key() {
        let mut signing_keys = SigningKeySet::new();

        for event in signing_keys
            .handle(SigningKeySetCommand::ProvisionKey {
                tenant_id: TenantId::new("tenant_1"),
                kid: SigningKeyId::new("kid_1"),
                algorithm: "RS256".to_string(),
                public_jwk_json: Some("{\"kid\":\"kid_1\"}".to_string()),
                private_key_ref: Some("secret://kid_1".to_string()),
                created_at_ms: 1,
            })
            .unwrap()
        {
            signing_keys.apply(&event);
        }
        for event in signing_keys
            .handle(SigningKeySetCommand::ActivateKey {
                kid: SigningKeyId::new("kid_1"),
                retire_previous: true,
                activated_at_ms: 2,
            })
            .unwrap()
        {
            signing_keys.apply(&event);
        }
        for event in signing_keys
            .handle(SigningKeySetCommand::ProvisionKey {
                tenant_id: TenantId::new("tenant_1"),
                kid: SigningKeyId::new("kid_2"),
                algorithm: "RS256".to_string(),
                public_jwk_json: Some("{\"kid\":\"kid_2\"}".to_string()),
                private_key_ref: Some("secret://kid_2".to_string()),
                created_at_ms: 3,
            })
            .unwrap()
        {
            signing_keys.apply(&event);
        }
        for event in signing_keys
            .handle(SigningKeySetCommand::ActivateKey {
                kid: SigningKeyId::new("kid_2"),
                retire_previous: true,
                activated_at_ms: 4,
            })
            .unwrap()
        {
            signing_keys.apply(&event);
        }

        assert_eq!(signing_keys.tenant_id, Some(TenantId::new("tenant_1")));
        assert_eq!(
            signing_keys
                .key(&SigningKeyId::new("kid_1"))
                .unwrap()
                .status,
            SigningKeyStatus::Retired
        );
        assert_eq!(
            signing_keys
                .key(&SigningKeyId::new("kid_2"))
                .unwrap()
                .status,
            SigningKeyStatus::Active
        );
    }

    #[test]
    fn revoked_signing_key_cannot_be_activated() {
        let loaded = SigningKeySet::replay_raw_events_from_zero(&[
            SigningKeySetEvent::SigningKeyProvisioned {
                tenant_id: TenantId::new("tenant_1"),
                kid: SigningKeyId::new("kid_1"),
                algorithm: "RS256".to_string(),
                public_jwk_json: Some("{\"kid\":\"kid_1\"}".to_string()),
                private_key_ref: Some("secret://kid_1".to_string()),
                created_at_ms: 1,
            },
            SigningKeySetEvent::SigningKeyRevoked {
                kid: SigningKeyId::new("kid_1"),
                revoked_at_ms: 2,
            },
        ]);

        let error = loaded
            .state
            .handle(SigningKeySetCommand::ActivateKey {
                kid: SigningKeyId::new("kid_1"),
                retire_previous: true,
                activated_at_ms: 3,
            })
            .unwrap_err();

        assert_eq!(error.public_code(), "validation");
        assert!(error.public_message().contains("cannot be activated"));
    }

    #[test]
    fn provider_config_validates_required_fields_and_redirects_are_idempotent() {
        let mut config = provider_config();
        config.token_endpoint.clear();

        let error = AuthProviderConfigAggregate::new()
            .handle(AuthProviderConfigCommand::Configure {
                config,
                configured_at_ms: 1,
            })
            .unwrap_err();

        assert_eq!(error.public_code(), "validation");

        let mut aggregate = AuthProviderConfigAggregate::new();
        for event in aggregate
            .handle(AuthProviderConfigCommand::Configure {
                config: provider_config(),
                configured_at_ms: 1,
            })
            .unwrap()
        {
            aggregate.apply(&event);
        }

        assert!(aggregate
            .handle(AuthProviderConfigCommand::AddRedirectUri {
                redirect_uri: "http://localhost:3008/auth/callback/google".to_string(),
                updated_at_ms: 2,
            })
            .unwrap()
            .is_empty());

        let added = aggregate
            .handle(AuthProviderConfigCommand::AddRedirectUri {
                redirect_uri: "http://localhost:3009/auth/callback/google".to_string(),
                updated_at_ms: 3,
            })
            .unwrap();
        assert_eq!(added.len(), 1);
        aggregate.apply(&added[0]);

        let removed = aggregate
            .handle(AuthProviderConfigCommand::RemoveRedirectUri {
                redirect_uri: "http://localhost:3009/auth/callback/google".to_string(),
                updated_at_ms: 4,
            })
            .unwrap();
        assert_eq!(removed.len(), 1);
        aggregate.apply(&removed[0]);

        assert!(aggregate
            .handle(AuthProviderConfigCommand::RemoveRedirectUri {
                redirect_uri: "http://localhost:3009/auth/callback/google".to_string(),
                updated_at_ms: 5,
            })
            .unwrap()
            .is_empty());
    }

    fn provider_config() -> AuthProviderConfig {
        AuthProviderConfig {
            provider_id: AuthProviderId::new("google"),
            profile: crate::OAuthProviderProfile::Google,
            issuer: "https://accounts.google.com".to_string(),
            authorization_endpoint: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_endpoint: "https://oauth2.googleapis.com/token".to_string(),
            jwks_uri: Some("https://www.googleapis.com/oauth2/v3/certs".to_string()),
            userinfo_endpoint: Some("https://openidconnect.googleapis.com/v1/userinfo".to_string()),
            client_id_env: "AUTH_GOOGLE_CLIENT_ID".to_string(),
            client_secret_ref: "AUTH_GOOGLE_CLIENT_SECRET".to_string(),
            scopes: vec!["openid".to_string(), "email".to_string()],
            redirect_uri_allowlist: vec!["http://localhost:3008/auth/callback/google".to_string()],
        }
    }
}
