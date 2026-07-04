use std::env;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(runtime_spin)");
    println!("cargo::rustc-check-cfg=cfg(runtime_wasmtime)");
    println!("cargo::rerun-if-env-changed=WASI_RUNTIME");
    println!("cargo::rerun-if-env-changed=SPIN_BUILD");
    println!("cargo::rerun-if-changed=proto/auth.proto");
    println!("cargo::rerun-if-changed=proto/authz.proto");

    if env::var_os("CARGO_FEATURE_SPIN_GRPC").is_some() {
        tonic_build::configure()
            .build_transport(false)
            .compile_protos(&["proto/auth.proto", "proto/authz.proto"], &["proto"])
            .unwrap();
    }

    let runtime = env::var("WASI_RUNTIME").unwrap_or_else(|_| "wasmtime".to_string());
    println!("cargo:rustc-env=WASI_RUNTIME={runtime}");

    match runtime.as_str() {
        "spin" => {
            println!("cargo:rustc-cfg=runtime_spin");
            println!("cargo:warning=Building auth-stack for Spin runtime");
        }
        "wasmtime" => {
            println!("cargo:rustc-cfg=runtime_wasmtime");
            println!("cargo:warning=Building auth-stack for Wasmtime runtime");
        }
        _ => {
            println!("cargo:rustc-cfg=runtime_wasmtime");
            println!("cargo:warning=Unknown runtime '{runtime}', defaulting to Wasmtime");
        }
    }

    if env::var("SPIN_BUILD").is_ok() {
        println!("cargo:rustc-cfg=runtime_spin");
        println!("cargo:warning=SPIN_BUILD detected, building auth-stack for Spin");
    }
}
