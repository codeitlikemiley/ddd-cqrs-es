//! Composable skeleton loaders for async UI.
//!
//! Build custom layouts from primitives (`SkeletonBone`, `SkeletonRow`, …) or use
//! recipes (`FormSkeleton`, `TableSkeleton`, `SettingsPageSkeleton`). Colours use
//! semantic `bg-surface-subtle` — see `DESIGN.md` (loading states).

#![allow(unused_imports)]

use super::classes::{
    SKEL_BONE, SKEL_BONE_LG, SKEL_BONE_MD, SKEL_BONE_SM, SKEL_CARD, SKEL_CIRCLE, SKEL_PAGE_HEADER,
    SKEL_PANEL, SKEL_ROOT, SKEL_ROW, SKEL_STACK, SKEL_TABLE_ROW, with_extra,
};
use leptos::prelude::*;

// ── Primitives ─────────────────────────────────────────────────────────────

/// Single pulse block. Pass `class` for width/height overrides (e.g. `"h-9 w-40"`).
#[component]
pub fn SkeletonBone(#[prop(optional, into)] class: Option<String>) -> impl IntoView {
    let class_name = with_extra(SKEL_BONE, class.as_deref());
    // Default size when no height/width utilities provided.
    let class_name = if class
        .as_deref()
        .is_some_and(|c| c.split_whitespace().any(|t| t.starts_with('h') || t.starts_with('w')))
    {
        class_name
    } else {
        format!("{class_name} h-3 w-full")
    };
    view! { <span class=class_name aria-hidden="true"></span> }
}

/// Multi-line text placeholder with staggered widths.
#[component]
pub fn SkeletonText(
    #[prop(default = 3)] lines: usize,
    #[prop(optional, into)] class: Option<String>,
) -> impl IntoView {
    let widths = ["w-[92%]", "w-[78%]", "w-[64%]", "w-[88%]", "w-[55%]", "w-[70%]"];
    let n = lines.max(1).min(8);
    let stack = with_extra(SKEL_STACK, class.as_deref());
    view! {
        <div class=stack aria-hidden="true">
            {(0..n)
                .map(|i| {
                    let w = widths[i % widths.len()];
                    let bone = with_extra(SKEL_BONE_MD, Some(w));
                    view! { <span class=bone></span> }
                })
                .collect_view()}
        </div>
    }
}

/// Circular avatar / monogram placeholder.
#[component]
pub fn SkeletonCircle(#[prop(optional, into)] class: Option<String>) -> impl IntoView {
    let class_name = with_extra(SKEL_CIRCLE, class.as_deref());
    view! { <span class=class_name aria-hidden="true"></span> }
}

/// Vertical stack of skeleton children.
#[component]
pub fn SkeletonStack(
    #[prop(optional, into)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let class_name = with_extra(SKEL_STACK, class.as_deref());
    view! { <div class=class_name>{children()}</div> }
}

/// Horizontal row of skeleton children.
#[component]
pub fn SkeletonRow(
    #[prop(optional, into)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let class_name = with_extra(SKEL_ROW, class.as_deref());
    view! { <div class=class_name>{children()}</div> }
}

/// Panel chrome matching product cards, with skeleton children inside.
#[component]
pub fn SkeletonPanel(
    #[prop(optional, into)] class: Option<String>,
    #[prop(optional, into)] label: Option<String>,
    children: Children,
) -> impl IntoView {
    let class_name = with_extra(SKEL_PANEL, class.as_deref());
    let aria = label.unwrap_or_else(|| "Loading".into());
    view! {
        <section class=class_name aria-busy="true" aria-label=aria>
            {children()}
        </section>
    }
}

// ── Recipes ────────────────────────────────────────────────────────────────

/// Page title + subtitle bones (settings / account headers).
#[component]
pub fn PageHeaderSkeleton(#[prop(optional, into)] class: Option<String>) -> impl IntoView {
    let class_name = with_extra(SKEL_PAGE_HEADER, class.as_deref());
    view! {
        <header class=class_name aria-hidden="true">
            <span class=with_extra(SKEL_BONE, Some("h-7 w-[min(280px,55%)]"))></span>
            <span class=with_extra(SKEL_BONE_SM, Some("w-[min(420px,72%)]"))></span>
        </header>
    }
}

/// Label + control row × `fields` inside a panel.
#[component]
pub fn FormSkeleton(
    #[prop(default = 3)] fields: usize,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] class: Option<String>,
) -> impl IntoView {
    let n = fields.max(1).min(12);
    let aria = label.unwrap_or_else(|| "Loading form".into());
    let panel = with_extra(SKEL_PANEL, class.as_deref());
    view! {
        <section class=panel aria-busy="true" aria-label=aria>
            <div class=SKEL_STACK>
                {(0..n)
                    .map(|_| {
                        view! {
                            <div class=SKEL_STACK>
                                <span class=with_extra(SKEL_BONE_SM, Some("w-24"))></span>
                                <span class=SKEL_BONE_LG></span>
                            </div>
                        }
                    })
                    .collect_view()}
                <div class=with_extra(SKEL_ROW, Some("mt-2"))>
                    <span class=with_extra(SKEL_BONE, Some("h-10 w-28 rounded-[10px]"))></span>
                </div>
            </div>
        </section>
    }
}

/// Table-like list: optional avatar column + `cols` cell bones × `rows`.
#[component]
pub fn TableSkeleton(
    #[prop(default = 5)] rows: usize,
    #[prop(default = 3)] cols: usize,
    #[prop(optional)] with_avatar: bool,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] class: Option<String>,
) -> impl IntoView {
    let n_rows = rows.max(1).min(20);
    let n_cols = cols.max(1).min(8);
    let aria = label.unwrap_or_else(|| "Loading table".into());
    let panel = with_extra(SKEL_PANEL, class.as_deref());
    // Header toolbar bones
    view! {
        <section class=panel aria-busy="true" aria-label=aria>
            <div class=with_extra(SKEL_ROW, Some("mb-1 justify-between"))>
                <span class=with_extra(SKEL_BONE, Some("h-9 w-40 rounded-[10px]"))></span>
                <span class=with_extra(SKEL_BONE, Some("h-9 w-28 rounded-[10px]"))></span>
            </div>
            <div class=SKEL_STACK>
                {(0..n_rows)
                    .map(|_| {
                        view! {
                            <div class=SKEL_TABLE_ROW>
                                {with_avatar.then(|| {
                                    view! { <SkeletonCircle /> }.into_any()
                                })}
                                <div class=with_extra(SKEL_ROW, Some("min-w-0 flex-[1_1_auto]"))>
                                    {(0..n_cols)
                                        .map(|i| {
                                            let w = match i % 3 {
                                                0 => "w-[28%]",
                                                1 => "w-[22%]",
                                                _ => "w-[18%]",
                                            };
                                            let bone = with_extra(SKEL_BONE_MD, Some(w));
                                            view! { <span class=bone></span> }
                                        })
                                        .collect_view()}
                                </div>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </section>
    }
}

/// Stack of full-width rows (sessions, simple lists).
#[component]
pub fn ListSkeleton(
    #[prop(default = 4)] rows: usize,
    #[prop(optional)] with_avatar: bool,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] class: Option<String>,
) -> impl IntoView {
    let n = rows.max(1).min(16);
    let aria = label.unwrap_or_else(|| "Loading list".into());
    let root = with_extra(SKEL_ROOT, class.as_deref());
    view! {
        <div class=root aria-busy="true" aria-label=aria>
            {(0..n)
                .map(|_| {
                    view! {
                        <div class=with_extra(
                            SKEL_ROW,
                            Some("rounded-[12px] border border-border-subtle bg-surface px-3.5 py-3"),
                        )>
                            {with_avatar.then(|| view! { <SkeletonCircle /> }.into_any())}
                            <div class=with_extra(SKEL_STACK, Some("min-w-0 flex-1"))>
                                <span class=with_extra(SKEL_BONE_MD, Some("w-[45%]"))></span>
                                <span class=with_extra(SKEL_BONE_SM, Some("w-[70%]"))></span>
                            </div>
                            <span class=with_extra(SKEL_BONE, Some("h-8 w-16 rounded-lg"))></span>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// Card grid (dashboard-style tiles).
#[component]
pub fn CardGridSkeleton(
    #[prop(default = 6)] cards: usize,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] class: Option<String>,
) -> impl IntoView {
    let n = cards.max(1).min(12);
    let aria = label.unwrap_or_else(|| "Loading cards".into());
    let root = with_extra(SKEL_ROOT, class.as_deref());
    view! {
        <div class=root aria-busy="true" aria-label=aria>
            <span class=with_extra(SKEL_BONE, Some("h-[72px] w-full rounded-[14px]"))></span>
            <div class="grid grid-cols-2 gap-3 min-[901px]:grid-cols-3">
                {(0..n)
                    .map(|_| view! { <span class=SKEL_CARD></span> })
                    .collect_view()}
            </div>
        </div>
    }
}

/// Settings body recipes (form vs table vs compact danger panel).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsSkeletonVariant {
    #[default]
    Form,
    Table,
    List,
    Danger,
}

/// Full settings content skeleton: optional header + variant body.
#[component]
pub fn SettingsPageSkeleton(
    #[prop(into)] label: String,
    #[prop(optional)] variant: Option<SettingsSkeletonVariant>,
    #[prop(default = true)] show_header: bool,
) -> impl IntoView {
    let variant = variant.unwrap_or_default();
    let label_body = label.clone();
    view! {
        <div class=SKEL_ROOT aria-busy="true" aria-label=label>
            {show_header.then(|| view! { <PageHeaderSkeleton /> }.into_any())}
            {match variant {
                SettingsSkeletonVariant::Form => view! {
                    <FormSkeleton fields=3 label=label_body />
                }
                .into_any(),
                SettingsSkeletonVariant::Table => view! {
                    <TableSkeleton rows=6 cols=3 with_avatar=true label=label_body />
                }
                .into_any(),
                SettingsSkeletonVariant::List => view! {
                    <ListSkeleton rows=5 with_avatar=false label=label_body />
                }
                .into_any(),
                SettingsSkeletonVariant::Danger => view! {
                    <SkeletonPanel label=label_body>
                        <SkeletonText lines=2 />
                        <span class=with_extra(SKEL_BONE_LG, Some("mt-2 max-w-md"))></span>
                        <div class=with_extra(SKEL_ROW, Some("mt-3"))>
                            <span class=with_extra(SKEL_BONE, Some("h-10 w-36 rounded-[10px]"))></span>
                            <span class=with_extra(SKEL_BONE, Some("h-10 w-28 rounded-[10px]"))></span>
                        </div>
                    </SkeletonPanel>
                }
                .into_any(),
            }}
        </div>
    }
}

/// Compact chrome placeholder (org switcher / user menu foot).
#[component]
pub fn ChromeRowSkeleton(#[prop(optional, into)] label: Option<String>) -> impl IntoView {
    let aria = label.unwrap_or_else(|| "Loading".into());
    view! {
        <div
            class=with_extra(SKEL_ROW, Some("min-h-11 w-full px-1"))
            aria-busy="true"
            aria-label=aria
        >
            <SkeletonCircle class="h-8 w-8".to_string() />
            <div class=with_extra(SKEL_STACK, Some("min-w-0 flex-1 shell-mini:hidden"))>
                <span class=with_extra(SKEL_BONE_MD, Some("w-[70%]"))></span>
                <span class=with_extra(SKEL_BONE_SM, Some("w-[45%]"))></span>
            </div>
        </div>
    }
}
