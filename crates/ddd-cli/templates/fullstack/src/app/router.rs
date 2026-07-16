//! Document shell and route table (keep this file route-only as the app grows).

use crate::app::dashboard::DashboardPage;
use crate::app::workspace::{AppLayout, WorkspaceOnboardingPage};
use crate::app::workspace_settings::{
    WorkspaceSettingsAuditPage, WorkspaceSettingsDangerPage, WorkspaceSettingsGeneralPage,
    WorkspaceSettingsIndexRedirect, WorkspaceSettingsInvitationsPage, WorkspaceSettingsMembersPage,
    WorkspaceSettingsRolesPage, WorkspaceSettingsShell,
};
use crate::app::{
    AccountMfaPage, AccountPasskeysPage, AccountPasswordPage, AccountProfilePage,
    AccountProvidersPage, AccountSessionsPage, AccountVaultRedirectPage, AdminHealthPage,
    AdminPoliciesPage, AdminUsersPage, AuthProviderAdminPage, AuthRequiredPage,
    AuthorizationPolicyPage, ForbiddenPage, ForgotPasswordPage, HomePage, InvitationAcceptPage,
    LoginPage, NotFoundPage, OAuthCallbackErrorPage, OAuthCallbackPage, OrgVaultPage,
    OrganizationAuditPage, OrganizationInvitationsPage, OrganizationMembersPage,
    OrganizationPermissionsPage, OrganizationRolesPage, OrganizationSettingsPage,
    OrganizationsPage, PasskeyUnsupportedPage, PublicProfilePage, RedirectAllowlistPage,
    RegisterPage, ResendVerificationPage, ResetPasswordPage, SessionExpiredPage,
    SigningKeyAdminPage, VerificationPendingPage, VerifyEmailPage,
};
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::*, path};

#[cfg(feature = "ssr")]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                // Apply sidebar preference before first paint so every navigation
                // keeps mini/hidden without flashing full → mini.
                <script>
                    {r#"(function(){try{var m=localStorage.getItem("workspace-sidebar-mode");if(m==="mini"||m==="hidden"||m==="full"){document.documentElement.setAttribute("data-sidebar-pref",m);}}catch(e){}})();"#}
                </script>
                <AutoReload options=options.clone() />
                <HydrationScripts options=options.clone() islands=true root="" />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let fallback = || view! { <NotFoundPage /> }.into_view();

    // ParentRoute + Outlet: workspace chrome mounts once and is reused across
    // authenticated navigations (islands-router). Only page content swaps.
    view! {
        <Stylesheet id="leptos" href="/pkg/fullstack_app.css" />
        // Inline SVG avoids a static-file route for /favicon.ico.
        <Link
            rel="icon"
            type_="image/svg+xml"
            href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='8' fill='%230d0d0d'/%3E%3Ctext x='16' y='22' text-anchor='middle' font-family='system-ui,sans-serif' font-size='16' font-weight='700' fill='%23fff'%3Ed%3C/text%3E%3C/svg%3E"
        />
        <Meta name="description" content="Production fullstack Rust with Leptos islands, verified sessions, REST, and Spin gRPC." />
        <Title text="wasi-auth / fullstack" />

        <Router>
            <Routes fallback>
                <ParentRoute path=path!("") view=AppLayout>
                    <Route path=path!("") view=HomePage />
                    <Route path=path!("/login") view=LoginPage />
                    <Route path=path!("/register") view=RegisterPage />
                    <Route path=path!("/forgot-password") view=ForgotPasswordPage />
                    <Route path=path!("/reset-password") view=ResetPasswordPage />
                    <Route path=path!("/verify-email") view=VerifyEmailPage />
                    <Route path=path!("/verify-email/pending") view=VerificationPendingPage />
                    <Route path=path!("/verify-email/resend") view=ResendVerificationPage />
                    <Route path=path!("/dashboard") view=DashboardPage />
                    <Route path=path!("/invitations/accept") view=InvitationAcceptPage />
                    <Route path=path!("/auth/callback/:provider") view=OAuthCallbackPage />
                    <Route path=path!("/auth/callback/:provider/error") view=OAuthCallbackErrorPage />
                    <Route path=path!("/auth/required") view=AuthRequiredPage />
                    <Route path=path!("/auth/forbidden") view=ForbiddenPage />
                    <Route path=path!("/auth/session-expired") view=SessionExpiredPage />
                    <Route path=path!("/auth/passkey-unsupported") view=PasskeyUnsupportedPage />
                    <Route path=path!("/account/profile") view=AccountProfilePage />
                    <Route path=path!("/account/password") view=AccountPasswordPage />
                    <Route path=path!("/account/providers") view=AccountProvidersPage />
                    <Route path=path!("/account/passkeys") view=AccountPasskeysPage />
                    <Route path=path!("/account/mfa") view=AccountMfaPage />
                    <Route path=path!("/account/sessions") view=AccountSessionsPage />
                    <Route path=path!("/account/vault") view=AccountVaultRedirectPage />
                    <Route path=path!("/onboarding/workspace") view=WorkspaceOnboardingPage />
                    <Route path=path!("/org/:slug/vault") view=OrgVaultPage />
                    // Nested so `:slug` is in scope for the settings shell (nav links).
                    <ParentRoute path=path!("/org/:slug/settings") view=WorkspaceSettingsShell>
                        <Route path=path!("") view=WorkspaceSettingsIndexRedirect />
                        <Route path=path!("/general") view=WorkspaceSettingsGeneralPage />
                        <Route path=path!("/members") view=WorkspaceSettingsMembersPage />
                        <Route path=path!("/invitations") view=WorkspaceSettingsInvitationsPage />
                        <Route path=path!("/roles") view=WorkspaceSettingsRolesPage />
                        <Route path=path!("/audit") view=WorkspaceSettingsAuditPage />
                        <Route path=path!("/danger") view=WorkspaceSettingsDangerPage />
                    </ParentRoute>
                    <Route path=path!("/u/:handle") view=PublicProfilePage />
                    <Route path=path!("/organizations") view=OrganizationsPage />
                    // Legacy org management → slug-scoped settings redirects.
                    <Route path=path!("/organizations/settings") view=OrganizationSettingsPage />
                    <Route path=path!("/organizations/members") view=OrganizationMembersPage />
                    <Route path=path!("/organizations/invitations") view=OrganizationInvitationsPage />
                    <Route path=path!("/organizations/roles") view=OrganizationRolesPage />
                    <Route path=path!("/organizations/permissions") view=OrganizationPermissionsPage />
                    <Route path=path!("/organizations/audit") view=OrganizationAuditPage />
                    <Route path=path!("/admin/users") view=AdminUsersPage />
                    <Route path=path!("/admin/health") view=AdminHealthPage />
                    <Route path=path!("/admin/policies") view=AdminPoliciesPage />
                    <Route path=path!("/admin/auth/signing-keys") view=SigningKeyAdminPage />
                    <Route path=path!("/admin/auth/providers") view=AuthProviderAdminPage />
                    <Route path=path!("/admin/auth/redirects") view=RedirectAllowlistPage />
                    <Route path=path!("/admin/authorization/policy") view=AuthorizationPolicyPage />
                    <Route path=path!("/*any") view=NotFoundPage />
                </ParentRoute>
            </Routes>
        </Router>
    }
}
