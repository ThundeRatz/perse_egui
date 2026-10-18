use std::error::Error;

use eframe::{CreationContext, Frame};
use egui::{Context, Ui};
#[cfg(not(target_arch = "wasm32"))]
use re_grpc_server::MessageProxyHandle;
use rewire_viewer::{app::RewireApp, views};
#[cfg(not(target_arch = "wasm32"))]
use rewire_viewer::{connection::RelayLink, control};

pub type IntegrationResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct RewireIntegration {
    viewer: RewireApp,
    #[cfg(target_arch = "wasm32")]
    stream_url: String,
    #[cfg(not(target_arch = "wasm32"))]
    _control_handle: MessageProxyHandle,
}

impl RewireIntegration {
    pub fn create(cc: &CreationContext<'_>, endpoint: &str) -> IntegrationResult<Self> {
        let main_thread_token = re_viewer::MainThreadToken::i_promise_i_am_on_the_main_thread();

        re_viewer::customize_eframe_and_setup_renderer(cc)?;

        let mut rerun_app = re_viewer::App::new(
            main_thread_token,
            re_viewer::build_info(),
            re_viewer::AppEnvironment::Custom("perse_egui".to_owned()),
            re_viewer::StartupOptions::default(),
            cc,
            None,
            re_viewer::AsyncRuntimeHandle::from_current_tokio_runtime_or_wasmbindgen()?,
        );

        rerun_app.app_options_mut().check_for_updates_on_startup = false;

        views::TopicsView::register(&mut rerun_app)?;
        views::NodesView::register(&mut rerun_app)?;
        views::DiagnosticsView::register(&mut rerun_app)?;
        crate::editor::space_view::MissionSpaceView::register(&mut rerun_app)?;

        #[cfg(not(target_arch = "wasm32"))]
        {
            let uri: re_uri::ProxyUri = RelayLink::normalize(endpoint).parse()?;
            let (link, log_receiver) = RelayLink::open(uri);
            let (control_handle, control_receiver) = control::spawn();

            rerun_app.add_log_receiver(log_receiver);
            rerun_app.add_log_receiver(control_receiver);

            // Garante que o blueprint padrão esteja presente no diretório de persistência do Rerun (~/.local/share/rerun/blueprints/rewire.rbl)
            // Dessa forma o Rerun carrega o blueprint nativamente sem criar data sources nem poluir timelines com chunks de arquivo
            let web_dir = crate::net::daemon::resolve_web_directory(None);
            let default_bpt = web_dir.join("default_blueprint.rbl");
            if default_bpt.exists() {
                if let Some(storage_dir) = eframe::storage_dir("rerun") {
                    let dest_dir = storage_dir.join("blueprints");
                    let _ = std::fs::create_dir_all(&dest_dir);
                    let target_rbl = dest_dir.join("rewire.rbl");
                    if !target_rbl.exists() {
                        if let Err(err) = std::fs::copy(&default_bpt, &target_rbl) {
                            re_log::warn!(
                                "Falha ao copiar blueprint padrão para {target_rbl:?}: {err}"
                            );
                        } else {
                            re_log::info!("Blueprint padrão semeado com sucesso em {target_rbl:?}");
                        }
                    }
                }
            }

            let viewer = RewireApp::new(rerun_app, link);

            Ok(Self {
                viewer,
                _control_handle: control_handle,
            })
        }

        #[cfg(target_arch = "wasm32")]
        {
            let mut url = if endpoint.is_empty() {
                let host = web_sys::window()
                    .and_then(|w| w.location().hostname().ok())
                    .unwrap_or_else(|| "127.0.0.1".to_string());
                format!("rerun+http://{}:9876/proxy", host)
            } else if !endpoint.starts_with("rerun+http://")
                && !endpoint.starts_with("rerun+https://")
                && !endpoint.starts_with("http://")
                && !endpoint.starts_with("https://")
                && !endpoint.starts_with("ws://")
                && !endpoint.starts_with("wss://")
            {
                format!("rerun+http://{}/proxy", endpoint.trim_end_matches('/'))
            } else {
                endpoint.to_string()
            };

            if (url.starts_with("rerun+http://") || url.starts_with("http://"))
                && !url.ends_with("/proxy")
            {
                url = format!("{}/proxy", url.trim_end_matches('/'));
            }

            if !url.is_empty() {
                re_log::info!("Opening WASM Rerun URL: {}", url);
                rerun_app.open_url_or_file(&url);
            }

            let blueprint_url = if let Some(window) = web_sys::window() {
                let loc = window.location();
                let origin = loc.origin().unwrap_or_default();
                if !origin.is_empty() {
                    format!("{}/default_blueprint.rbl", origin.trim_end_matches('/'))
                } else {
                    "default_blueprint.rbl".to_string()
                }
            } else {
                "default_blueprint.rbl".to_string()
            };
            re_log::info!("Carregando blueprint padrão WASM: {}", blueprint_url);
            rerun_app.open_url_or_file(&blueprint_url);

            let viewer = RewireApp::new(rerun_app);
            Ok(Self {
                viewer,
                stream_url: url,
            })
        }
    }

    pub fn reconnect(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            if !self.stream_url.is_empty() {
                re_log::info!("Reconectando ao stream Rerun/Rewire: {}", self.stream_url);
                self.viewer.open_url_or_file(&self.stream_url);
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn stream_url(&self) -> &str {
        &self.stream_url
    }

    pub fn logic(&mut self, ctx: &Context, frame: &mut Frame) {
        eframe::App::logic(&mut self.viewer, ctx, frame);
        crate::editor::space_view::sync_mission_points_to_rerun(self.viewer.rerun_app());
    }

    pub fn show(&mut self, ui: &mut Ui, frame: &mut Frame) {
        if ui.available_width() >= 320.0 && ui.available_height() >= 200.0 {
            eframe::App::ui(&mut self.viewer, ui, frame);
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("Espaço insuficiente para exibir o visualizador central.")
                        .italics()
                        .color(egui::Color32::from_gray(140)),
                );
            });
        }
    }

    pub fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::App::save(&mut self.viewer, storage);
    }
}
