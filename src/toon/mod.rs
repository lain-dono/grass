pub mod core_pipeline;
pub mod grass;
pub mod normal_pass;
pub mod outline;
pub mod postprocess;

pub use self::grass::GrassPlugin;
pub use self::normal_pass::NormalPassPlugin;
pub use self::outline::OutlinePlugin;
pub use self::postprocess::PostprocessPassPlugin;

pub fn replace_core_pipeline(
    group: &mut bevy::app::PluginGroupBuilder,
) -> &mut bevy::app::PluginGroupBuilder {
    group
        .disable::<bevy::core_pipeline::CorePipelinePlugin>()
        .add_after::<bevy::render::RenderPlugin, _>(self::core_pipeline::CorePipelinePlugin)
}

pub struct _DbgRenderPlugin;

impl bevy::prelude::Plugin for _DbgRenderPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        use bevy::render::RenderStage;

        let render_app = match app.get_sub_app_mut(bevy::render::RenderApp) {
            Ok(render_app) => render_app,
            Err(_) => return,
        };

        fn extract() {
            dbg!()
        }
        fn prepare() {
            dbg!()
        }
        fn queue() {
            dbg!()
        }
        fn sort() {
            dbg!()
        }
        fn render() {
            dbg!()
        }
        fn cleanup() {
            dbg!()
        }

        render_app
            .add_system_to_stage(RenderStage::Extract, extract)
            .add_system_to_stage(RenderStage::Prepare, prepare)
            .add_system_to_stage(RenderStage::Queue, queue)
            .add_system_to_stage(RenderStage::PhaseSort, sort)
            .add_system_to_stage(RenderStage::Render, render)
            .add_system_to_stage(RenderStage::Cleanup, cleanup);
    }
}
