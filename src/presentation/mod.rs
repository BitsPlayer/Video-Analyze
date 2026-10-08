//! Presentation layer: command-line interface.

mod cli;

pub fn run() -> anyhow::Result<()> {
    cli::run()
}
