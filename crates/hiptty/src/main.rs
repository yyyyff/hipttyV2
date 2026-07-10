use clap::Parser;
use hiptty_adapter::DiscuzClient;
use hiptty_app::{config_dir, load_settings, run, App};
#[derive(Debug, Parser)]
#[command(name = "hiptty", version, about = "4d4y forum terminal client")]
struct Cli {
    /// Config directory override (default: ~/.config/hiptty)
    #[arg(long, env = "HIPTTY_CONFIG")]
    config: Option<std::path::PathBuf>,

    /// Session profile name
    #[arg(long, default_value = "default", env = "HIPTTY_PROFILE")]
    profile: String,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    let cfg_dir = config_dir(cli.config.as_deref())?;
    let settings = load_settings(&cfg_dir.join("settings.json"));

    let client = DiscuzClient::new(cli.config.as_deref(), &cli.profile)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    let app = App::new(settings, cfg_dir, cli.profile);

    if let Err(err) = run(app, client).await {
        return Err(color_eyre::eyre::eyre!("{err}"));
    }

    Ok(())
}
