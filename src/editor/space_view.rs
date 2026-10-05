use re_sdk_types::reflection::{ViewApplicability, ViewReflection};
use re_sdk_types::ViewClassIdentifier;
use re_viewer_context::{
    SystemExecutionOutput, ViewClass, ViewClassLayoutPriority, ViewClassRegistryError,
    ViewClassUiOutput, ViewQuery, ViewSpawnHeuristics, ViewState, ViewStateExt as _,
    ViewSystemExecutionError, ViewSystemRegistrator, ViewerContext,
};

#[derive(Default)]
pub struct MissionSpaceViewState {
    pub editor: crate::editor::MissionEditor,
}

impl ViewState for MissionSpaceViewState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn heap_size_bytes(&self) -> u64 {
        0
    }
}

#[derive(Default)]
pub struct MissionSpaceView;

impl MissionSpaceView {
    pub fn register(app: &mut re_viewer::App) -> Result<(), ViewClassRegistryError> {
        app.add_view_class::<Self>(ViewReflection {
            applicability: ViewApplicability::Archetypes(Default::default()),
        })
    }
}

impl ViewClass for MissionSpaceView {
    fn identifier() -> ViewClassIdentifier
    where
        Self: Sized,
    {
        "MissionEditor".into()
    }

    fn display_name(&self) -> &'static str {
        "Editor de Missão 2D"
    }

    fn icon(&self) -> &'static re_ui::Icon {
        &re_ui::icons::VIEW_2D
    }

    fn help(&self, _os: egui::os::OperatingSystem) -> re_ui::Help {
        re_ui::Help::new("Painel 2D de edição de pontos de missão e obstáculos")
    }

    fn on_register(
        &self,
        _system_registry: &mut ViewSystemRegistrator<'_>,
    ) -> Result<(), ViewClassRegistryError> {
        Ok(())
    }

    fn new_state(&self) -> Box<dyn ViewState> {
        Box::<MissionSpaceViewState>::default()
    }

    fn layout_priority(&self) -> ViewClassLayoutPriority {
        ViewClassLayoutPriority::Medium
    }

    fn spawn_heuristics(
        &self,
        _ctx: &ViewerContext<'_>,
        _include_entity: &dyn Fn(&re_log_types::EntityPath) -> bool,
    ) -> ViewSpawnHeuristics {
        ViewSpawnHeuristics::empty()
    }

    fn ui(
        &self,
        _ctx: &ViewerContext<'_>,
        _missing_chunk_reporter: &re_chunk_store::MissingChunkReporter,
        ui: &mut egui::Ui,
        state: &mut dyn ViewState,
        _query: &ViewQuery<'_>,
        _system_output: SystemExecutionOutput,
    ) -> Result<ViewClassUiOutput, ViewSystemExecutionError> {
        let state = state.downcast_mut::<MissionSpaceViewState>()?;
        let zoom_speed = crate::editor::get_shared_zoom_speed();
        state.editor.ui(ui, zoom_speed, None);
        Ok(ViewClassUiOutput::default())
    }
}
