mod engine;
mod types;

use std::{env, error::Error, ffi::OsString, process};

use crate::{
    engine::Engine,
    types::{common::CsvRow, transactions::Tx},
};

fn run() -> Result<(), Box<dyn Error>> {
    let file_path = get_first_arg()?;

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_path(file_path)?;
    let mut engine = Engine::new();

    for result in rdr.deserialize() {
        let record: CsvRow = match result {
            Ok(r) => r,
            Err(_) => continue, // Skip malformed CSV rows
        };

        let tx = match Tx::try_from(record) {
            Ok(t) => t,
            Err(_) => continue, // Skip invalid transaction types
        };

        engine.process_tx(tx);
    }

    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for (_client_id, client) in engine.clients().iter() {
        wtr.serialize(client)?;
    }
    wtr.flush()?;

    Ok(())
}

fn get_first_arg() -> Result<OsString, Box<dyn Error>> {
    match env::args_os().nth(1) {
        None => Err(From::from("Expected 1 argument, but got none")),
        Some(file_path) => Ok(file_path),
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{}", err);
        process::exit(1);
    }
}
