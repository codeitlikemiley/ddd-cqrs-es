#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::helpers::action_result_text;
use crate::app::{ChangePassword, change_password};
use crate::ui::classes::{
    ACCOUNT_CARD_ACTIONS, ACCOUNT_LEDE_FLUSH, ACCOUNT_PANEL, ACCOUNT_PANEL_HEAD,
    ACCOUNT_PANEL_TITLE, AUTH_TEXT_LINK, RESULT_LINE,
};
use crate::ui::{
    ErrorBanner, Field, FieldGroup, PrimaryButton, SectionLabel, SuccessBanner, TextInput,
    account_page_shell,
};
use leptos::prelude::*;
use server_fn::ServerFnError;

#[component]
pub fn AccountPasswordPage() -> impl IntoView {
    account_page_shell(
        "Password",
        "Update the password for this account. Enter your current password to confirm.",
        "password",
        view! { <ChangePasswordForm /> },
    )
}

#[island]
pub fn ChangePasswordForm() -> impl IntoView {
    let action = ServerAction::<ChangePassword>::new();
    let pending = action.pending();
    let value = action.value();
    let (current_password, set_current_password) = signal(String::new());
    let (new_password, set_new_password) = signal(String::new());
    let (confirm_password, set_confirm_password) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    let can_submit = move || {
        let current = current_password.get();
        let next = new_password.get();
        let confirm = confirm_password.get();
        !pending.get()
            && !current.is_empty()
            && next.chars().count() >= 15
            && next == confirm
            && next != current
    };

    let disabled = Signal::derive(move || !can_submit());
    let success_msg = Signal::derive(move || {
        if matches!(value.get(), Some(Ok(_))) {
            Some("Password updated. Other sessions were signed out.".to_owned())
        } else {
            None
        }
    });

    view! {
        <section class=ACCOUNT_PANEL>
            <div class=ACCOUNT_PANEL_HEAD>
                <div>
                    <SectionLabel>"Credential"</SectionLabel>
                    <h2 class=ACCOUNT_PANEL_TITLE>"Change password"</h2>
                </div>
            </div>
            <p class=ACCOUNT_LEDE_FLUSH>
                "Enter your current password to confirm it's you. Use at least 15 characters for the new password. Other signed-in sessions will be signed out."
            </p>
            <FieldGroup>
                <Field label="Current password">
                    <TextInput
                        input_type="password"
                        autocomplete="current-password"
                        value=current_password
                        on_input=Callback::new(move |v: String| {
                            set_client_error.set(None);
                            set_current_password.set(v);
                        })
                    />
                </Field>
                <Field label="New password" hint="Minimum 15 characters. Prefer a long phrase.">
                    <TextInput
                        input_type="password"
                        autocomplete="new-password"
                        value=new_password
                        on_input=Callback::new(move |v: String| {
                            set_client_error.set(None);
                            set_new_password.set(v);
                        })
                    />
                </Field>
                <Field label="Confirm new password">
                    <TextInput
                        input_type="password"
                        autocomplete="new-password"
                        value=confirm_password
                        on_input=Callback::new(move |v: String| {
                            set_client_error.set(None);
                            set_confirm_password.set(v);
                        })
                    />
                </Field>
            </FieldGroup>
            <div class=ACCOUNT_CARD_ACTIONS>
                <PrimaryButton
                    disabled=disabled
                    on_click=Callback::new(move |_| {
                        let current = current_password.get_untracked();
                        let next = new_password.get_untracked();
                        let confirm = confirm_password.get_untracked();
                        if next.chars().count() < 15 {
                            set_client_error.set(Some("New password must be at least 15 characters.".to_owned()));
                            return;
                        }
                        if next != confirm {
                            set_client_error.set(Some("New password and confirmation do not match.".to_owned()));
                            return;
                        }
                        if next == current {
                            set_client_error.set(Some("New password must be different from the current password.".to_owned()));
                            return;
                        }
                        set_client_error.set(None);
                        action.dispatch(ChangePassword {
                            current_password: current,
                            new_password: next,
                        });
                    })
                >
                    {move || if pending.get() { "Updating password…" } else { "Update password" }}
                </PrimaryButton>
                <ErrorBanner message=client_error />
                <Show when=move || value.get().is_some()>
                    <p class=RESULT_LINE>{move || action_result_text(value.get())}</p>
                </Show>
                <SuccessBanner message=success_msg />
                <a class=AUTH_TEXT_LINK href="/forgot-password">"Forgot password? Use email reset"</a>
            </div>
        </section>
    }
}
