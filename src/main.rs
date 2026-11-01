use clap::Parser;

use fodelta::cli::Cli;

fn main() {
    let cli = Cli::parse();
    match fodelta::run(cli) {
        Ok(code) => std::process::exit(code.code()),
        Err(err) => {
            eprintln!("fodelta: {err}");
            std::process::exit(err.exit_code());
        }
    }
}
