#![allow(unused_imports)]
#![allow(dead_code)]

use wasi_auth::context::{
    AuthenticationAssurance, Principal, SessionId, UserId, VerifiedAuthContext,
    VerifiedRequestContext,
};
use wasi_auth::http::AuthenticatedSession;

#[derive(Clone, Debug, Default)]
pub struct RequestAuth {
    pub session_id: Option<String>,
    pub access_token: Option<String>,
    pub request_id: Option<String>,
    pub verified: Option<VerifiedRequestContext>,
}

impl RequestAuth {
    pub fn from_parts(
        session_id: Option<String>,
        access_token: Option<String>,
        request_id: Option<String>,
    ) -> Self {
        Self {
            session_id,
            access_token,
            request_id,
            verified: None,
        }
    }

    pub fn from_verified(context: VerifiedRequestContext) -> Self {
        Self {
            session_id: Some(context.auth().session_id().as_str().to_owned()),
            access_token: None,
            request_id: Some(context.auth().request_id().as_str().to_owned()),
            verified: Some(context),
        }
    }

    #[cfg_attr(not(all(feature = "spin-grpc", runtime_spin)), allow(dead_code))]
    pub fn for_revalidation(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            access_token: self.access_token.clone(),
            request_id: self.request_id.clone(),
            verified: None,
        }
    }
}
