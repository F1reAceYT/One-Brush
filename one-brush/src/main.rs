#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rosin::{prelude::*, widgets::*};

const TOOLS: [(&str, &str); 14] = [
    ("Sel", "Select"), ("Sub", "Subselect"), ("Line", "Line"), ("Pen", "Pen"),
    ("Text", "Text"), ("Oval", "Oval"), ("Rect", "Rectangle"), ("Penc", "Pencil"),
    ("Brush", "Brush"), ("Ink", "Ink Bottle"), ("Fill", "Paint Bucket"),
    ("Pick", "Eyedropper"), ("Erase", "Eraser"), ("Zoom", "Zoom"),
];

struct State {
    style: Stylesheet,
    selected_tool: Var<u32>,
    current_frame: Var<u32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            style: stylesheet!("src/style.css"),
            selected_tool: Var::new(0),
            current_frame: Var::new(1),
        }
    }
}

fn menu_bar(ui: &mut Ui<State, WindowHandle>) {
    ui.node().id(id!()).classes("menu-bar").children(|ui| {
        label(ui, id!(), "ONE BRUSH").classes("wordmark");
        for (i, item) in ["File", "Edit", "View", "Insert", "Modify", "Control", "Window", "Help"].into_iter().enumerate() {
            label(ui, id!(i as u64), item).classes("menu-item");
        }
        label(ui, id!(), "Scene 1").classes("scene-name");
    });
}

fn tool_strip(state: &State, ui: &mut Ui<State, WindowHandle>) {
    let selected = state.selected_tool.get();
    ui.node().id(id!()).classes("tool-strip panel").children(|ui| {
        label(ui, id!(), "TOOLS").classes("panel-title");
        ui.node().id(id!()).classes("tool-grid").children(|ui| {
            for column in 0..2_usize {
                ui.node().id(id!(column as u64)).classes("tool-column").children(|ui| {
                    for index in column * 7..column * 7 + 7 {
                        let classes = if index as u32 == selected { "tool-button selected" } else { "tool-button" };
                        button(ui, id!(index as u64), TOOLS[index].0, move |state, _| state.selected_tool.set(index as u32)).classes(classes);
                    }
                });
            }
        });
        label(ui, id!(), "ACTIVE TOOL").classes("section-label");
        label(ui, id!(), TOOLS[selected as usize].1).classes("active-tool");
        ui.node().id(id!()).classes("color-control").children(|ui| {
            ui.node().id(id!()).classes("stroke-chip");
            ui.node().id(id!()).classes("fill-chip");
        });
    });
}

fn stage(ui: &mut Ui<State, WindowHandle>) {
    ui.node().id(id!()).classes("workspace").children(|ui| {
        ui.node().id(id!()).classes("stage-wrap").children(|ui| {
            ui.node().id(id!()).classes("stage").children(|ui| {
                label(ui, id!(), "STAGE").classes("stage-label");
                label(ui, id!(), "800 × 450").classes("stage-size");
            });
        });
        ui.node().id(id!()).classes("status-bar").children(|ui| {
            label(ui, id!(), "100%").classes("status-zoom");
            label(ui, id!(), "RGB").classes("status-item");
            label(ui, id!(), "24 fps").classes("status-item");
        });
    });
}

fn properties(ui: &mut Ui<State, WindowHandle>) {
    ui.node().id(id!()).classes("properties panel").children(|ui| {
        label(ui, id!(), "COLOR").classes("panel-title");
        ui.node().id(id!()).classes("swatches").children(|ui| {
            for (i, class) in ["swatch black", "swatch white", "swatch red", "swatch orange", "swatch yellow", "swatch green", "swatch cyan", "swatch blue", "swatch violet", "swatch pink"].into_iter().enumerate() {
                ui.node().id(id!(i as u64)).classes(class);
            }
        });
        label(ui, id!(), "OBJECT").classes("panel-title property-title");
        for (i, (name, value)) in [("X", "0.0"), ("Y", "0.0"), ("W", "800.0"), ("H", "450.0"), ("Stroke", "1.0 px")].into_iter().enumerate() {
            ui.node().id(id!(i as u64)).classes("property-row").children(|ui| {
                label(ui, id!(i as u64, 0_u64), name).classes("property-name");
                label(ui, id!(i as u64, 1_u64), value).classes("property-value");
            });
        }
    });
}

fn frame_cell(ui: &mut Ui<State, WindowHandle>, frame: u32, layer: u64, selected: u32) {
    let classes = if frame == selected { "frame-cell current" } else if frame == 1 { "frame-cell keyframe" } else { "frame-cell" };
    ui.node().id(id!(layer, frame as u64)).classes(classes).event(On::PointerDown, move |state, _| state.current_frame.set(frame));
}

fn timeline(state: &State, ui: &mut Ui<State, WindowHandle>) {
    let selected = state.current_frame.get();
    ui.node().id(id!()).classes("timeline panel").children(|ui| {
        ui.node().id(id!()).classes("timeline-head").children(|ui| {
            label(ui, id!(), "TIMELINE").classes("panel-title timeline-title");
            label(ui, id!(), format!("Frame {selected} / 24")).classes("frame-readout");
        });
        ui.node().id(id!()).classes("timeline-body").children(|ui| {
            ui.node().id(id!()).classes("layer-list").children(|ui| {
                label(ui, id!(), "LAYERS").classes("layer-header");
                for (i, name) in ["Ink", "Sketch", "Background"].into_iter().enumerate() {
                    ui.node().id(id!(i as u64)).classes("layer-row").children(|ui| {
                        label(ui, id!(i as u64, 0_u64), "O").classes("layer-toggle");
                        label(ui, id!(i as u64, 1_u64), name).classes("layer-name");
                    });
                }
                ui.node().id(id!()).classes("layer-actions").children(|ui| {
                    label(ui, id!(), "+ Layer").classes("layer-action");
                    label(ui, id!(), "Remove").classes("layer-action muted");
                });
            });
            ui.node().id(id!()).classes("frame-area").children(|ui| {
                ui.node().id(id!()).classes("frame-ruler").children(|ui| {
                    for frame in 1..=24_u32 {
                        let text = if frame == 1 || frame % 5 == 0 { frame.to_string() } else { String::new() };
                        label(ui, id!(frame as u64), text).classes("ruler-cell");
                    }
                });
                for layer in 0..3_u64 {
                    ui.node().id(id!(layer)).classes("frame-row").children(|ui| {
                        for frame in 1..=24_u32 { frame_cell(ui, frame, layer, selected); }
                    });
                }
            });
        });
    });
}

fn main_view(state: &State, ui: &mut Ui<State, WindowHandle>) {
    ui.node().id(id!()).style_sheet(&state.style).classes("root").children(|ui| {
        menu_bar(ui);
        ui.node().id(id!()).classes("editor").children(|ui| {
            tool_strip(state, ui);
            stage(ui);
            properties(ui);
        });
        timeline(state, ui);
    });
}

fn main() {
    env_logger::init();
    let window = WindowDesc::new(callback!(main_view)).title("One Brush").size(1280, 820).min_size(1100, 700);
    AppLauncher::new(window).run(State::default(), TranslationMap::default()).expect("failed to launch One Brush");
}
