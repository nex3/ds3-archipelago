use darksouls3::sprj::SprjTaskImp;
use fromsoftware_shared::singleton::get_instance;
use hudhook::{ImguiRenderLoop, RenderContext};
use imgui::*;
use log::*;

use crate::config::Config;

/// The fully-initialized Archipelago mod at the whole-game level. This doesn't
/// contain anything specific to a loaded game instance.
pub struct ArchipelagoMod {
    /// The configuration for the current Archipelago connection. This is not
    /// guaranteed to be complete *or* accurate; it's the mod's responsibility
    /// to ensure it makes sense before actually interacting with an individual
    /// game.
    config: Config,

    /// The last-known size of the viewport. This is only set once hudhook has
    /// been initialized and the viewport has a non-zero size.
    viewport_size: Option<[f32; 2]>,

    task_imp: &'static mut SprjTaskImp,
}

impl ArchipelagoMod {
    pub fn new() -> ArchipelagoMod {
        let config = match Config::load_or_default() {
            Ok(config) => config,
            Err(e) => panic!("Failed to load config: {e:?}"),
        };

        let Some(task_imp) = (unsafe { get_instance::<SprjTaskImp>() }) else {
            panic!("Couldn't load SprjTaskImp");
        };

        Self {
            config,
            viewport_size: None,
            task_imp,
        }
    }
}

impl ImguiRenderLoop for ArchipelagoMod {
    fn render(&mut self, ui: &mut Ui) {
        let Some(viewport_size) = self.viewport_size else {
            // Work around veeenu/hudhook#235
            ui.window("tmp")
                .size([100., 100.], Condition::Always)
                .position([-200., -200.], Condition::Always)
                .build(|| {});
            return;
        };

        ui.window("Archipelago")
            .position([viewport_size[0] - 30., 30.], Condition::FirstUseEver)
            .position_pivot([1., 0.])
            .size([viewport_size[0] * 0.4, 200.], Condition::FirstUseEver)
            .build(|| {
                let scale = 1.8;
                ui.set_window_font_scale(scale);

                let pos_before_text = ui.cursor_pos();
                ui.text_wrapped("Connection status: ");
                // All cusor positions are just magic numbers found by testing.
                // As far as I know imgui has no better way of doing this.
                ui.set_cursor_pos([pos_before_text[0] + 130.0 * scale, pos_before_text[1]]);
                ui.text_colored(
                    ImColor32::from_rgb(0xff, 0x44, 0x44).to_rgba_f32s(),
                    "Disconnected",
                );
                ui.separator();
            });
    }

    fn before_render<'a>(
        &'a mut self,
        ctx: &mut Context,
        _render_context: &'a mut dyn RenderContext,
    ) {
        self.viewport_size = match ctx.main_viewport().size {
            [0., 0.] => None,
            size => Some(size),
        };
    }
}
