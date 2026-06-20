mod app;
mod config;
mod document;
mod highlight;
mod input;
mod mermaid;
mod render;
mod sync;
mod watcher;

use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};
use clap::{ArgAction, Parser};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const DEFAULT_WINDOW_SIZE: [f32; 2] = [1280.0, 900.0];

#[derive(Debug, Parser)]
#[command(
    name = "nvmd",
    about = "Lightweight native Markdown preview for Neovim."
)]
struct Cli {
    /// Markdown file to preview.
    path: PathBuf,

    /// Disable Mermaid rendering and show Mermaid source blocks instead.
    #[arg(long)]
    no_mermaid: bool,

    /// File containing the current editor cursor line for live synchronization.
    #[arg(long)]
    cursor_file: Option<PathBuf>,

    /// Markdown snapshot file supplied by an editor for unsaved live reload.
    #[arg(long)]
    content_file: Option<PathBuf>,

    /// Internal marker for the detached process that owns the native window.
    #[arg(long = "window-process", hide = true, action = ArgAction::SetTrue)]
    window_process: bool,
}

fn main() -> Result<()> {
    install_panic_log();

    let cli = Cli::parse();
    if !cli.window_process {
        return detach(cli);
    }

    run(cli)
}

fn run(cli: Cli) -> Result<()> {
    let config = config::Config::new(cli.path)?;
    let app_options = app::AppOptions {
        render_mermaid: !cli.no_mermaid,
        cursor_file: cli.cursor_file,
        content_file: cli.content_file,
    };
    let title = format!("nvmd - {}", config.file_name());

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(title.clone())
            .with_inner_size(DEFAULT_WINDOW_SIZE),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        &title,
        native_options,
        Box::new(
            move |cc| match app::NvmdApp::try_new(cc, config, app_options) {
                Ok(app) => Ok(Box::new(app)),
                Err(message) => Ok(Box::new(app::NvmdApp::from_startup_error(message))),
            },
        ),
    )
    .map_err(|err| anyhow::anyhow!("failed to start native window: {err}"))
}

fn detach(cli: Cli) -> Result<()> {
    let exe = std::env::current_exe().context("failed to resolve nvmd executable")?;
    let mut command = Command::new(exe);
    command.arg("--window-process").arg(cli.path);

    if let Some(cursor_file) = cli.cursor_file {
        command.arg("--cursor-file").arg(cursor_file);
    }
    if let Some(content_file) = cli.content_file {
        command.arg("--content-file").arg(content_file);
    }

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if cli.no_mermaid {
        command.arg("--no-mermaid");
    }

    #[cfg(unix)]
    command.process_group(0);

    command
        .spawn()
        .context("failed to launch detached nvmd preview")?;
    Ok(())
}

fn install_panic_log() {
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown location".to_owned());
        let message = if let Some(message) = info.payload().downcast_ref::<&str>() {
            (*message).to_owned()
        } else if let Some(message) = info.payload().downcast_ref::<String>() {
            message.clone()
        } else {
            "unknown panic payload".to_owned()
        };
        let log = format!("panic at {location}: {message}\n");
        let _ = std::fs::write("/tmp/nvmd-panic.log", &log);
        eprintln!("{log}");
    }));
}
