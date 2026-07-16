//! Shared presentational UI primitives.
//!
//! Components emit Tailwind utility class groups from [`classes`]. Prefer these
//! over ad-hoc class strings when adding new UI. See `TAILWIND_MIGRATION.md`.

#![allow(unused_imports)] // Public re-exports are consumed gradually as call sites migrate.

mod banner;
mod brand;
mod button;
pub mod classes;
mod combobox;
mod field;
mod kicker;
mod panel;
mod shells;

pub use banner::{ErrorBanner, ResultLine, SuccessBanner};
pub use brand::AuthBrand;
pub use button::{LinkButton, PrimaryButton, SecondaryButton};
pub use classes::{
    AUTH_CARD, AUTH_PAGE, BTN_AUTH_SUBMIT, BTN_PRIMARY, BTN_SECONDARY, INPUT, PANEL, with_extra,
};
pub use combobox::{ComboboxOption, FilterCombobox};
pub use field::{Field, FieldGroup, TextInput};
pub use kicker::SectionLabel;
pub use panel::{CompactPanel, Panel};
pub use shells::{account_page_shell, error_page_shell, page_shell, public_page_shell};
