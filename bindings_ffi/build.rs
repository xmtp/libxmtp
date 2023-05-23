// TODO: took from swift-bridge example, but not sure if this is needed
// https://chinedufn.github.io/swift-bridge/building/xcode-and-cargo/index.html
// const XCODE_CONFIGURATION_ENV: &str = "CONFIGURATION";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    uniffi::generate_scaffolding("src/xmtpv3.udl").unwrap();
    // == swift-bridge generation code ==

    // let out_dir = "Generated";

    // let bridges = vec!["src/lib.rs"];
    // for path in &bridges {
    //     println!("cargo:rerun-if-changed={}", path);
    // }
    // println!("cargo:rerun-if-env-changed={}", XCODE_CONFIGURATION_ENV);

    // swift_bridge_build::parse_bridges(bridges)
    //     .write_all_concatenated(out_dir, env!("CARGO_PKG_NAME"));
    // == end swift-bridge generation code ==
    Ok(())
}
