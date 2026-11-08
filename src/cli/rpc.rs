//! `octa --rpc '<json>'` - ask the running Octa something.
//!
//! Connects over the same token-guarded loopback channel the single-
//! instance file hand-over uses (`<config dir>/instance` holds port +
//! token). The reply is one JSON line on stdout; a refused or unanswered
//! request is an error on stderr.
//!
//! ```
//! octa --rpc '{"method":"ping"}'
//! octa --rpc '{"method":"tabs"}'
//! octa --rpc '{"method":"enrich","params":{"tab":"@active","column":"url",
//!            "target":"live","extract":"status"}}'
//! ```

use super::OutputFormat;

pub fn run(line: &str, _format: OutputFormat) -> anyhow::Result<()> {
    let dir = octa::ui::settings::AppSettings::config_dir()
        .ok_or_else(|| anyhow::anyhow!("no config directory to find the running Octa in"))?;
    let reply = octa::single_instance::rpc(&dir, line)?;
    println!("{reply}");
    Ok(())
}
