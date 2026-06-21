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


#[derive(Debug, Parser)]
#[command(
    name = "nvmd",
    about = "Lightweight native Markdown preview for Neovim."
)]
struct Cli {
    /// Markdown file to preview, or - to read from stdin.
    path: Option<PathBuf>,

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
    let is_stdin = cli.path.as_deref().map(|p| p == std::path::Path::new("-")).unwrap_or(true);
    let stdin_content = if is_stdin {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .context("failed to read markdown from stdin")?;
        Some(buf)
    } else {
        None
    };
    let path = if is_stdin {
        std::env::current_dir().unwrap_or_default().join("stdin.md")
    } else {
        cli.path.unwrap()
    };
    let config = config::Config::new(path)?;
    let viewer_settings = render::settings::ViewerSettings::load();
    let window_size = [
        viewer_settings.window_width.clamp(400.0, 3840.0),
        viewer_settings.window_height.clamp(300.0, 2160.0),
    ];
    let render_mermaid = if cli.no_mermaid {
        false
    } else {
        viewer_settings.enable_mermaid
    };
    let app_options = app::AppOptions {
        render_mermaid,
        cursor_file: cli.cursor_file,
        content_file: cli.content_file,
        stdin_content,
    };
    let title = if is_stdin {
        "nvmd - stdin".to_owned()
    } else {
        format!("nvmd - {}", config.file_name())
    };

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(title.clone())
            .with_inner_size(window_size),
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
    // Stdin mode can't be detached — the parent process holds stdin.
    let is_stdin = cli.path.as_deref().map(|p| p == std::path::Path::new("-")).unwrap_or(true);
    if is_stdin {
        return run(cli);
    }
    let exe = std::env::current_exe().context("failed to resolve nvmd executable")?;
    let mut command = Command::new(exe);
    command.arg("--window-process").arg(cli.path.as_deref().unwrap());

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
