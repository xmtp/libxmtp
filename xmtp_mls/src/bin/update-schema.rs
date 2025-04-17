extern crate toml;
extern crate xmtp_db;

use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::Command,
};

use rand::distributions::{Alphanumeric, DistString};
use toml::Table;

use xmtp_db::{EncryptedMessageStore, StorageOption};

const XMTP_DB_PATH: &str = "../xmtp_db";
const DIESEL_TOML: &str = "./diesel.toml";

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
#[tokio::main]
async fn main() {
    match update_schemas_encrypted_message_store().await {
        Ok(_) => println!("Schema updated successfully"),
        Err(e) => panic!("{:?}", e),
    }
}

async fn update_schemas_encrypted_message_store() -> Result<(), std::io::Error> {
    let tmp_db = format!(
        "update-{}.db3",
        Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
    );

    {
        // Initialize DB to read the latest table definitions
        let _ = EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmp_db.clone()))
            .await
            .unwrap();
    }
    let toml_output = parse_diesel_toml().unwrap();

    let diesel_result = exec_diesel(&tmp_db, &toml_output.patch_file_path);
    if let Err(e) = fs::remove_file(tmp_db) {
        println!("Error Deleting Tmp DB: {}", e);
    }

    match diesel_result {
        Ok(v) => {
            let schema_path = toml_output.schema_file_path;
            println!("Writing Schema definitions to {}", schema_path);
            let mut file = File::create(schema_path)?;
            file.write_all(&v)?;
        }
        Err(e) => {
            println!("Fatal Error: {}", e);
        }
    }

    Ok(())
}

fn get_command_output(command: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new(command)
        .args(args)
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        return Err(format!(
            "Command ({}) failed to execute with args [{}]\nStatus Code:{}\n\n{}",
            command,
            args.join(" "),
            output.status.code().unwrap(),
            String::from_utf8(output.stderr).unwrap()
        ));
    }

    Ok(output.stdout)
}

struct DieselTomlOutput {
    schema_file_path: String,
    patch_file_path: String,
}

fn parse_diesel_toml() -> Result<DieselTomlOutput, std::io::Error> {
    match env::current_exe() {
        Ok(exe_path) => println!("Path of this executable is: {}", exe_path.display()),
        Err(e) => println!("failed to get current exe path: {e}"),
    };

    let xmtp_db_path = Path::new(XMTP_DB_PATH);
    let diesel_toml_path = xmtp_db_path.join(DIESEL_TOML);
    let mut file = File::open(diesel_toml_path)?;
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

    let patch_file_path = toml
        .get("print_schema")
        .unwrap()
        .get("patch_file")
        .unwrap()
        .as_str()
        .unwrap();

    Ok(DieselTomlOutput {
        schema_file_path: xmtp_db_path
            .join(schema_file_path)
            .to_str()
            .unwrap()
            .to_string(),
        patch_file_path: xmtp_db_path
            .join(patch_file_path)
            .to_str()
            .unwrap()
            .to_string(),
    })
}

fn exec_diesel(db: &str, patch_file_path: &str) -> Result<Vec<u8>, String> {
    let output = get_command_output(
        "diesel",
        &[
            "print-schema",
            "--database-url",
            db,
            "--patch-file",
            patch_file_path,
        ],
    )
    .expect("command failed");

    Ok(output)
}
