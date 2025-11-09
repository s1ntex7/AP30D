// PHASE 1 PRODUCTION V3 - Multi-Process Architecture
//
// SOLUTION: Multi-process architecture with file-based IPC (not threads!)
//
// WHY NOT THREADS:
// ❌ std::thread::spawn() per monitor → winit panics "EventLoop must be on main thread"
// ❌ Single spanning window → Windows API limitation (transparent windows can't span monitors)
//
// CORRECT SOLUTION:
// ✅ Parent process: captures monitors → saves to temp files → spawns child processes
// ✅ Child process per monitor: loads from temp → runs eframe::run_native() on main thread
// ✅ State synchronization: file-based IPC via JSON in %TEMP%\egui_overlay\
//
// ARCHITECTURE:
//
//   Parent Process (overlay_egui.exe)
//       │
//       ├─> Capture all monitors
//       ├─> Save to %TEMP%\egui_overlay\
//       │   ├── monitors.json (monitor metadata)
//       │   ├── vdb.json (virtual desktop bounds)
//       │   ├── state.json (shared state)
//       │   ├── monitor_0.png (screenshot)
//       │   └── monitor_1.png (screenshot)
//       │
//       ├─> Spawn: overlay_egui.exe --monitor 0
//       └─> Spawn: overlay_egui.exe --monitor 1
//               │
//               └─> Each child runs eframe::run_native() on ITS main thread ✅

use eframe::egui;
use xcap::{Monitor, image}; // xcap re-exports image crate
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

const MIN_SELECTION_SIZE: f32 = 5.0;

/// Monitor metadata (serializable for IPC)
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CapturedMonitor {
    image_path: PathBuf,  // Path to saved PNG screenshot
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f64,
    screen_index: usize,
}

/// Shared state synchronized across processes via file
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct SharedState {
    /// Selection rectangle in virtual desktop coordinates [min_x, min_y, max_x, max_y]
    selection_rect: Option<[f32; 4]>,
    /// Whether user is currently dragging
    is_dragging: bool,
    /// Drag start point in virtual desktop coordinates [x, y]
    drag_start: Option<[f32; 2]>,
    /// Whether to close all windows
    should_close: bool,
}

impl SharedState {
    /// Convert array to egui::Rect (for rendering)
    fn to_rect(&self) -> Option<egui::Rect> {
        self.selection_rect.map(|[min_x, min_y, max_x, max_y]| {
            egui::Rect::from_min_max(
                egui::pos2(min_x, min_y),
                egui::pos2(max_x, max_y),
            )
        })
    }

    /// Set selection from egui::Rect (for writing)
    fn set_rect(&mut self, rect: Option<egui::Rect>) {
        self.selection_rect = rect.map(|r| [r.min.x, r.min.y, r.max.x, r.max.y]);
    }

    /// Get drag start as egui::Pos2
    fn drag_start_pos(&self) -> Option<egui::Pos2> {
        self.drag_start.map(|[x, y]| egui::pos2(x, y))
    }

    /// Set drag start from egui::Pos2
    fn set_drag_start(&mut self, pos: Option<egui::Pos2>) {
        self.drag_start = pos.map(|p| [p.x, p.y]);
    }
}

struct OverlayApp {
    monitor: CapturedMonitor,
    texture: Option<egui::TextureHandle>,
    texture_width: u32,   // Actual texture width after GPU downscale
    texture_height: u32,  // Actual texture height after GPU downscale
    state_file: PathBuf,
    virtual_desktop_bounds: egui::Rect,
    local_cursor_pos: Option<egui::Pos2>,
    last_state_check: Instant,
}

impl OverlayApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        monitor: CapturedMonitor,
        state_file: PathBuf,
        virtual_desktop_bounds: egui::Rect,
    ) -> Self {
        // Load screenshot from PNG file
        let texture = match image::open(&monitor.image_path) {
            Ok(img) => {
                let mut rgba = img.to_rgba8();

                // Get logical dimensions (what egui expects)
                let logical_width = monitor.width;
                let logical_height = monitor.height;
                let mut physical_width = rgba.width();
                let mut physical_height = rgba.height();

                // CRITICAL FIX: GPU texture size limit (most GPUs max 2048×2048)
                const MAX_TEXTURE_SIZE: u32 = 2048;

                if physical_width > MAX_TEXTURE_SIZE || physical_height > MAX_TEXTURE_SIZE {
                    // Calculate scale to fit within GPU limits
                    let scale = (MAX_TEXTURE_SIZE as f32 / physical_width.max(physical_height) as f32).min(1.0);
                    let target_width = (physical_width as f32 * scale) as u32;
                    let target_height = (physical_height as f32 * scale) as u32;

                    tracing::warn!(
                        "Monitor {}: Texture size {}×{} exceeds GPU limit ({}). Downscaling to {}×{}",
                        monitor.screen_index,
                        physical_width,
                        physical_height,
                        MAX_TEXTURE_SIZE,
                        target_width,
                        target_height
                    );

                    rgba = image::imageops::resize(
                        &rgba,
                        target_width,
                        target_height,
                        image::imageops::FilterType::Lanczos3,
                    );

                    physical_width = target_width;
                    physical_height = target_height;
                }

                // CRITICAL: Final texture size MUST NOT exceed GPU limit
                // Calculate final size preserving aspect ratio
                let (final_width, final_height) = if logical_width > MAX_TEXTURE_SIZE || logical_height > MAX_TEXTURE_SIZE {
                    // Scale down proportionally to fit within GPU limits
                    let scale = (MAX_TEXTURE_SIZE as f32 / logical_width.max(logical_height) as f32).min(1.0);
                    let scaled_width = (logical_width as f32 * scale) as u32;
                    let scaled_height = (logical_height as f32 * scale) as u32;

                    tracing::info!(
                        "Monitor {}: Scaling texture to fit GPU limit: {}×{} → {}×{} (scale: {:.2})",
                        monitor.screen_index,
                        logical_width,
                        logical_height,
                        scaled_width,
                        scaled_height,
                        scale
                    );

                    (scaled_width, scaled_height)
                } else {
                    (logical_width, logical_height)
                };

                // Resize to final dimensions if needed
                if physical_width != final_width || physical_height != final_height {
                    tracing::info!(
                        "Monitor {}: Resizing texture from physical {}×{} to final {}×{} (DPI scale: {:.2})",
                        monitor.screen_index,
                        physical_width,
                        physical_height,
                        final_width,
                        final_height,
                        monitor.scale_factor
                    );

                    rgba = image::imageops::resize(
                        &rgba,
                        final_width,
                        final_height,
                        image::imageops::FilterType::Lanczos3, // High-quality downscaling
                    );
                }

                // Convert to egui ColorImage with final size (guaranteed ≤ 2048)
                let pixels: Vec<egui::Color32> = rgba.pixels().map(|p| {
                    egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])
                }).collect();

                let color_image = egui::ColorImage {
                    size: [final_width as usize, final_height as usize],
                    pixels,
                };

                Some(cc.egui_ctx.load_texture(
                    format!("monitor_{}", monitor.screen_index),
                    color_image,
                    egui::TextureOptions::LINEAR
                ))
            }
            Err(e) => {
                tracing::error!("Failed to load screenshot from {}: {}",
                    monitor.image_path.display(), e);
                None
            }
        };

        tracing::info!(
            "Child process: overlay window created for monitor {} at ({}, {}) size {}×{}",
            monitor.screen_index, monitor.x, monitor.y, monitor.width, monitor.height
        );

        // Extract actual texture dimensions (after GPU downscale)
        let (texture_width, texture_height) = if let Some(tex) = &texture {
            let [w, h] = tex.size();
            tracing::info!("📐 Texture actual size: {}×{} (may differ from monitor size due to GPU limits)", w, h);
            (w as u32, h as u32)
        } else {
            tracing::warn!("⚠️ No texture loaded, using monitor dimensions as fallback");
            (monitor.width, monitor.height)
        };

        // NOTE: Window is already created with correct size in run_monitor_overlay()
        // No need to resize here anymore

        Self {
            monitor,
            texture,
            texture_width,
            texture_height,
            state_file,
            virtual_desktop_bounds,
            local_cursor_pos: None,
            last_state_check: Instant::now(),
        }
    }

    /// Read shared state from file
    fn read_state(&self) -> SharedState {
        fs::read_to_string(&self.state_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Write shared state to file
    fn write_state(&self, state: &SharedState) {
        if let Ok(json) = serde_json::to_string(state) {
            let _ = fs::write(&self.state_file, json);
        }
    }

    /// Convert local window coordinates to virtual desktop coordinates
    fn window_to_virtual(&self, window_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            window_pos.x + self.monitor.x as f32,
            window_pos.y + self.monitor.y as f32,
        )
    }

    /// Convert virtual desktop coordinates to local window coordinates
    fn virtual_to_window(&self, virtual_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            virtual_pos.x - self.monitor.x as f32,
            virtual_pos.y - self.monitor.y as f32,
        )
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        // Get cursor position relative to this window
        if let Some(cursor_pos) = ctx.pointer_latest_pos() {
            self.local_cursor_pos = Some(self.window_to_virtual(cursor_pos));
        } else {
            self.local_cursor_pos = None;
        }

        // Read current state
        let mut state = self.read_state();

        // Handle mouse button press (start drag)
        if ctx.input(|i| i.pointer.primary_pressed()) {
            if let Some(pos) = self.local_cursor_pos {
                state.is_dragging = true;
                state.set_drag_start(Some(pos));
                state.set_rect(Some(egui::Rect::from_min_max(pos, pos)));
                self.write_state(&state);
                tracing::info!("Started drag at virtual pos: {:?}", pos);
            }
        }

        // Handle mouse drag (update selection)
        if state.is_dragging {
            if let (Some(start), Some(current)) = (state.drag_start_pos(), self.local_cursor_pos) {
                let min_x = start.x.min(current.x);
                let min_y = start.y.min(current.y);
                let max_x = start.x.max(current.x);
                let max_y = start.y.max(current.y);

                let width = max_x - min_x;
                let height = max_y - min_y;

                if width >= MIN_SELECTION_SIZE && height >= MIN_SELECTION_SIZE {
                    state.set_rect(Some(egui::Rect::from_min_max(
                        egui::pos2(min_x, min_y),
                        egui::pos2(max_x, max_y)
                    )));
                } else {
                    state.set_rect(None);
                }
                self.write_state(&state);
            }
        }

        // Handle mouse button release (end drag)
        if ctx.input(|i| i.pointer.primary_released()) {
            if state.is_dragging {
                state.is_dragging = false;
                self.write_state(&state);
                if let Some(rect) = state.to_rect() {
                    tracing::info!(
                        "Selection complete: ({:.0},{:.0}) → ({:.0},{:.0}) [{}×{}]",
                        rect.min.x, rect.min.y,
                        rect.max.x, rect.max.y,
                        rect.width(), rect.height()
                    );
                }
            }
        }

        // Handle Escape key (cancel and close)
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            tracing::info!("Escape pressed, signaling all windows to close");
            state.should_close = true;
            self.write_state(&state);
        }

        // TODO Phase 2: Handle Enter/Ctrl+C to save selection
    }

    fn render_overlay(&self, ui: &mut egui::Ui) {
        let painter = ui.painter();

        // LAYER 0: Input capture region (nearly invisible)
        // Ensures window receives mouse events and prevents click-through bug
        let full_rect = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(self.texture_width as f32, self.texture_height as f32),
        );
        painter.rect_filled(
            full_rect,
            0.0,
            egui::Color32::from_rgba_premultiplied(0, 0, 0, 3), // ~1% opacity
        );

        // LAYER 1: Render monitor screenshot at (0,0) in window coordinates
        if let Some(texture) = &self.texture {
            let rect = full_rect; // Reuse full_rect from LAYER 0
            painter.image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        // DEBUG OVERLAY - Large text showing monitor identity and loaded image
        let texture_size = if let Some(tex) = &self.texture {
            format!("{}×{}", tex.size()[0], tex.size()[1])
        } else {
            "NO TEXTURE".to_string()
        };

        let image_filename = self.monitor.image_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("UNKNOWN");

        let debug_text = format!(
            "🔍 DEBUG MONITOR {}\nPos: ({}, {})\nSize: {}×{}\nImage: {}\nTexture: {}",
            self.monitor.screen_index,
            self.monitor.x,
            self.monitor.y,
            self.monitor.width,
            self.monitor.height,
            image_filename,
            texture_size
        );

        // Background for debug text
        let debug_bg_rect = egui::Rect::from_min_size(
            egui::pos2(10.0, 10.0),
            egui::vec2(400.0, 160.0),
        );
        painter.rect_filled(
            debug_bg_rect,
            4.0,
            egui::Color32::from_rgba_premultiplied(0, 0, 0, 200),
        );

        // Debug text in bright red
        painter.text(
            egui::pos2(20.0, 20.0),
            egui::Align2::LEFT_TOP,
            debug_text,
            egui::FontId::proportional(24.0),
            egui::Color32::from_rgb(255, 50, 50),
        );

        // LAYER 2: Dark overlay with selection cutout
        let state = self.read_state();

        // Get selection rect in virtual coordinates, convert to window coordinates
        let selection_rect_window = state.to_rect().map(|rect| {
            egui::Rect::from_min_max(
                self.virtual_to_window(rect.min),
                self.virtual_to_window(rect.max),
            )
        });

        self.render_dark_overlay_with_cutout(
            painter,
            selection_rect_window,
            egui::Color32::from_rgba_premultiplied(0, 0, 0, 128),
        );

        // LAYER 3: Selection border and info
        if let Some(selection_window) = selection_rect_window {
            let window_rect = egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(self.texture_width as f32, self.texture_height as f32),
            );

            let intersection = window_rect.intersect(selection_window);
            if !intersection.is_negative() {
                // Draw selection border
                painter.rect_stroke(
                    intersection,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(59, 130, 246)),
                );

                // Draw selection info (only on primary monitor)
                if self.monitor.screen_index == 0 {
                    if let Some(full_rect) = state.to_rect() {
                        let size_text = format!(
                            "{} × {}",
                            full_rect.width() as i32,
                            full_rect.height() as i32
                        );

                        let label_pos_virtual = egui::pos2(
                            full_rect.min.x,
                            full_rect.min.y - 25.0,
                        );
                        let label_pos = self.virtual_to_window(label_pos_virtual);

                        // Background for label
                        let label_galley = painter.layout_no_wrap(
                            size_text.clone(),
                            egui::FontId::proportional(14.0),
                            egui::Color32::WHITE
                        );
                        let label_rect = egui::Rect::from_min_size(
                            label_pos,
                            label_galley.size() + egui::vec2(8.0, 4.0)
                        );
                        painter.rect_filled(
                            label_rect,
                            2.0,
                            egui::Color32::from_rgb(59, 130, 246)
                        );

                        // Label text
                        painter.text(
                            egui::pos2(label_pos.x + 4.0, label_pos.y + 2.0),
                            egui::Align2::LEFT_TOP,
                            size_text,
                            egui::FontId::proportional(14.0),
                            egui::Color32::WHITE
                        );
                    }
                }
            }
        }

        // LAYER 4: Instructions (only on primary monitor when no selection)
        if self.monitor.screen_index == 0 && state.selection_rect.is_none() {
            let instructions = "Click and drag to select area (minimum 5px) • ESC to cancel";
            painter.text(
                egui::pos2(self.texture_width as f32 / 2.0, 20.0),
                egui::Align2::CENTER_TOP,
                instructions,
                egui::FontId::proportional(18.0),
                egui::Color32::WHITE,
            );
        }
    }

    /// Render dark overlay EXCLUDING selection rectangle
    fn render_dark_overlay_with_cutout(
        &self,
        painter: &egui::Painter,
        cutout: Option<egui::Rect>,
        color: egui::Color32,
    ) {
        let full_rect = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(self.texture_width as f32, self.texture_height as f32),
        );

        if let Some(cutout) = cutout {
            let cutout = cutout.intersect(full_rect);
            if cutout.is_negative() {
                return;
            }

            // TOP
            if cutout.min.y > full_rect.min.y {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        full_rect.min,
                        egui::pos2(full_rect.max.x, cutout.min.y),
                    ),
                    0.0,
                    color,
                );
            }

            // BOTTOM
            if cutout.max.y < full_rect.max.y {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(full_rect.min.x, cutout.max.y),
                        full_rect.max,
                    ),
                    0.0,
                    color,
                );
            }

            // LEFT
            if cutout.min.x > full_rect.min.x {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(full_rect.min.x, cutout.min.y),
                        egui::pos2(cutout.min.x, cutout.max.y),
                    ),
                    0.0,
                    color,
                );
            }

            // RIGHT
            if cutout.max.x < full_rect.max.x {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(cutout.max.x, cutout.min.y),
                        egui::pos2(full_rect.max.x, cutout.max.y),
                    ),
                    0.0,
                    color,
                );
            }
        } else {
            painter.rect_filled(full_rect, 0.0, color);
        }
    }
}

impl eframe::App for OverlayApp {
    /// CRITICAL: Make background ALMOST transparent (not fully)
    /// Fully transparent windows may trigger WS_EX_TRANSPARENT behavior
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.01]  // 1% opacity - invisible but captures input
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ESC to cancel (professional screenshot tool behavior)
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            tracing::info!("🚫 ESC pressed - cancelling screenshot, closing all overlays");
            // Signal all monitors to close
            let mut state = self.read_state();
            state.should_close = true;
            self.write_state(&state);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Poll for close signal every 100ms
        if self.last_state_check.elapsed() > Duration::from_millis(100) {
            if self.read_state().should_close {
                tracing::info!("Received close signal, shutting down");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            self.last_state_check = Instant::now();
        }

        // Handle input
        self.handle_input(ctx);

        // Render overlay
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.render_overlay(ui);
            });

        // Request continuous repaint
        ctx.request_repaint();
    }
}

/// Helper struct to store monitor metadata before processing
#[derive(Clone)]
struct MonitorMetadata {
    monitor: Monitor,
    index: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
}

fn capture_all_monitors() -> Vec<CapturedMonitor> {
    let mut monitors = match Monitor::all() {
        Ok(monitors) => monitors,
        Err(e) => {
            tracing::error!("Failed to get monitors: {}", e);
            return Vec::new();
        }
    };

    tracing::info!("Found {} monitor(s) - RAW ORDER FROM Monitor::all():", monitors.len());

    // Log RAW order from Monitor::all() BEFORE sorting
    for (idx, monitor) in monitors.iter().enumerate() {
        tracing::info!(
            "  RAW[{}]: pos=({}, {}), size={}×{}, scale={:.2}",
            idx,
            monitor.x().unwrap_or(0),
            monitor.y().unwrap_or(0),
            monitor.width().unwrap_or(0),
            monitor.height().unwrap_or(0),
            monitor.scale_factor().unwrap_or(1.0)
        );
    }

    // CRITICAL FIX: Sort monitors by X position (left to right)
    monitors.sort_by_key(|m| m.x().unwrap_or(0));

    tracing::info!("Monitors AFTER sorting by X position:");
    for (idx, monitor) in monitors.iter().enumerate() {
        tracing::info!(
            "  SORTED[{}]: pos=({}, {}), size={}×{}, scale={:.2}",
            idx,
            monitor.x().unwrap_or(0),
            monitor.y().unwrap_or(0),
            monitor.width().unwrap_or(0),
            monitor.height().unwrap_or(0),
            monitor.scale_factor().unwrap_or(1.0)
        );
    }

    // TWO-PASS APPROACH:
    // Pass 1: Collect all monitor metadata (needed for cropping calculations)
    let monitor_metadata: Vec<MonitorMetadata> = monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            MonitorMetadata {
                x: monitor.x().unwrap_or(0),
                y: monitor.y().unwrap_or(0),
                width: monitor.width().unwrap_or(1920),
                height: monitor.height().unwrap_or(1080),
                scale: monitor.scale_factor().unwrap_or(1.0) as f64,
                index,
                monitor,
            }
        })
        .collect();

    let temp_dir = std::env::temp_dir().join("egui_overlay");
    fs::create_dir_all(&temp_dir).ok();

    // Detect virtual desktop DPI scale (usually primary monitor at x=0, y=0)
    let vd_scale = monitor_metadata.iter()
        .find(|m| m.x == 0 && m.y == 0)
        .map(|m| m.scale)
        .unwrap_or(1.0);
    tracing::info!("🌍 Virtual Desktop DPI scale detected: {:.2}", vd_scale);

    // Pass 2: Capture and crop each monitor
    monitor_metadata
        .into_iter()
        .filter_map(|meta| {
            let index = meta.index;
            let mon_x = meta.x;
            let mon_y = meta.y;
            let mon_width = meta.width;
            let mon_height = meta.height;
            let mon_scale = meta.scale;

            tracing::info!(
                "Monitor {} metadata: logical {}×{} @ ({}, {}), scale {:.2}",
                index, mon_width, mon_height, mon_x, mon_y, mon_scale
            );

            match meta.monitor.capture_image() {
                Ok(rgba_image) => {
                    let physical_width = rgba_image.width();
                    let physical_height = rgba_image.height();
                    let expected_physical_width = (mon_width as f64 * mon_scale) as u32;
                    let expected_physical_height = (mon_height as f64 * mon_scale) as u32;

                    // Log RAW capture dimensions
                    tracing::info!(
                        "Monitor {}: RAW capture {}×{} (expected {}×{} based on DPI)",
                        index,
                        physical_width,
                        physical_height,
                        expected_physical_width,
                        expected_physical_height
                    );

                    // CRITICAL: Detect virtual desktop capture and crop to this monitor
                    let scale_x = physical_width as f64 / expected_physical_width as f64;
                    let scale_y = physical_height as f64 / expected_physical_height as f64;

                    // DIAGNOSTIC: Log detection math
                    tracing::warn!(
                        "🔍 Monitor {}: Scale check: {:.3}×{:.3} | DPI: {:.2} | Threshold: >1.1",
                        index, scale_x, scale_y, mon_scale
                    );

                    // Fixed threshold: Any capture >10% larger indicates virtual desktop
                    let is_virtual_desktop = scale_x > 1.1 || scale_y > 1.1;

                    let final_image = if is_virtual_desktop {
                        tracing::warn!(
                            "Monitor {}: DIMENSION MISMATCH! Captured {}×{} but expected {}×{}",
                            index, physical_width, physical_height,
                            expected_physical_width, expected_physical_height
                        );
                        tracing::warn!(
                            "Monitor {}: Detected VIRTUAL DESKTOP capture! Scale {}×{} >> DPI scale {:.2}",
                            index, scale_x, scale_y, mon_scale
                        );

                        // Save RAW virtual desktop for diagnostics
                        let raw_path = temp_dir.join(format!("monitor_{}_RAW_PHYSICAL.png", index));
                        if let Err(e) = rgba_image.save(&raw_path) {
                            tracing::warn!("Failed to save RAW screenshot: {}", e);
                        } else {
                            tracing::info!("Saved RAW virtual desktop to: {}", raw_path.display());
                        }

                        // Calculate crop bounds - use VIRTUAL DESKTOP scale, not individual monitor scale!
                        // Virtual desktop is rendered at primary monitor's DPI
                        let crop_x = (mon_x as f64 * vd_scale) as u32;
                        let crop_y = (mon_y as f64 * vd_scale) as u32;
                        let crop_w = (mon_width as f64 * vd_scale) as u32;
                        let crop_h = (mon_height as f64 * vd_scale) as u32;

                        // Validate crop bounds
                        if crop_x + crop_w <= physical_width && crop_y + crop_h <= physical_height {
                            tracing::info!(
                                "Monitor {}: ✅ Cropping virtual desktop at ({}, {}) size {}×{}",
                                index, crop_x, crop_y, crop_w, crop_h
                            );

                            // Crop the image
                            let cropped = image::imageops::crop_imm(&rgba_image, crop_x, crop_y, crop_w, crop_h);
                            cropped.to_image()
                        } else {
                            tracing::error!(
                                "Monitor {}: ❌ Invalid crop bounds! ({}, {}) size {}×{} exceeds {}×{}",
                                index, crop_x, crop_y, crop_w, crop_h, physical_width, physical_height
                            );
                            tracing::warn!("Monitor {}: Using uncropped image as fallback", index);
                            rgba_image
                        }
                    } else {
                        // No virtual desktop detected - use original image
                        if physical_width != expected_physical_width || physical_height != expected_physical_height {
                            tracing::info!(
                                "Monitor {}: Minor dimension difference (not virtual desktop): {}×{} vs {}×{}",
                                index, physical_width, physical_height,
                                expected_physical_width, expected_physical_height
                            );
                        }
                        rgba_image
                    };

                    // Save final (potentially cropped) image
                    let image_path = temp_dir.join(format!("monitor_{}.png", index));
                    if final_image.save(&image_path).is_err() {
                        tracing::warn!("Failed to save screenshot for monitor {}", index);
                        return None;
                    }

                    tracing::info!(
                        "Monitor {}: ✅ Saved {} screenshot ({}×{}) to {}",
                        index,
                        if is_virtual_desktop { "CROPPED" } else { "direct" },
                        final_image.width(), final_image.height(),
                        image_path.display()
                    );

                    Some(CapturedMonitor {
                        image_path,
                        x: mon_x,
                        y: mon_y,
                        width: mon_width,
                        height: mon_height,
                        scale_factor: mon_scale,
                        screen_index: index,
                    })
                }
                Err(e) => {
                    tracing::warn!("Failed to capture monitor {}: {}", index, e);
                    None
                }
            }
        })
        .collect()
}

/// Calculate final texture size after GPU downscaling
/// Returns (width, height) that will be used for the actual texture
fn calculate_final_texture_size(monitor: &CapturedMonitor) -> (u32, u32) {
    const MAX_TEXTURE_SIZE: u32 = 2048;

    let logical_width = monitor.width;
    let logical_height = monitor.height;

    if logical_width > MAX_TEXTURE_SIZE || logical_height > MAX_TEXTURE_SIZE {
        // Scale down proportionally to fit within GPU limits
        let scale = (MAX_TEXTURE_SIZE as f32 / logical_width.max(logical_height) as f32).min(1.0);
        let scaled_width = (logical_width as f32 * scale) as u32;
        let scaled_height = (logical_height as f32 * scale) as u32;
        (scaled_width, scaled_height)
    } else {
        (logical_width, logical_height)
    }
}

fn calculate_virtual_desktop_bounds(monitors: &[CapturedMonitor]) -> egui::Rect {
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for monitor in monitors {
        min_x = min_x.min(monitor.x);
        min_y = min_y.min(monitor.y);
        max_x = max_x.max(monitor.x + monitor.width as i32);
        max_y = max_y.max(monitor.y + monitor.height as i32);
    }

    egui::Rect::from_min_max(
        egui::pos2(min_x as f32, min_y as f32),
        egui::pos2(max_x as f32, max_y as f32),
    )
}

/// Child process: run overlay for specific monitor
fn run_monitor_overlay(monitor_index: usize) -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = std::env::temp_dir().join("egui_overlay");

    // Load monitor data from temp files
    let monitors_json = fs::read_to_string(temp_dir.join("monitors.json"))?;
    let monitors: Vec<CapturedMonitor> = serde_json::from_str(&monitors_json)?;

    let vdb_json = fs::read_to_string(temp_dir.join("vdb.json"))?;
    let virtual_desktop_bounds: [f32; 4] = serde_json::from_str(&vdb_json)?;
    let vdb = egui::Rect::from_min_max(
        egui::pos2(virtual_desktop_bounds[0], virtual_desktop_bounds[1]),
        egui::pos2(virtual_desktop_bounds[2], virtual_desktop_bounds[3]),
    );

    let monitor = monitors.get(monitor_index)
        .ok_or("Monitor index out of bounds")?
        .clone();

    let state_file = temp_dir.join("state.json");

    // CRITICAL FIX: Reset state.json to prevent instant close from previous ESC
    let fresh_state = SharedState::default();
    if let Ok(json) = serde_json::to_string(&fresh_state) {
        let _ = fs::write(&state_file, json);
        tracing::info!("Child process: Reset state.json (cleared should_close flag)");
    }

    // CRITICAL FIX: Calculate final texture size BEFORE creating window
    // This allows us to position and size the window correctly
    let (texture_width, texture_height) = calculate_final_texture_size(&monitor);

    // Calculate scale ratio to adjust window position
    let scale_x = texture_width as f32 / monitor.width as f32;
    let scale_y = texture_height as f32 / monitor.height as f32;

    // Scale window position proportionally to texture size
    let window_x = monitor.x as f32 * scale_x;
    let window_y = monitor.y as f32 * scale_y;

    tracing::info!(
        "Child process starting for monitor {} - Monitor: ({}, {}) {}×{} → Window: ({:.0}, {:.0}) {}×{} (scale: {:.3})",
        monitor.screen_index,
        monitor.x, monitor.y, monitor.width, monitor.height,
        window_x, window_y, texture_width, texture_height,
        scale_x
    );

    // Create window for THIS monitor only
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_position(egui::pos2(window_x, window_y))
            .with_inner_size(egui::vec2(texture_width as f32, texture_height as f32))
            .with_resizable(false)
            .with_taskbar(false),
        ..Default::default()
    };

    let window_title = format!("Screenshot Overlay - Monitor {}", monitor.screen_index);

    eframe::run_native(
        &window_title,
        options,
        Box::new(move |cc| {
            Ok(Box::new(OverlayApp::new(cc, monitor, state_file, vdb)))
        }),
    )?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Check if we're a child process
    if args.len() == 3 && args[1] == "--monitor" {
        let monitor_index: usize = args[2].parse()?;
        return run_monitor_overlay(monitor_index);
    }

    // Check for --only-monitor flag (F10: capture all but show only selected monitor)
    let only_monitor: Option<usize> = if args.len() == 3 && args[1] == "--only-monitor" {
        Some(args[2].parse()?)
    } else {
        None
    };

    // ===== PARENT PROCESS MODE =====

    if let Some(mon_idx) = only_monitor {
        tracing::info!("Parent process: starting screenshot overlay for Monitor {} ONLY", mon_idx);
    } else {
        tracing::info!("Parent process: starting multi-monitor screenshot overlay");
    }

    // Capture all monitors (PNG screenshots already saved by capture function)
    let monitors = capture_all_monitors();

    if monitors.is_empty() {
        return Err("No monitors captured".into());
    }

    let temp_dir = std::env::temp_dir().join("egui_overlay");

    // Calculate virtual desktop bounds
    let virtual_desktop_bounds = calculate_virtual_desktop_bounds(&monitors);
    tracing::info!(
        "Virtual desktop bounds: ({:.0}, {:.0}) → ({:.0}, {:.0}) [{}×{}]",
        virtual_desktop_bounds.min.x,
        virtual_desktop_bounds.min.y,
        virtual_desktop_bounds.max.x,
        virtual_desktop_bounds.max.y,
        virtual_desktop_bounds.width(),
        virtual_desktop_bounds.height()
    );

    // Save metadata to JSON
    fs::write(
        temp_dir.join("monitors.json"),
        serde_json::to_string(&monitors)?
    )?;

    fs::write(
        temp_dir.join("vdb.json"),
        serde_json::to_string(&[
            virtual_desktop_bounds.min.x,
            virtual_desktop_bounds.min.y,
            virtual_desktop_bounds.max.x,
            virtual_desktop_bounds.max.y,
        ])?
    )?;

    fs::write(
        temp_dir.join("state.json"),
        serde_json::to_string(&SharedState::default())?
    )?;

    tracing::info!("Saved metadata to temp directory");

    // Launch child process per monitor (or only selected monitor if --only-monitor was used)
    let exe_path = std::env::current_exe()?;
    let mut children = Vec::new();

    let monitors_to_launch: Vec<usize> = if let Some(selected_idx) = only_monitor {
        // F10 mode: Launch only selected monitor
        vec![selected_idx]
    } else {
        // F11 mode: Launch all monitors
        (0..monitors.len()).collect()
    };

    for index in monitors_to_launch {
        tracing::info!("Launching child process for monitor {}", index);
        let child = Command::new(&exe_path)
            .arg("--monitor")
            .arg(index.to_string())
            .spawn()?;
        children.push(child);
    }

    tracing::info!("Launched {} child process(es)", children.len());

    // Wait for all children to exit
    for (index, mut child) in children.into_iter().enumerate() {
        match child.wait() {
            Ok(status) => {
                if status.success() {
                    tracing::info!("Child process {} exited successfully", index);
                } else {
                    tracing::warn!("Child process {} exited with status: {}", index, status);
                }
            }
            Err(e) => {
                tracing::error!("Failed to wait for child process {}: {}", index, e);
            }
        }
    }

    // Cleanup temp directory (DISABLED for diagnostics)
    // if let Err(e) = fs::remove_dir_all(&temp_dir) {
    //     tracing::warn!("Failed to cleanup temp directory: {}", e);
    // } else {
    //     tracing::info!("Cleaned up temp directory");
    // }
    tracing::info!("Temp files preserved in: {}", temp_dir.display());

    tracing::info!("Parent process exiting");

    Ok(())
}
