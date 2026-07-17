#![allow(unused_imports)]
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Editable account profile (app-owned; not the wasi-auth principal).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileView {
    pub email: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub display_name: String,
    pub username: String,
    pub is_public: bool,
    /// Optional data-URL avatar (`data:image/...;base64,...`).
    pub avatar_data_url: Option<String>,
    /// Public profile path when a username is set, e.g. `/u/jane`.
    pub public_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileUpdateRequest {
    pub first_name: String,
    pub last_name: String,
    pub display_name: String,
    pub username: String,
    pub is_public: bool,
    /// When `Some`, replace avatar. Empty string clears. `None` leaves unchanged.
    #[serde(default)]
    pub avatar_data_url: Option<String>,
}

/// Public @handle profile (only returned when the owner marked the profile public).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicProfileView {
    pub username: String,
    pub display_name: String,
    pub first_name: String,
    pub last_name: String,
    pub avatar_data_url: Option<String>,
}
