fn main() {
    uniffi::generate_scaffolding("./src/xmtpv3.udl").expect("Building the UDL file failed");
}
