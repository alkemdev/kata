mod ks;
mod tui;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
#[command(name = "kata", version, about = "The Kata language toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a KataScript source file
    Ks {
        /// Path to the .ks file
        file: PathBuf,
        /// Print the token stream as JSON and exit
        #[arg(long)]
        dump_tokens: bool,
        /// Print the AST as JSON and exit
        #[arg(long)]
        dump_ast: bool,
    },
    /// Launch the interactive KataScript REPL
    Repl,
}

fn main() {
    // Honour RUST_LOG; default to warnings so normal runs stay quiet.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Ks { file, dump_tokens, dump_ast } => {
            let filename = file.display().to_string();
            info!(file = %filename, "ks subcommand");

            let source = std::fs::read_to_string(&file).unwrap_or_else(|e| {
                eprintln!("kata: cannot read '{}': {e}", file.display());
                std::process::exit(1);
            });

            if dump_tokens {
                let tokens = ks::lex(&source);
                println!("{}", serde_json::to_string_pretty(&tokens).unwrap());
                return;
            }

            if dump_ast {
                match ks::parse(&source, &filename) {
                    Ok(ast) => println!("{}", serde_json::to_string_pretty(&ast).unwrap()),
                    Err(()) => std::process::exit(1),
                }
                return;
            }

            if let Err(()) = ks::run(&source, &filename) {
                std::process::exit(1);
            }
        }

        Command::Repl => {
            info!("launching REPL");
            if let Err(e) = tui::run_repl() {
                eprintln!("kata: REPL error: {e}");
                std::process::exit(1);
            }
        }
    }
}
