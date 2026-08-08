use anyhow::Result;
use edge_runtime::RuntimeProtocolCatalog;

fn main() -> Result<()> {
    serde_json::to_writer_pretty(std::io::stdout(), &RuntimeProtocolCatalog::all())?;
    println!();
    Ok(())
}
