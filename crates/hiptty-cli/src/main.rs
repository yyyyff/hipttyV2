mod auth_prompt;
mod cli;
mod output;
mod run;

use clap::Parser;
use hiptty_adapter::DiscuzClient;

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();

    if cli.verbose {
        eprintln!(
            "hiptty profile={} config={:?}",
            cli.profile,
            cli.config.as_deref()
        );
    }

    let client = match DiscuzClient::new(cli.config.as_deref(), &cli.profile) {
        Ok(client) => client,
        Err(err) => {
            if cli.human {
                output::print_human_error(&err);
            } else {
                output::print_json(&output::Response::<()>::failure(err));
            }
            std::process::exit(output::exit::BUSINESS_ERROR);
        }
    };

    let code = run::execute(cli, &client).await;
    std::process::exit(code);
}
