//! Shared presentational UI primitives.
//!
//! These wrap existing semantic CSS class names so we can modularize the app
//! before a full Tailwind utility migration. Prefer these over ad-hoc class
//! strings when adding new UI.

#![allow(unused_imports)] // Public re-exports are consumed gradually as call sites migrate.

mod banner;
mod brand;
mod button;
mod combobox;
mod field;
mod kicker;
mod panel;
mod shells;

pub use banner::{ErrorBanner, ResultLine, SuccessBanner};
pub use brand::AuthBrand;
pub use button::{LinkButton, PrimaryButton, SecondaryButton};
pub use combobox::{ComboboxOption, FilterCombobox};
pub use field::{Field, FieldGroup, TextInput};
pub use kicker::SectionLabel;
pub use panel::{CompactPanel, Panel};
pub use shells::{account_page_shell, error_page_shell, page_shell, public_page_shell};
