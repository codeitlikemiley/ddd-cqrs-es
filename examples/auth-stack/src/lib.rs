#![recursion_limit = "512"]

mod app;
mod contracts;

#[cfg(feature = "ssr")]
mod application;

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
    leptos::mount::hydrate_body(app::App);
}
