#![recursion_limit = "256"]

mod app;
pub mod domain;

#[cfg(all(target_arch = "wasm32", target_env = "p3"))]
mod wasip3_random;

#[cfg(feature = "ssr")]
mod auth;

#[cfg(feature = "ssr")]
mod application;

#[cfg(feature = "ssr")]
mod error;

#[cfg(all(feature = "spin-grpc", runtime_spin))]
mod grpc;

#[cfg(feature = "ssr")]
mod rest;

#[cfg(feature = "ssr")]
pub mod store;

#[cfg(feature = "ssr")]
mod server;

/// This is the entrypoint called by the JS "igniter" script.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
