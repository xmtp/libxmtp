use std::env;
// use std::process::Command;

fn main() {
//    println!("cargo::rerun-if-changed=package.js");
//    println!("cargo::rerun-if-changed=package.json");

    //    Command::new("yarn").args(["install"]).status().unwrap();
    //    Command::new("yarn")
    //        .args(["run", "build"])
    //        .status()
    //        .unwrap();

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch != "wasm32" {
        // Emit a compile error if the target is not wasm32-unknown-unknown
        panic!("This crate only supports the wasm32 architecture");
    }
}
