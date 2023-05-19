extern crate toml;
extern crate xmtp;
use rand::distributions::{Alphanumeric, DistString};
use std::{
    fs::File,
    io::{Read, Write},
    process::Command,
};
use toml::Table;
use xmtp::storage::unencrypted_store::{StorageOption, UnencryptedMessageStore};

const DIESEL_TOML: &str = "./diesel.toml";
fn main() {
    update_schemas_unencrypted_message_store().unwrap();
}

fn update_schemas_unencrypted_message_store() -> Result<(), std::io::Error> {
    let tmp_db = format!(
        "update-{}.db3",
        Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
    );

    {
        // Initalize DB to read the latest table definitions
        let _ = UnencryptedMessageStore::new(StorageOption::Peristent(tmp_db.clone())).unwrap();
    }

    match exec_diesel(&tmp_db) {
        Ok(v) => {
            let schema_path = get_schema_path()?;
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

fn get_schema_path() -> Result<String, std::io::Error> {
    let mut file = File::open(DIESEL_TOML)?;
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

    Ok(format!("./{}", schema_file_path))
}

fn exec_diesel(db: &str) -> Result<Vec<u8>, String> {
    let schema_defs = Command::new("diesel")
        .args(["print-schema", "--database-url", db])
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
