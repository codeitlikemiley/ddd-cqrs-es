#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::helpers::{action_result_text, optional_text, server_error_text};
use crate::app::{
    GetAccountProfile, GetPublicProfile, UpdateAccountProfile, browser_load, get_account_profile,
    get_public_profile, update_account_profile,
};
use crate::contracts::{ProfileUpdateRequest, ProfileView, PublicProfileView};
use crate::ui::{ErrorBanner, PrimaryButton, account_page_shell, public_page_shell};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use server_fn::ServerFnError;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

#[cfg(feature = "hydrate")]
use crate::app::pick_image_data_url;
use crate::ui::classes::{
    AUTH_TEXT_LINK, BANNER_ERROR, BANNER_SUCCESS, BTN_AUTH_SUBMIT, BTN_PRIMARY, BTN_SECONDARY,
    BUTTON_ROW, FIELD, FIELD_GROUP, INPUT, PANEL, PANEL_COMPACT, RESULT_LINE, SECTION_LABEL,
};

#[component]
pub fn AccountProfilePage() -> impl IntoView {
    account_page_shell(
        "Profile",
        "Your name, @handle, avatar, and whether others can find you.",
        "profile",
        view! { <AccountProfileCard /> },
    )
}

#[component]
pub fn PublicProfilePage() -> impl IntoView {
    let params = use_params_map();
    public_page_shell(
        "Profile",
        "Public account",
        view! {
            {move || {
                let handle = params
                    .get()
                    .get("handle")
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                view! { <PublicProfileCard handle=handle /> }.into_any()
            }}
        },
    )
}

#[island]
pub fn AccountProfileCard() -> impl IntoView {
    let profile = browser_load(get_account_profile);
    let action = ServerAction::<UpdateAccountProfile>::new();
    let pending = action.pending();
    let value = action.value();

    let (first_name, set_first_name) = signal(String::new());
    let (last_name, set_last_name) = signal(String::new());
    let (display_name, set_display_name) = signal(String::new());
    let (username, set_username) = signal(String::new());
    let (is_public, set_is_public) = signal(false);
    let (avatar_data_url, set_avatar_data_url) = signal(None::<String>);
    let (avatar_dirty, set_avatar_dirty) = signal(false);
    let (client_error, set_client_error) = signal(None::<String>);
    let (seeded, set_seeded) = signal(false);

    Effect::new(move |_| {
        if seeded.get() {
            return;
        }
        // Prefer a successful save result so the form stays in sync after update.
        if let Some(Ok(saved)) = value.get() {
            seed_profile_form(
                &saved,
                set_first_name,
                set_last_name,
                set_display_name,
                set_username,
                set_is_public,
                set_avatar_data_url,
            );
            set_avatar_dirty.set(false);
            set_seeded.set(true);
            return;
        }
        if let Some(Ok(loaded)) = profile.get() {
            seed_profile_form(
                &loaded,
                set_first_name,
                set_last_name,
                set_display_name,
                set_username,
                set_is_public,
                set_avatar_data_url,
            );
            set_avatar_dirty.set(false);
            set_seeded.set(true);
        }
    });

    // After a successful save, re-seed from the response.
    Effect::new(move |_| {
        if let Some(Ok(saved)) = value.get() {
            seed_profile_form(
                &saved,
                set_first_name,
                set_last_name,
                set_display_name,
                set_username,
                set_is_public,
                set_avatar_data_url,
            );
            set_avatar_dirty.set(false);
            set_client_error.set(None);
        }
    });

    let preview_initials = move || {
        profile_initials(
            &display_name.get(),
            &first_name.get(),
            &last_name.get(),
            profile
                .get()
                .and_then(Result::ok)
                .and_then(|p| p.email)
                .as_deref()
                .unwrap_or(""),
        )
    };

    let on_avatar_file = move |event| {
        #[cfg(feature = "hydrate")]
        {
            use wasm_bindgen::JsCast;
            let input: web_sys::HtmlInputElement = event_target(&event);
            spawn_local(async move {
                match pick_image_data_url(input, 250_000).await {
                    Ok(value) if value.is_null() || value.is_undefined() => {}
                    Ok(value) => {
                        if let Some(data_url) = value.as_string() {
                            set_avatar_data_url.set(Some(data_url));
                            set_avatar_dirty.set(true);
                            set_client_error.set(None);
                        }
                    }
                    Err(error) => {
                        let message = error
                            .as_string()
                            .unwrap_or_else(|| "Could not read image.".to_owned());
                        set_client_error.set(Some(message));
                    }
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = event;
        }
    };

    view! {
        <section class=format!("{}{}", PANEL, " profile-editor")>
            {move || match profile.get() {
                Some(Err(error)) => view! {
                    <p class=BANNER_ERROR>{server_error_text(error)}</p>
                }.into_any(),
                None if !seeded.get() => view! {
                    <div class="profile-loading" aria-busy="true">
                        <div class="profile-skeleton-avatar"></div>
                        <div class="profile-skeleton-lines">
                            <span></span><span></span><span></span>
                        </div>
                    </div>
                }.into_any(),
                _ => view! {
                    <div class="profile-editor-body">
                        // Centered identity: avatar + one primary line + optional handle
                        <header class="profile-identity-strip">
                            <div
                                class="profile-avatar-wrap"
                                class:has-photo=move || {
                                    avatar_data_url
                                        .get()
                                        .as_ref()
                                        .is_some_and(|url| !url.is_empty())
                                }
                            >
                                <Show when=move || {
                                    avatar_data_url
                                        .get()
                                        .as_ref()
                                        .is_some_and(|url| !url.is_empty())
                                }>
                                    <button
                                        type="button"
                                        class="profile-avatar-clear"
                                        aria-label="Remove photo"
                                        title="Remove photo"
                                        on:click=move |ev| {
                                            ev.prevent_default();
                                            ev.stop_propagation();
                                            set_avatar_data_url.set(None);
                                            set_avatar_dirty.set(true);
                                            set_client_error.set(None);
                                        }
                                    >
                                        <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                                            <path
                                                fill="currentColor"
                                                d="M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.75.75 0 1 1 1.06 1.06L9.06 8l3.22 3.22a.75.75 0 1 1-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 0 1-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z"
                                            />
                                        </svg>
                                    </button>
                                </Show>
                                <label class="profile-avatar-control" title="Change photo">
                                    <input
                                        type="file"
                                        accept="image/png,image/jpeg,image/webp,image/gif"
                                        class="profile-file-input"
                                        aria-label="Upload profile photo"
                                        on:change=on_avatar_file
                                    />
                                    <span class="profile-avatar-disk" aria-hidden="true">
                                        {move || match avatar_data_url.get() {
                                            Some(url) if !url.is_empty() => view! {
                                                <img class="profile-avatar-img" src=url alt="" />
                                            }.into_any(),
                                            _ => view! {
                                                <span class="profile-avatar-fallback">{preview_initials()}</span>
                                            }.into_any(),
                                        }}
                                        <span class="profile-avatar-veil">
                                            <svg class="profile-avatar-camera" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
                                                <path
                                                    fill="currentColor"
                                                    d="M9 3.75A1.75 1.75 0 0 1 10.53 2.5h2.94A1.75 1.75 0 0 1 15 3.75V5h2.25A2.75 2.75 0 0 1 20 7.75v9.5A2.75 2.75 0 0 1 17.25 20H6.75A2.75 2.75 0 0 1 4 17.25v-9.5A2.75 2.75 0 0 1 6.75 5H9V3.75Zm1.5 1.5V5h3V5.25h-3ZM12 9a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm0 1.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z"
                                                />
                                            </svg>
                                        </span>
                                    </span>
                                </label>
                            </div>
                            <div class="profile-identity-copy">
                                <h2 class="profile-display-preview">
                                    {move || {
                                        let display = display_name.get();
                                        let first = first_name.get();
                                        let last = last_name.get();
                                        let composed = format!("{first} {last}").trim().to_owned();
                                        let email = profile
                                            .get()
                                            .and_then(Result::ok)
                                            .and_then(|p| p.email)
                                            .unwrap_or_default();
                                        if !display.trim().is_empty() {
                                            display
                                        } else if !composed.is_empty() {
                                            composed
                                        } else if !email.is_empty() {
                                            email
                                        } else {
                                            "Your name".to_owned()
                                        }
                                    }}
                                </h2>
                                // Handle only when set — never show a placeholder @handle.
                                <Show when=move || !username.get().trim().is_empty()>
                                    <p class="profile-handle-preview">
                                        {move || format!("@{}", username.get().trim().to_ascii_lowercase())}
                                    </p>
                                </Show>
                                // Email only when primary title is a name (avoid duplicate email lines).
                                <Show when=move || {
                                    let display = display_name.get();
                                    let first = first_name.get();
                                    let last = last_name.get();
                                    let composed = format!("{first} {last}").trim().to_owned();
                                    let has_name = !display.trim().is_empty() || !composed.is_empty();
                                    has_name && profile.get().and_then(Result::ok).and_then(|p| p.email).is_some()
                                }>
                                    <p class="profile-email-line">
                                        {move || profile
                                            .get()
                                            .and_then(Result::ok)
                                            .and_then(|p| p.email)
                                            .unwrap_or_default()}
                                    </p>
                                </Show>
                            </div>
                        </header>

                        <div class="profile-sections">
                            <section class="profile-section">
                                <div class="profile-section-head">
                                    <h3>"Name"</h3>
                                    <p>"Legal name stays private unless you publish your profile."</p>
                                </div>
                                <div class=format!("{}{}", FIELD, "s profile-form-grid")>
                                    <label class=FIELD>
                                        <span>"First name"</span>
                                        <input
                                            class=INPUT
                                            type="text"
                                            autocomplete="given-name"
                                            maxlength="60"
                                            prop:value=move || first_name.get()
                                            on:input=move |event| {
                                                set_client_error.set(None);
                                                set_first_name.set(event_target_value(&event));
                                            }
                                        />
                                    </label>
                                    <label class=FIELD>
                                        <span>"Last name"</span>
                                        <input
                                            class=INPUT
                                            type="text"
                                            autocomplete="family-name"
                                            maxlength="60"
                                            prop:value=move || last_name.get()
                                            on:input=move |event| {
                                                set_client_error.set(None);
                                                set_last_name.set(event_target_value(&event));
                                            }
                                        />
                                    </label>
                                    <label class=format!("{}{}", FIELD, " profile-field-span")>
                                        <span>"Display name"</span>
                                        <input
                                            class=INPUT
                                            type="text"
                                            autocomplete="nickname"
                                            maxlength="80"
                                            prop:value=move || display_name.get()
                                            on:input=move |event| {
                                                set_client_error.set(None);
                                                set_display_name.set(event_target_value(&event));
                                            }
                                        />
                                        <small>"Shown publicly. Falls back to first + last when empty."</small>
                                    </label>
                                </div>
                            </section>

                            <section class="profile-section">
                                <div class="profile-section-head">
                                    <h3>"Handle"</h3>
                                    <p>"Your unique @username. Required for a public profile link."</p>
                                </div>
                                <div class=FIELD_GROUP>
                                    <label class=FIELD>
                                        <span>"Username"</span>
                                        <div class="profile-username-field">
                                            <span class="profile-username-at" aria-hidden="true">"@"</span>
                                            <input
                                                class=format!("{}{}", INPUT, " profile-username-input")
                                                type="text"
                                                autocomplete="username"
                                                spellcheck="false"
                                                maxlength="30"
                                                prop:value=move || username.get()
                                                on:input=move |event| {
                                                    set_client_error.set(None);
                                                    let raw = event_target_value(&event);
                                                    let cleaned = raw
                                                        .chars()
                                                        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                                                        .collect::<String>()
                                                        .to_ascii_lowercase();
                                                    set_username.set(cleaned);
                                                }
                                            />
                                        </div>
                                        <small>"3–30 characters · letters, numbers, underscore"</small>
                                    </label>
                                </div>
                            </section>

                            <section class="profile-section profile-section-privacy">
                                <div class="profile-section-head">
                                    <h3>"Visibility"</h3>
                                    <p>"Profiles are private until you choose to publish."</p>
                                </div>
                                <label class="profile-switch">
                                    <input
                                        type="checkbox"
                                        role="switch"
                                        prop:checked=move || is_public.get()
                                        on:change=move |event| {
                                            set_client_error.set(None);
                                            set_is_public.set(event_target_checked(&event));
                                        }
                                    />
                                    <span class="profile-switch-track" aria-hidden="true">
                                        <span class="profile-switch-thumb"></span>
                                    </span>
                                    <span class="profile-switch-copy">
                                        <strong>"Public profile"</strong>
                                        <small>
                                            {move || if is_public.get() {
                                                "Anyone with your link can see your name, @handle, and photo."
                                            } else {
                                                "Only you can see this profile."
                                            }}
                                        </small>
                                    </span>
                                </label>
                                <Show when=move || {
                                    is_public.get() && !username.get().trim().is_empty()
                                }>
                                    <p class="profile-public-link">
                                        <span class="profile-public-link-label">"Live at"</span>
                                        <a
                                            class="profile-public-link-url"
                                            href=move || format!(
                                                "/u/{}",
                                                username.get().trim().to_ascii_lowercase()
                                            )
                                        >
                                            {move || format!(
                                                "/u/{}",
                                                username.get().trim().to_ascii_lowercase()
                                            )}
                                        </a>
                                    </p>
                                </Show>
                            </section>
                        </div>

                        <footer class="profile-footer">
                            <button
                                type="button"
                                class=BTN_PRIMARY
                                disabled=move || pending.get()
                                on:click=move |_| {
                                    set_client_error.set(None);
                                    let handle = username.get_untracked().trim().to_owned();
                                    if !handle.is_empty()
                                        && (handle.len() < 3 || handle.len() > 30)
                                    {
                                        set_client_error.set(Some(
                                            "Username must be 3–30 characters.".to_owned(),
                                        ));
                                        return;
                                    }
                                    action.dispatch(UpdateAccountProfile {
                                        first_name: first_name.get_untracked(),
                                        last_name: last_name.get_untracked(),
                                        display_name: display_name.get_untracked(),
                                        username: handle,
                                        is_public: is_public.get_untracked(),
                                        avatar_data_url: if avatar_dirty.get_untracked() {
                                            Some(
                                                avatar_data_url
                                                    .get_untracked()
                                                    .unwrap_or_default(),
                                            )
                                        } else {
                                            None
                                        },
                                    });
                                }
                            >
                                {move || if pending.get() { "Saving…" } else { "Save changes" }}
                            </button>
                            <p class=BANNER_ERROR hidden=move || client_error.get().is_none()>
                                {move || client_error.get().unwrap_or_default()}
                            </p>
                            <Show when=move || {
                                value.get().is_some_and(|result| result.is_err())
                            }>
                                <p class=BANNER_ERROR>
                                    {move || action_result_text(value.get())}
                                </p>
                            </Show>
                            <Show when=move || matches!(value.get(), Some(Ok(_)))>
                                <p class=format!("{} profile-save-ok", BANNER_SUCCESS)>
                                    <span>"Saved"</span>
                                </p>
                            </Show>
                        </footer>
                    </div>
                }.into_any(),
            }}
        </section>
    }
}

pub fn seed_profile_form(
    profile: &ProfileView,
    set_first_name: WriteSignal<String>,
    set_last_name: WriteSignal<String>,
    set_display_name: WriteSignal<String>,
    set_username: WriteSignal<String>,
    set_is_public: WriteSignal<bool>,
    set_avatar_data_url: WriteSignal<Option<String>>,
) {
    set_first_name.set(profile.first_name.clone());
    set_last_name.set(profile.last_name.clone());
    set_display_name.set(profile.display_name.clone());
    set_username.set(profile.username.clone());
    set_is_public.set(profile.is_public);
    set_avatar_data_url.set(profile.avatar_data_url.clone());
}

pub fn profile_initials(display_name: &str, first: &str, last: &str, email: &str) -> String {
    let display = display_name.trim();
    if !display.is_empty() {
        let parts: Vec<&str> = display.split_whitespace().collect();
        if parts.len() >= 2 {
            let a = parts[0].chars().next().unwrap_or('?');
            let b = parts[1].chars().next().unwrap_or('?');
            return format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_uppercase());
        }
        return display
            .chars()
            .take(2)
            .map(|c| c.to_ascii_uppercase())
            .collect();
    }
    let first = first.trim();
    let last = last.trim();
    match (first.chars().next(), last.chars().next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_uppercase()),
        (Some(a), None) => a.to_ascii_uppercase().to_string(),
        (None, Some(b)) => b.to_ascii_uppercase().to_string(),
        _ => account_initials(email),
    }
}

pub fn account_initials(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email).trim();
    let mut chars = local.chars().filter(|c| c.is_alphanumeric());
    match (chars.next(), chars.next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_uppercase()),
        (Some(a), None) => a.to_ascii_uppercase().to_string(),
        _ => "?".to_string(),
    }
}

#[island]
pub fn PublicProfileCard(handle: String) -> impl IntoView {
    let profile = {
        let handle = handle.clone();
        browser_load(move || get_public_profile(handle))
    };

    view! {
        <section class=format!("{}{}", PANEL, " public-profile-panel")>
            {move || match profile.get() {
                None => view! { <p class=RESULT_LINE>"Loading profile…"</p> }.into_any(),
                Some(Err(_)) => view! {
                    <div class="public-profile-empty">
                        <div class="profile-avatar-fallback public-profile-empty-avatar" aria-hidden="true">"?"</div>
                        <h2>"Profile unavailable"</h2>
                        <p class=RESULT_LINE>
                            "This @handle is private or does not exist."
                        </p>
                        <a class=BTN_SECONDARY href="/">"Back home"</a>
                    </div>
                }.into_any(),
                Some(Ok(view)) => {
                    let display = if !view.display_name.trim().is_empty() {
                        view.display_name.clone()
                    } else {
                        let composed = format!("{} {}", view.first_name, view.last_name)
                            .trim()
                            .to_owned();
                        if composed.is_empty() {
                            format!("@{}", view.username)
                        } else {
                            composed
                        }
                    };
                    let initials = profile_initials(
                        &view.display_name,
                        &view.first_name,
                        &view.last_name,
                        &view.username,
                    );
                    let handle_label = format!("@{}", view.username);
                    let avatar = view.avatar_data_url.clone();
                    let legal_name = {
                        let composed = format!("{} {}", view.first_name, view.last_name)
                            .trim()
                            .to_owned();
                        if composed.is_empty() || composed == display {
                            None
                        } else {
                            Some(composed)
                        }
                    };
                    view! {
                        <div class="public-profile-hero">
                            <div class="public-profile-avatar" aria-hidden="true">
                                {match avatar {
                                    Some(url) if !url.is_empty() => view! {
                                        <img class="profile-avatar-img" src=url alt="" />
                                    }.into_any(),
                                    _ => view! {
                                        <span class="profile-avatar-fallback">{initials}</span>
                                    }.into_any(),
                                }}
                            </div>
                            <div class="public-profile-meta">
                                <p class="profile-kicker">"Public profile"</p>
                                <h2>{display}</h2>
                                <p class="profile-handle-preview">{handle_label}</p>
                                {legal_name.map(|name| view! {
                                    <p class="profile-email-line">{name}</p>
                                })}
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </section>
    }
}
