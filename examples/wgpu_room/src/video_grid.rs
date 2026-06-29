use std::cmp;

#[derive(Debug, Clone, Default, PartialEq)]
struct State {
    num_videos: u32,
}

impl State {
    pub fn load(ctx: &egui::Context, id: egui::Id) -> Option<Self> {
        ctx.data(|i| i.get_temp(id))
    }

    pub fn store(self, ctx: &egui::Context, id: egui::Id) {
        ctx.data_mut(|i| i.insert_temp(id, self))
    }
}

pub const DEFAULT_VIDEO_SIZE: egui::Vec2 = egui::vec2(320.0 / 1.4, 180.0 / 1.4);
pub const DEFAULT_MAX_COLUMNS: u32 = 4;
pub const DEFAULT_SPACING: f32 = 8.0;

pub struct VideoGrid {
    id: egui::Id,

    // Current frame
    available_rect: egui::Rect,
    prev_state: State,
    curr_state: State,
    video_index: u32, // Kinda "cursor"

    // Options
    min_video_size: egui::Vec2,
    max_columns: u32,
    spacing: f32,
}

impl VideoGrid {
    pub fn new(id_source: impl std::hash::Hash) -> Self {
        Self {
            id: egui::Id::new(id_source),
            available_rect: egui::Rect::NAN,
            prev_state: State::default(),
            curr_state: State::default(),
            video_index: 0,
            min_video_size: DEFAULT_VIDEO_SIZE,
            max_columns: DEFAULT_MAX_COLUMNS,
            spacing: DEFAULT_SPACING,
        }
    }

    pub fn show<R>(
        mut self,
        ui: &mut egui::Ui,
        grid: impl FnOnce(&mut VideoGridContext) -> R,
    ) -> egui::InnerResponse<R> {
        // TODO(theomonnom): Should I care about the current egui layout?

        let prev_state = State::load(ui.ctx(), self.id);
        let is_first_frame = prev_state.is_none();

        self.prev_state = prev_state.unwrap_or_default();
        self.available_rect = ui.available_rect_before_wrap();

        ui.ctx().check_for_id_clash(self.id, self.available_rect, "VideoGrid");

        ui.allocate_ui_at_rect(self.available_rect, |ui| {
            ui.set_visible(!is_first_frame);

            let mut ctx = VideoGridContext { layout: &mut self, ui };
            let res = grid(&mut ctx);

            // Save the new state
            if self.curr_state != self.prev_state {
                self.curr_state.clone().store(ui.ctx(), self.id);
                ui.ctx().request_repaint();
            }

            res
        })
    }

    fn next_frame_rect(&mut self) -> egui::Rect {
        assert!(self.available_rect.is_finite());
        assert!(self.spacing <= self.min_video_size.x);

        // increment the amount of videos for the next frame
        self.curr_state.num_videos += 1;

        let num_videos = self.prev_state.num_videos;
        if num_videos == 0 {
            return egui::Rect::NOTHING;
        }

        let max_columns = self.max_columns;
        let minimum_size = self.min_video_size;
        let available_size = self.available_rect.size();

        let calc_min_width =
            |columns: u32| columns as f32 * minimum_size.x + (columns - 1) as f32 * self.spacing;

        let total_columns = {
            let mut est = (available_size.x / minimum_size.x) as u32 + 1;
            if available_size.x < calc_min_width(est) {
                est -= 1;
            }
            cmp::max(1, cmp::min(est, max_columns))
        };

        let aspect_ratio = minimum_size.x / minimum_size.y;
        let remaining_width = available_size.x - calc_min_width(total_columns);
        let w = minimum_size.x + remaining_width / total_columns as f32;
        let h = w / aspect_ratio;

        let x_index = self.video_index % total_columns;
        let y_index = self.video_index / total_columns;

        let x = {
            let mut x = x_index as f32 * (w + self.spacing);

            // vertically center the last row
            let total_rows = num_videos / total_columns + 1;
            if (y_index + 1) == total_rows {
                let nb_items = num_videos - (total_rows - 1) * total_columns; // nb. of items on the last row
                x += (total_columns - nb_items) as f32 * (w + self.spacing) / 2.0;
            }

            x
        };
        let y = y_index as f32 * (h + self.spacing);

        let min = egui::pos2(x, y) + self.available_rect.left_top().to_vec2();
        let max = egui::pos2(w, h) + min.to_vec2();

        self.video_index += 1;

        egui::Rect { min, max }
    }
}

#[allow(dead_code)]
impl VideoGrid {
    pub fn min_video_size(mut self, min_video_size: egui::Vec2) -> Self {
        self.min_video_size = min_video_size;
        self
    }

    pub fn max_columns(mut self, max_columns: u32) -> Self {
        self.max_columns = max_columns;
        self
    }

    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }
}

pub struct VideoGridContext<'a> {
    layout: &'a mut VideoGrid,
    ui: &'a mut egui::Ui,
}

impl<'a> VideoGridContext<'a> {
    pub fn video_frame(&mut self, add_contents: impl FnOnce(&mut egui::Ui)) -> egui::Response {
        let frame_rect = self.layout.next_frame_rect();

        if self.ui.is_visible() {
            let mut child_ui = self.ui.child_ui(frame_rect, egui::Layout::default(), None);
            add_contents(&mut child_ui);
        }

        self.ui.allocate_rect(frame_rect, egui::Sense::hover())
    }
}
