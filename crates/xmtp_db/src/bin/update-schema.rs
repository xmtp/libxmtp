extern crate toml;
extern crate xmtp_db;

use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use rand::distributions::{Alphanumeric, DistString};
use toml::Table;

use xmtp_db::{EncryptedMessageStore, NativeDb};

const DIESEL_TOML: &str = "diesel.toml";

/// This binary is used to to generate the schema files from a sqlite database instance and update
/// the appropriate file. The destination is read from the `diesel.toml` print_schema
/// configuration.
///
/// Since the migrations are embedded it can be difficult to have an instance available to run
/// diesel cli on. This binary creates a temporary sqlite instance and generates the schema
/// definitions from there.
///
/// To run the binary: `cargo run update-schema`
///
/// Notes:
/// - there is not great handling around tmp database cleanup in error cases.
/// - https://github.com/diesel-rs/diesel/issues/852 -> BigInts are weird.
fn main() {
    update_schemas_encrypted_message_store().unwrap();
}

fn update_schemas_encrypted_message_store() -> Result<(), std::io::Error> {
    let tmp_db = format!(
        "update-{}.db3",
        Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
    );

    {
        // Initialize DB to read the latest table definitions
        let db = NativeDb::builder()
            .persistent(tmp_db.clone())
            .build_unencrypted()
            .unwrap();
        let _ = EncryptedMessageStore::new(db).unwrap();
    }

    let diesel_result = exec_diesel(&tmp_db);
    if let Err(e) = fs::remove_file(tmp_db) {
        println!("Error Deleting Tmp DB: {}", e);
    }

    match diesel_result {
        Ok(v) => {
            let schema_path = get_schema_path()?;
            println!("Writing Schema definitions to {}", schema_path.display());
            let mut file = File::create(schema_path)?;
            file.write_all(&v)?;
        }
        Err(e) => {
            println!("Fatal Error: {}", e);
        }
    }

    Ok(())
}

fn get_schema_path() -> Result<PathBuf, std::io::Error> {
    match env::current_exe() {
        Ok(exe_path) => println!("Path of this executable is: {}", exe_path.display()),
        Err(e) => println!("failed to get current exe path: {e}"),
    };
    let manifest = env!("CARGO_MANIFEST_DIR");
    let diesel_toml = Path::new(manifest).join(DIESEL_TOML);
    println!(
        "Location of Diesel Configuration File: {}",
        diesel_toml.display()
    );
    let mut file = File::open(diesel_toml)?;
    let mut toml_contents = String::new();
    file.read_to_string(&mut toml_contents)?;
    let toml = toml_contents.parse::<Table>().unwrap();
    let schema_file_path = toml
        .get("print_schema")
        .unwrap()
        .get("file")
        .unwrap()
        .as_str()
        .unwrap();
    Ok(Path::new(manifest).join(schema_file_path))
}

fn exec_diesel(db: &str) -> Result<Vec<u8>, String> {
    let schema_defs = Command::new("diesel")
        .args(["print-schema", "--database-url", db, "-e", "client_events"])
        .output()
        .expect("failed to execute process");

    if !schema_defs.status.success() {
        return Err(format!(
            "Diesel-CLI failed to execute {} - {}",
            schema_defs.status.code().unwrap(),
            String::from_utf8(schema_defs.stderr).unwrap()
        ));
    }

    Ok(schema_defs.stdout)
}
