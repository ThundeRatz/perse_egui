mod app;
mod config;
mod panels;
mod rewire_integration;
mod state;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "perse_egui",
    version,
    about = "Aplicação baseada em egui com rewire-viewer incorporado"
)]
struct Cli {
    /// Endpoint do relay (`host`, `host:port`, ou `rerun+http://host:port/proxy`)
    #[arg(long, default_value = "127.0.0.1:9876")]
    connect: String,
}

#[global_allocator]
static GLOBAL: re_memory::AccountingAllocator<mimalloc::MiMalloc> =
    re_memory::AccountingAllocator::new(mimalloc::MiMalloc);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        re_log::setup_logging();
        re_crash_handler::install_crash_handlers(re_viewer::build_info());

        let mut native_options = re_viewer::native::eframe_options(None);
        native_options.viewport = native_options.viewport.with_app_id("perse_egui");

        eframe::run_native(
            "Perse Egui",
            native_options,
            Box::new(move |cc| Ok(Box::new(app::MyApp::new(cc, cli.connect.clone())?))),
        )?;

        Ok::<(), Box<dyn std::error::Error>>(())
    })
}
