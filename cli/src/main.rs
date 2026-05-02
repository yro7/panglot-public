use clap::{Parser, Subcommand};
use anyhow::Result;

#[derive(Parser)]
#[command(name = "panglot")]
#[command(about = "Panglot Language Learning CLI Engine", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate an Anki deck from a topic
    Generate {
        #[arg(short, long)]
        language: String,
        
        #[arg(short, long)]
        topic: String,
    },
    /// Parse and analyze morphological features of a text
    Analyze {
        #[arg(short, long)]
        language: String,
        
        #[arg(short, long)]
        text: String,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Generate { language, topic } => {
            println!("Generating deck for language: {} with topic: {}", language, topic);
            // TODO: Wire up engine::Generator and anki_bridge::export
            println!("Not implemented yet.");
        }
        Commands::Analyze { language, text } => {
            println!("Analyzing text in {}: {}", language, text);
            // TODO: Wire up panini feature extraction via engine
            println!("Not implemented yet.");
        }
    }

    Ok(())
}
