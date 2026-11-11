use clob::Cluster;
use std::{
    io::{self, Read},
    process::ExitCode,
};
fn main() -> ExitCode {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("{e}");
        return ExitCode::from(2);
    }
    let mut cluster = Cluster::default();
    match cluster.run(&input) {
        Ok(()) => {
            print!("{}", cluster.render());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
