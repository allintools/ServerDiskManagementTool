#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = nova_rack_lib::askpass_main() {
        std::process::exit(code);
    }
    if std::env::args().any(|arg| arg == "--selftest-ssh") {
        match nova_rack_lib::selftest_ssh() {
            Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())),
            Err(error) => { eprintln!("selftest failed: {error}"); std::process::exit(1); }
        }
        return;
    }
    nova_rack_lib::run()
}
