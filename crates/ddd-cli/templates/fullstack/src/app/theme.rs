//! App color theme: light / dark / system (persisted in localStorage).

#![allow(unused_imports)]

use crate::ui::classes::{
    THEME_TOGGLE, THEME_TOGGLE_HINT, THEME_TOGGLE_ICON, THEME_TOGGLE_LABEL, THEME_TOGGLE_META,
};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

pub(crate) const THEME_STORAGE_KEY: &str = "app-theme";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppTheme {
    Light,
    Dark,
    System,
}

impl AppTheme {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "System",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
            Self::System => Self::Light,
        }
    }

    pub(crate) fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}

/// Bottom-most sidebar control: cycle light → dark → system.
#[island]
pub fn ThemeToggle() -> impl IntoView {
    let (theme, set_theme) = signal(AppTheme::System);

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            let initial = read_stored_theme().unwrap_or(AppTheme::System);
            set_theme.set(initial);
            apply_theme(initial);
        }
    });

    let cycle = move |_| {
        let next = theme.get_untracked().next();
        set_theme.set(next);
        #[cfg(feature = "hydrate")]
        {
            store_theme(next);
            apply_theme(next);
        }
    };

    view! {
        <button
            type="button"
            class=THEME_TOGGLE
            data-testid="theme-toggle"
            aria-label=move || format!("Theme: {}. Click to change.", theme.get().label())
            title=move || format!("Theme: {} (click to cycle)", theme.get().label())
            on:click=cycle
        >
            <span class=THEME_TOGGLE_ICON aria-hidden="true">
                {move || theme_glyph(theme.get())}
            </span>
            <span class=THEME_TOGGLE_META>
                <span class=THEME_TOGGLE_LABEL>"Theme"</span>
                <span class=THEME_TOGGLE_HINT>{move || theme.get().label()}</span>
            </span>
        </button>
    }
}

fn theme_glyph(theme: AppTheme) -> &'static str {
    match theme {
        AppTheme::Light => "☀",
        AppTheme::Dark => "☾",
        AppTheme::System => "◐",
    }
}

#[cfg(feature = "hydrate")]
fn read_stored_theme() -> Option<AppTheme> {
    let window = window()?;
    let storage = window.local_storage().ok().flatten()?;
    let raw = storage.get_item(THEME_STORAGE_KEY).ok().flatten()?;
    Some(AppTheme::parse(&raw))
}

#[cfg(feature = "hydrate")]
fn store_theme(theme: AppTheme) {
    let Some(window) = window() else {
        return;
    };
    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item(THEME_STORAGE_KEY, theme.as_str());
    }
}

#[cfg(feature = "hydrate")]
fn apply_theme(theme: AppTheme) {
    let Some(window) = window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(root) = document.document_element() else {
        return;
    };
    let _ = root.set_attribute("data-theme", theme.as_str());
}
