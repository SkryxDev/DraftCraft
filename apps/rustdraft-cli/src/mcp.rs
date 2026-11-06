use super::args::{McpArgs, McpTarget};
use super::error::{CliError, Result};

pub(crate) fn mcp(a: &McpArgs) -> Result {
    let backend: Box<dyn rustdraft_mcp::Backend> = match a.target() {
        McpTarget::Connect { addr, lazy: true, launch } => Box::new(rustdraft_mcp::Remote::lazy(&addr, launch)),
        McpTarget::Connect { addr, .. } => match rustdraft_mcp::Remote::connect(&addr) {
            Ok(r) => Box::new(r),
            Err(source) => return Err(CliError::McpConnect { addr, source }),
        },
        McpTarget::Headless => Box::new(rustdraft_mcp::Headless::default()),
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    rustdraft_mcp::Server::new(backend).serve(stdin.lock(), stdout.lock())?;
    Ok(())
}
