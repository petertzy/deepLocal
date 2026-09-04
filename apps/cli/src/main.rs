use clap::{Parser, Subcommand};
use deeplocal_core::{DeepLocalConfig, ModelDescriptor};
use deeplocal_runtime::{LlamaCppBackend, MockBackend, RuntimeManager};
use std::{fs, net::SocketAddr, path::PathBuf, sync::Arc};

#[derive(Debug, Parser)]
#[command(
    name = "deeplocal",
    version,
    about = "deepLocal local AI runtime and desktop studio"
)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Hardware,
    Serve {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
    },
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ModelsCommand {
    DescribeLocal { id: String, path: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let config = DeepLocalConfig::load(cli.config)?;

    match cli.command {
        Command::Hardware => {
            println!(
                "{}",
                serde_json::to_string_pretty(&deeplocal_hardware::detect_hardware())?
            );
        }
        Command::Serve { host, port } => {
            let runtime = RuntimeManager::default();
            runtime.register_backend(Arc::new(MockBackend)).await;
            runtime
                .register_backend(Arc::new(LlamaCppBackend::from_env()))
                .await;
            register_local_gguf_models(&runtime, &config.models.directory).await?;

            let host = host.unwrap_or(config.server.host);
            let port = port.unwrap_or(config.server.port);
            warn_if_public_bind(&host);
            let app = deeplocal_api::router_with_cors(runtime, config.server.enable_cors);
            let addr: SocketAddr = format!("{host}:{port}").parse()?;
            let listener = tokio::net::TcpListener::bind(addr).await?;
            println!("deepLocal API listening on http://{addr}");
            axum::serve(listener, app).await?;
        }
        Command::Models { command } => match command {
            ModelsCommand::DescribeLocal { id, path } => {
                let descriptor = ModelDescriptor::local_gguf(id, path);
                println!("{}", serde_json::to_string_pretty(&descriptor)?);
            }
        },
    }

    Ok(())
}

async fn register_local_gguf_models(
    runtime: &RuntimeManager,
    directory: &PathBuf,
) -> anyhow::Result<()> {
    for path in collect_gguf_files(directory)? {
        let id = path
            .strip_prefix(directory)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, ":");
        runtime
            .register_model(ModelDescriptor::local_gguf(
                id,
                path.to_string_lossy().to_string(),
            ))
            .await;
    }
    Ok(())
}

fn warn_if_public_bind(host: &str) {
    if matches!(host, "0.0.0.0" | "::" | "[::]") {
        eprintln!(
            "WARNING: deepLocal is binding to {host}. Devices on your network may be able to access loaded models and local API responses. Use 127.0.0.1 unless you intentionally need LAN access."
        );
    }
}

fn collect_gguf_files(directory: &PathBuf) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !directory.exists() {
        return Ok(files);
    }
    collect_gguf_files_inner(directory, &mut files)?;
    Ok(files)
}

fn collect_gguf_files_inner(directory: &PathBuf, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_gguf_files_inner(&path, files)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
        {
            files.push(path);
        }
    }
    Ok(())
}
