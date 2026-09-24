//! Save a verified API credential in Nudge's user configuration directory.

use std::io::{self, IsTerminal, Read};

use clap::{Args, Subcommand};
use color_eyre::eyre::{Context, Result, eyre};
use nudge::{credentials, semantic::JevClient};

#[derive(Args)]
pub struct Config {
    #[command(subcommand)]
    provider: Provider,
}

#[derive(Subcommand)]
enum Provider {
    /// Save a TypeSafe API key for Jev semantic rules.
    #[command(name = "typesafe.ai")]
    TypeSafe {
        /// Read the API key from stdin instead of a hidden terminal prompt.
        #[arg(long)]
        stdin: bool,
    },
}

pub fn main(config: Config) -> Result<()> {
    let Provider::TypeSafe { stdin } = config.provider;
    let key = if stdin {
        let mut key = String::new();
        io::stdin()
            .read_to_string(&mut key)
            .context("read API key from stdin")?;
        key
    } else {
        if !io::stdin().is_terminal() {
            return Err(eyre!(
                "use `nudge login typesafe.ai --stdin` to read a piped API key"
            ));
        }
        eprintln!("Create a key at https://console.typesafe.ai/keys");
        rpassword::prompt_password("TypeSafe API key (hidden): ").context("read API key")?
    };
    let key = key.trim();
    credentials::validate_key(key)?;
    JevClient::default()
        .verify_key(key)
        .map_err(|error| eyre!("Jev login failed: {error}"))?;
    let path = credentials::save_jev(key).context("save Jev credential")?;
    println!("Jev credential verified and saved to {}", path.display());
    println!("Hooks and `nudge check` can now use it. TYPESAFE_API_KEY overrides this saved key.");
    Ok(())
}
