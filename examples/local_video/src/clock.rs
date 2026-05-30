use anyhow::Result;
use clap::Parser;
use eframe::egui;
use eframe::wgpu;
use eframe::Renderer;
use egui_wgpu as egui_wgpu_backend;

mod clock_render;

use clock_render::ClockPaintCallback;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Display a low-latency millisecond clock with a millisecond grid",
    long_about = None
)]
struct Args {
    /// Start in borderless fullscreen
    #[arg(long, default_value_t = false)]
    fullscreen: bool,

    /// Keep the clock above normal windows
    #[arg(long, default_value_t = false)]
    always_on_top: bool,

    /// Enable vsync for tear-free output at the cost of extra queuing
    #[arg(long, default_value_t = false)]
    vsync: bool,
}

struct ClockApp;

impl eframe::App for ClockApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            ui.painter().rect_filled(ui.max_rect(), 0, egui::Color32::BLACK);

            let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
            let cb = egui_wgpu_backend::Callback::new_paint_callback(
                rect,
                ClockPaintCallback::for_rect(ctx, rect),
            );
            ui.painter().add(cb);
        });
    }
}

fn native_options(args: &Args) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("LiveKit Clock")
        .with_inner_size([960.0, 420.0])
        .with_min_inner_size([480.0, 210.0])
        .with_fullscreen(args.fullscreen);

    if args.always_on_top {
        viewport = viewport.with_always_on_top();
    }

    let mut wgpu_options = egui_wgpu_backend::WgpuConfiguration::default();
    wgpu_options.present_mode =
        if args.vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
    wgpu_options.desired_maximum_frame_latency = Some(1);

    eframe::NativeOptions {
        viewport,
        vsync: args.vsync,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        renderer: Renderer::Wgpu,
        wgpu_options,
        persist_window: false,
        dithering: false,
        ..Default::default()
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    eframe::run_native(
        "LiveKit Clock",
        native_options(&args),
        Box::new(|_| Ok(Box::new(ClockApp))),
    )?;
    Ok(())
}
