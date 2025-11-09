# Screenshot Overlay - Multi-Monitor Rendering Issue (10+ hours debugging)

## 🔴 VISUAL EVIDENCE - What User Sees (Cannot Attach Images)

**NOTE:** I cannot attach screenshots, so I'm describing in extreme detail what appears on each monitor.

### Monitor 0 Overlay (2560×1440 @ 150% DPI):
**BEFORE F10:** Clean desktop with icons, taskbar, background image
**AFTER F10:**
- Overlay appears FULL SCREEN
- Shows **ONLY ZOOMED FRAGMENT** of the application window (not desktop)
- NO desktop icons visible
- NO wallpaper visible
- ONLY a portion of app UI (zoomed/cropped)
- **Looks like a CROP from larger image, not full monitor screenshot**

### Monitor 1 Overlay (1920×1080 @ 100% DPI):
**BEFORE F10:** Clean desktop with sunset wallpaper
**AFTER F10:**
- Overlay appears FULL SCREEN
- LEFT SIDE: Shows desktop (icons, wallpaper) - looks like it's from Monitor 0
- RIGHT SIDE: Shows zoomed overlay
- **Looks like MIDDLE OF VIRTUAL DESKTOP** - the boundary between two monitors
- This is COMPLETELY WRONG - should show only Monitor 1's screenshot

### What This Suggests:
The PNG file may contain **ENTIRE VIRTUAL DESKTOP** (both monitors together at ~4480px wide) instead of individual monitor screenshots.

---

## Problem Summary
Multi-monitor screenshot overlay using `eframe/egui` + `xcap` shows incorrect content - appears to be rendering fragments of entire virtual desktop instead of individual monitor screenshots.

## Current Architecture
- **Parent process**: Captures all monitors using `xcap::Monitor::all()`, saves PNG per monitor, spawns child processes
- **Child process per monitor**: Loads screenshot PNG, creates fullscreen overlay window
- **Synchronization**: File-based IPC via JSON in `%TEMP%\egui_overlay/`

## What We Implemented (Based on Your Suggestion + Our Additions)

### 1. Your Fix: Store Actual Texture Dimensions
**File**: `overlay_egui.rs`
**Lines**: 95-96, 223-230

```rust
struct OverlayApp {
    monitor: CapturedMonitor,
    texture: Option<egui::TextureHandle>,
    texture_width: u32,   // Actual texture width after GPU downscale
    texture_height: u32,  // Actual texture height after GPU downscale
    // ...
}

// Extract actual texture dimensions
let (texture_width, texture_height) = if let Some(tex) = &texture {
    let [w, h] = tex.size();
    (w as u32, h as u32)
} else {
    (monitor.width, monitor.height)
};
```

**Log Output**: `📐 Texture actual size: 2048×1152`

### 2. Updated All Rendering Rects
**Lines**: 353, 436, 496, 514
Changed all `monitor.width/height` to `texture_width/texture_height`

### 3. Calculate Final Texture Size BEFORE Window Creation
**Lines**: 754-771

```rust
fn calculate_final_texture_size(monitor: &CapturedMonitor) -> (u32, u32) {
    const MAX_TEXTURE_SIZE: u32 = 2048;
    if monitor.width > MAX_TEXTURE_SIZE || monitor.height > MAX_TEXTURE_SIZE {
        let scale = (MAX_TEXTURE_SIZE as f32 / monitor.width.max(monitor.height) as f32).min(1.0);
        ((monitor.width as f32 * scale) as u32, (monitor.height as f32 * scale) as u32)
    } else {
        (monitor.width, monitor.height)
    }
}
```

### 4. Scale Window Position AND Size
**Lines**: 820-847

```rust
// Calculate final texture size BEFORE creating window
let (texture_width, texture_height) = calculate_final_texture_size(&monitor);

// Scale window position proportionally
let scale_x = texture_width as f32 / monitor.width as f32;
let scale_y = texture_height as f32 / monitor.height as f32;
let window_x = monitor.x as f32 * scale_x;
let window_y = monitor.y as f32 * scale_y;

let options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default()
        .with_position(egui::pos2(window_x, window_y))
        .with_inner_size(egui::vec2(texture_width as f32, texture_height as f32))
        // ...
};
```

**Log Output**: `Monitor 0 - Monitor: (0, 0) 2560×1440 → Window: (0, 0) 2048×1152 (scale: 0.800)`

## What's STILL Broken

Despite ALL these fixes, overlay shows COMPLETELY WRONG content (see Visual Evidence above).

## Our New Hypothesis: PNG Contains Entire Virtual Desktop

### Screenshot Capture Code (lines 683-746)

```rust
fn capture_all_monitors() -> Vec<CapturedMonitor> {
    let monitors = Monitor::all()?;

    monitors
        .into_iter()
        .enumerate()
        .filter_map(|(index, monitor)| {
            match monitor.capture_image() {  // ← NO BOUNDS PARAMETER!
                Ok(rgba_image) => {
                    let physical_width = rgba_image.width();
                    let physical_height = rgba_image.height();

                    // Save RAW PNG
                    let image_path = temp_dir.join(format!("monitor_{}.png", index));
                    rgba_image.save(&image_path)?;

                    Some(CapturedMonitor {
                        image_path,  // Each monitor has its own file
                        x: monitor.x()?,
                        y: monitor.y()?,
                        width: monitor.width()?,
                        height: monitor.height()?,
                        scale_factor: monitor.scale_factor()?,
                        screen_index: index,
                    })
                }
            }
        })
        .collect()
}
```

**WE DON'T PASS ANY BOUNDS/REGION!** We just call `monitor.capture_image()` with no parameters.

### Texture Rendering Code (lines 362-370)

```rust
if let Some(texture) = &self.texture {
    let rect = full_rect;  // Window-space rect (texture_width × texture_height)
    painter.image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),  // UV coords
        egui::Color32::WHITE,
    );
}
```

**UV coords are (0,0) → (1,1)** - we show entire texture without cropping.

## CRITICAL QUESTION

**Does `xcap::Monitor::capture_image()` on Windows with dual-monitor setup return:**

**A) Individual monitor screenshot?**
- Then the bug is somewhere else in our rendering pipeline
- Need to investigate texture loading, UV coords, or window positioning

**B) Entire virtual desktop?**
- Then we MUST crop the PNG using `monitor.x/y/width/height` as bounds
- Would explain why Monitor 0 shows zoom (crop at 0,0 from larger image)
- Would explain why Monitor 1 shows desktop+zoom (crop at 2560,0 shows boundary)

### Why We Suspect (B):

1. **Monitor 0 shows zoom** → Could be crop at position (0, 0) size 2560×1440 from virtual desktop
2. **Monitor 1 shows desktop LEFT + zoom RIGHT** → Could be crop at position (2560, 0) size 1920×1080
3. **Monitor 1 offset calculation**: Monitor 0 is 2560px wide, Monitor 1 starts at X=2560
4. **Visual appearance**: Monitor 1 overlay looks EXACTLY like middle of virtual desktop where monitors meet

## Monitor Data Structure

```rust
struct CapturedMonitor {
    image_path: PathBuf,      // monitor_0.png, monitor_1.png, etc.
    x: i32,                   // From xcap::Monitor.x()
    y: i32,                   // From xcap::Monitor.y()
    width: u32,               // From xcap::Monitor.width() - logical
    height: u32,              // From xcap::Monitor.height() - logical
    scale_factor: f64,        // From xcap::Monitor.scale_factor() - NEVER USED!
    screen_index: usize,
}
```

**NOTE**: `scale_factor` is stored but NEVER used anywhere in rendering!

## What We've Tried (All Failed)

✅ Storing texture_width/texture_height → Works, but doesn't fix rendering
✅ Updated all rendering rects → Works, but doesn't fix rendering
✅ Calculated window size before creation → Works, but doesn't fix rendering
✅ Scaled window position → Works, but doesn't fix rendering
❌ **Content still renders incorrectly**

## What We Need

**PRIMARY QUESTION:** Does `xcap::Monitor::capture_image()` on Windows with dual-monitor return individual monitor or entire virtual desktop?

**If it returns virtual desktop:**
- How do we crop PNG to specific monitor?
- Should we use `monitor.x/y/width/height` as crop bounds?
- Do we need `image::imageops::crop()` or `SubImage`?
- Example code?

**If it returns individual monitor:**
- Where is the bug in our rendering pipeline?
- Are UV coordinates wrong?
- Does window positioning in multi-DPI corrupt content somehow?
- Is there some egui viewport/scaling setting we're missing?

## Environment

- **OS**: Windows 11
- **Rust**: latest stable
- **egui/eframe**: 0.29.1
- **xcap**: 0.7.1
- **Setup**: Dual monitor
  - Monitor 0: 2560×1440 @ 150% DPI
  - Monitor 1: 1920×1080 @ 100% DPI

---

**After 10+ hours of debugging, we're completely stuck. Any help would be greatly appreciated!**

Thank you!
