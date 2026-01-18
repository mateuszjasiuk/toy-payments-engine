mod engine;
mod types;

use std::{env, error::Error, ffi::OsString, process};

use crate::types::{common::CsvRow, transactions::Tx};

fn run() -> Result<(), Box<dyn Error>> {
    let file_path = get_first_arg()?;

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(file_path)?;

    for result in rdr.deserialize() {
        let record: CsvRow = result?;
        let tx = Tx::try_from(record).map_err(|_| "failed to convert CsvRow to Tx")?;
        println!("{:?}", tx);
    }
    Ok(())
}

/// Returns the first positional argument sent to this process. If there are no
/// positional arguments, then this returns an error.
fn get_first_arg() -> Result<OsString, Box<dyn Error>> {
    match env::args_os().nth(1) {
        None => Err(From::from("expected 1 argument, but got none")),
        Some(file_path) => Ok(file_path),
    }
}

fn main() {
    if let Err(err) = run() {
        println!("{}", err);
        process::exit(1);
    }
}
