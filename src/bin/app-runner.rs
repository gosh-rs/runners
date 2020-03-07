// [[file:../../runners.note::*imports][imports:1]]
use gosh_core::gut::{cli::*, prelude::*};

use linefeed::{Interface, ReadResult};

use gosh_runner::client::*;
use gosh_runner::server::JobId;
// imports:1 ends here

// [[file:../../runners.note::*main][main:1]]
fn main() -> CliResult {
    let interface = Interface::new("application runner client")?;

    let version = env!("CARGO_PKG_VERSION");
    println!("This is the rusty gosh shell version {}.", version);
    println!("Enter \"help\" or \"?\" for a list of commands.");
    println!("Press Ctrl-D or enter \"quit\" or \"q\" to exit.");
    println!("");

    interface.set_prompt("app> ")?;

    let mut command = Command::new();
    while let ReadResult::Input(line) = interface.read_line()? {
        let line = line.trim();
        if !line.is_empty() {
            interface.add_history(line.to_owned());

            let mut args: Vec<_> = line.split_whitespace().collect();
            args.insert(0, "app>");

            match Action::from_iter_safe(&args) {
                // show subcommands
                Ok(Action::Help {}) => {
                    let mut app = Action::clap();
                    app.print_help();
                    println!("");
                }

                Ok(Action::Quit {}) => {
                    break;
                }

                // apply subcommand
                Ok(x) => {
                    if let Err(e) = command.apply(&x) {
                        eprintln!("{:?}", e);
                    }
                }

                // show subcommand usage
                Err(e) => {
                    println!("{}", e.message);
                }
            }
        } else {
            println!("");
        }
    }

    Ok(())
}
// main:1 ends here
