mod cli;
mod state;

use cli::status::status_command;

fn main() {
    let args: Vec<String> =
        std::env::args().collect();

    if args.len() < 2 {
        eprintln!(
            "Usage: steam_deploy <command>"
        );
        std::process::exit(1);
    }

    let result = match args[1].as_str() {
        "status" => status_command(),
        _ => Err(format!(
            "unknown command: {}",
            args[1]
        )),
    };

    if let Err(error) = result {
        eprintln!("Error: {}", error);
        std::process::exit(1);
    }
}