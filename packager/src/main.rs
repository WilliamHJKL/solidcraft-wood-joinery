//! Turns the plugin's core WebAssembly module into the component SolidCraft loads.
//!
//! Usage: `packager <module.wasm> <component.wasm>`

use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let [_, module, component] = args.as_slice() else {
        eprintln!("usage: packager <module.wasm> <component.wasm>");
        return ExitCode::FAILURE;
    };
    let result = fs::read(module).map_err(|e| format!("{module}: {e}")).and_then(|bytes| {
        wit_component::ComponentEncoder::default()
            .validate(true)
            .module(&bytes)
            .and_then(|encoder| encoder.encode())
            .map_err(|e| format!("{module}: {e:#}"))
    });
    match result.and_then(|bytes| fs::write(component, bytes).map_err(|e| format!("{component}: {e}"))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
