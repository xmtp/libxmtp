fn main() {
    uniffi::generate_scaffolding("./src/xmtp_dh.udl").expect("Building the UDL file failed");
}
