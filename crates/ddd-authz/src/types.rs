use crate::AuthzError;
use std::fmt::{Display, Formatter};

macro_rules! ref_type {
    ($name:ident) => {
        #[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, AuthzError> {
                let value = value.into();
                validate_ref(stringify!($name), &value)?;
                Ok(Self(value))
            }

            pub fn unchecked(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn type_name(&self) -> &str {
                self.0.split_once(':').map(|(ty, _)| ty).unwrap_or("")
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

ref_type!(SubjectRef);
ref_type!(ObjectRef);
ref_type!(TenantRef);

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Relation(String);

impl Relation {
    pub fn new(value: impl Into<String>) -> Result<Self, AuthzError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AuthzError::validation("relation must not be empty"));
        }
        Ok(Self(value))
    }

    pub fn unchecked(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Relation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

fn validate_ref(label: &str, value: &str) -> Result<(), AuthzError> {
    let Some((ty, id)) = value.split_once(':') else {
        return Err(AuthzError::validation(format!(
            "{label} must use type:id syntax"
        )));
    };
    if ty.trim().is_empty() || id.trim().is_empty() {
        return Err(AuthzError::validation(format!(
            "{label} must use non-empty type and id"
        )));
    }
    Ok(())
}
