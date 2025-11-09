use tauri::{AppHandle, Manager, Emitter};
use std::{fs, path::PathBuf, process::Command};
use xcap::Monitor;

// Legacy imports for dead_code functions (will be removed in future)
#[allow(unused_imports)]
use screenshots::{Screen, image::RgbaImage};

#[allow(dead_code)]
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// Windows-specific cursor position detection
#[cfg(windows)]
fn get_cursor_position() -> std::result::Result<(i32, i32), String> {
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    use windows::Win32::Foundation::POINT;

    unsafe {
        let mut point = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut point).is_ok() {
            Ok((point.x, point.y))
        } else {
            Err("Failed to get cursor position".into())
        }
    }
}

#[cfg(not(windows))]
fn get_cursor_position() -> std::result::Result<(i32, i32), String> {
    Err("Cursor detection only supported on Windows".into())
}

/// Detect which monitor contains the cursor
fn detect_monitor_at_cursor() -> std::result::Result<usize, String> {
    let cursor_pos = get_cursor_position()?;

    let monitors = Monitor::all()
        .map_err(|e| format!("Failed to get monitors: {}", e))?;

    // Sort monitors by X position (left to right) for consistent indexing
    let mut monitors: Vec<_> = monitors.into_iter().enumerate().collect();
    monitors.sort_by_key(|(_, m)| m.x().unwrap_or(0));

    for (idx, monitor) in monitors.iter() {
        let x = monitor.x().unwrap_or(0);
        let y = monitor.y().unwrap_or(0);
        let w = monitor.width().unwrap_or(1920) as i32;
        let h = monitor.height().unwrap_or(1080) as i32;

        tracing::debug!(
            "Monitor {}: bounds ({}, {}) → ({}, {})",
            idx, x, y, x + w, y + h
        );

        if cursor_pos.0 >= x && cursor_pos.0 < x + w &&
           cursor_pos.1 >= y && cursor_pos.1 < y + h {
            tracing::info!(
                "✅ Cursor at ({}, {}) is on Monitor {}",
                cursor_pos.0, cursor_pos.1, idx
            );
            return Ok(*idx);
        }
    }

    tracing::warn!(
        "⚠️ Cursor at ({}, {}) not on any detected monitor, defaulting to Monitor 0",
        cursor_pos.0, cursor_pos.1
    );
    Ok(0) // Fallback to primary monitor
}

/// F10 → Launch overlay for ACTIVE monitor (where cursor is)
#[tauri::command]
pub async fn launch_screenshot_overlay_active_monitor() -> std::result::Result<String, String> {
    let monitor_index = detect_monitor_at_cursor()?;

    tracing::info!("🚀 Launching overlay for active Monitor {}...", monitor_index);

    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Failed to get current exe: {}", e))?
        .parent()
        .ok_or("No parent directory")?
        .join("overlay_egui.exe");

    tracing::info!("📍 Overlay path: {}", exe_path.display());

    // Launch overlay in PARENT MODE with --only-monitor flag
    // Parent will capture all monitors but spawn child only for specified monitor
    Command::new(&exe_path)
        .arg("--only-monitor")
        .arg(monitor_index.to_string())
        .spawn()
        .map_err(|e| format!("Failed to spawn overlay: {}", e))?;

    Ok(format!("Launched overlay for Monitor {} from {}", monitor_index, exe_path.display()))
}

/// F11 → Launch overlay for ALL monitors
#[tauri::command]
pub async fn launch_screenshot_overlay_all_monitors() -> std::result::Result<String, String> {
    tracing::info!("🚀 Launching overlay for ALL monitors...");

    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Failed to get current exe: {}", e))?
        .parent()
        .ok_or("No parent directory")?
        .join("overlay_egui.exe");

    tracing::info!("📍 Overlay path: {}", exe_path.display());

    // Launch overlay WITHOUT --monitor argument (parent process mode)
    Command::new(&exe_path)
        .spawn()
        .map_err(|e| format!("Failed to spawn overlay: {}", e))?;

    Ok(format!("Launched overlay for all monitors from {}", exe_path.display()))
}

/// LEGACY: Old F8 hotkey (deprecated, use F10/F11 instead)
#[tauri::command]
pub async fn launch_screenshot_overlay() -> std::result::Result<String, String> {
    tracing::warn!("⚠️ Using deprecated launch_screenshot_overlay (F8). Use F10/F11 instead.");
    launch_screenshot_overlay_all_monitors().await
}

/// Zwraca prostą ścieżkę do pliku store z ostatnim screenshotem (używane przez /ss)
#[allow(dead_code)]
fn store_path(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap().join("aplikacja3-store.json")
}

/// Zapisuje prosty JSON z polem last_screenshot_path (kompatybilne z Twoim simple_expansion)
#[allow(dead_code)]
fn write_last_screenshot(app: &AppHandle, path: &str) -> Result<()> {
    let data = serde_json::json!({
        "last_screenshot_path": path,
        "updated": chrono::Utc::now().to_rfc3339(),
        "total_shortcuts": 0
    });
    let p = store_path(app);
    if let Some(parent) = p.parent() { fs::create_dir_all(parent)?; }
    fs::write(p, serde_json::to_vec_pretty(&data)?)?;
    Ok(())
}

/// Czyści ewentualny stan - u nas nic nie trzymamy, ale zostawiamy sygnaturę kompatybilną
#[allow(dead_code)]
pub fn cancel_screenshot(_app: AppHandle) -> Result<()> {
    Ok(())
}

/// Główny capture: składa obraz z wielu ekranów na podstawie absolutnego prostokąta (x,y,w,h)
#[allow(dead_code)]
pub fn capture_region_and_save(app: AppHandle, x: i32, y: i32, w: i32, h: i32) -> Result<String> {
    let sel_x = x;
    let sel_y = y;
    let sel_w = w.max(0) as u32;
    let sel_h = h.max(0) as u32;

    // obraz wynikowy
    let mut final_img: RgbaImage = RgbaImage::new(sel_w, sel_h);

    // iterujemy po wszystkich ekranach i bierzemy część, która nachodzi na zaznaczenie
    for screen in Screen::all()? {
        let info = screen.display_info;
        let sx = info.x;
        let sy = info.y;
        let sw = info.width as i32;
        let sh = info.height as i32;

        // prostokąty przecięcia w ABS współrzędnych
        let ix = sel_x.max(sx);
        let iy = sel_y.max(sy);
        let ix2 = (sel_x + sel_w as i32).min(sx + sw);
        let iy2 = (sel_y + sel_h as i32).min(sy + sh);

        if ix2 <= ix || iy2 <= iy {
            continue; // brak przecięcia
        }

        let inter_w = (ix2 - ix) as u32;
        let inter_h = (iy2 - iy) as u32;

        // współrzędne względne ekranowe
        let rel_x = ix - sx;
        let rel_y = iy - sy;

        // capture_area: (x: i32, y: i32, width: u32, height: u32)
        let piece: RgbaImage = screen.capture_area(rel_x, rel_y, inter_w, inter_h)?;

        // gdzie wkleić w final_img (offset względem lewego-górnego rogu zaznaczenia)
        let dx = (ix - sel_x) as i64;
        let dy = (iy - sel_y) as i64;

        // ręczne wklejenie pikseli - bez `imageops::overlay`, żeby uniknąć konfliktu wersji `image`
        for yy in 0..inter_h {
            for xx in 0..inter_w {
                let px = piece.get_pixel(xx, yy);
                final_img.put_pixel((dx as u32) + xx, (dy as u32) + yy, *px);
            }
        }
    }

    // zapisz PNG w %TEMP%\aplikacja3\screens\YYYYmmdd_HHMMSS.png
    let mut out_dir = std::env::temp_dir();
    out_dir.push("aplikacja3");
    out_dir.push("screens");
    fs::create_dir_all(&out_dir)?;

    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("screenshot_{}.png", ts);
    let out_path = out_dir.join(filename);

    // zapis (RgbaImage ma .save())
    final_img.save(&out_path)?;

    // zapisz ścieżkę do store + emit event do frontu
    let out_str = out_path.to_string_lossy().to_string();
    let _ = write_last_screenshot(&app, &out_str);
    let _ = app.emit("screenshot-saved", &out_str);

    Ok(out_str)
}
