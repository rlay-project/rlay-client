use dialoguer::Confirmation;
use serde_json::{self, Value};
use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;

pub fn run_init() {
    add_rlay_config().unwrap();

    update_package_json();
    println!("\nDone! 🎉");
}

fn add_rlay_config() -> ::std::io::Result<()> {
    println!("Adding \"rlay.config.toml\".");
    let content = include_str!("../../data/rlay.config.toml.default");
    let mut file = File::create(env::current_dir().unwrap().join("rlay.config.toml"))?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

fn update_package_json() {
    let package_json_path = env::current_dir().unwrap().join("package.json");
    if !package_json_path.exists() {
        return;
    }

    println!("Detected \"package.json\".");
    if !Confirmation::new(
        "Do you want to add the scripts and devDependencies to run a local Rlay testnet?",
    )
    .interact()
    .unwrap()
    {
        return;
    }

    let contents = {
        let file = File::open(&package_json_path).unwrap();
        let mut contents: Value = serde_json::from_reader(file).unwrap();
        println!("Adding to \"package.json\":");
        // scripts
        if contents.get("scripts").is_none() {
            contents
                .as_object_mut()
                .unwrap()
                .insert("scripts".to_owned(), json!({}));
        }
        if let Some(scripts) = contents.get_mut("scripts").unwrap().as_object_mut() {
            println!("  - scripts: \"testnet\"");
            scripts.insert(
                "testnet".to_owned(),
                json!("node_modules/.bin/ganache-cli --seed 1234"),
            );
            println!("  - scripts: \"testnet:deploy\"");
            scripts.insert(
                "testnet:deploy".to_owned(),
                json!("node_modules/.bin/rlay-deploy-contracts"),
            );
        }
        // devDependencies
        if contents.get("devDependencies").is_none() {
            contents
                .as_object_mut()
                .unwrap()
                .insert("devDependencies".to_owned(), json!({}));
        }
        if let Some(dependencies) = contents.get_mut("devDependencies").unwrap().as_object_mut() {
            println!("  - devDependencies: \"@rlay/protocol\"");
            dependencies.insert("@rlay/protocol".to_owned(), json!("0.3.2"));
            println!("  - devDependencies: \"ganache-cli\"");
            dependencies.insert("ganache-cli".to_owned(), json!("^6.1.0"));
        }

        contents
    };

    let file = OpenOptions::new()
        .write(true)
        .open(&package_json_path)
        .unwrap();
    serde_json::to_writer_pretty(file, &contents).unwrap();
}
