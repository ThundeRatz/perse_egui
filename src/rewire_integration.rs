use std::error::Error;

use eframe::{CreationContext, Frame};
use egui::{Context, Ui};
use re_grpc_server::MessageProxyHandle;
use rewire_viewer::{app::RewireApp, connection::RelayLink, control, views};

pub type IntegrationResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct RewireIntegration {
    viewer: RewireApp,
    _control_handle: MessageProxyHandle,
}

impl RewireIntegration {
    pub fn create(cc: &CreationContext<'_>, endpoint: &str) -> IntegrationResult<Self> {
        let main_thread_token = re_viewer::MainThreadToken::i_promise_i_am_on_the_main_thread();

        re_viewer::customize_eframe_and_setup_renderer(cc)?;

        let uri: re_uri::ProxyUri = RelayLink::normalize(endpoint).parse()?;
        let (link, log_receiver) = RelayLink::open(uri);
        let (control_handle, control_receiver) = control::spawn();

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

        rerun_app.add_log_receiver(log_receiver);
        rerun_app.add_log_receiver(control_receiver);

        let viewer = RewireApp::new(rerun_app, link);

        Ok(Self {
            viewer,
            _control_handle: control_handle,
        })
    }

    pub fn logic(&mut self, ctx: &Context, frame: &mut Frame) {
        eframe::App::logic(&mut self.viewer, ctx, frame);
    }

    pub fn show(&mut self, ui: &mut Ui, frame: &mut Frame) {
        eframe::App::ui(&mut self.viewer, ui, frame);
    }

    pub fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::App::save(&mut self.viewer, storage);
    }
}
