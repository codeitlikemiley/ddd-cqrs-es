#![recursion_limit = "512"]

#[cfg(all(feature = "ssr", not(feature = "postgres")))]
compile_error!("the fullstack server requires the PostgreSQL storage feature");

// Optional product domain (business aggregates). Bootstrapped by
// `ddd add aggregate <Name>`; empty markers keep the stock example domain-free.
// ddd:product-domain
// ddd:product-domain:end
// Optional domain application + REST wiring (ssr only).
// ddd:product-domain-app
// ddd:product-domain-app:end

#[cfg(any(feature = "ssr", feature = "hydrate"))]
mod access;
#[cfg(any(feature = "ssr", feature = "hydrate"))]
mod app;
#[cfg(any(feature = "ssr", feature = "hydrate"))]
mod contracts;
#[cfg(any(feature = "ssr", feature = "hydrate"))]
mod ui;

#[cfg(all(target_arch = "wasm32", target_env = "p3"))]
mod wasip3_random;

#[cfg(feature = "ssr")]
mod application;

#[cfg(feature = "ssr")]
mod auth_product;

#[cfg(feature = "ssr")]
mod error;

#[cfg(feature = "ssr")]
mod oauth;

#[cfg(all(feature = "spin-grpc", runtime_spin))]
mod grpc;

#[cfg(feature = "ssr")]
mod rest;

#[cfg(feature = "ssr")]
mod server;

#[cfg(feature = "ssr")]
mod store;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
