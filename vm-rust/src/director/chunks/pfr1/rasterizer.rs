/// PFR1 Rasterizer - Outline-to-bitmap rendering
/// Implements: Bezier flattening, winding-number scanline fill, grid bitmap assembly

use super::types::*;
use super::log;

/// Flatten a cubic bezier curve into line segments using de Casteljau subdivision
fn flatten_cubic_bezier(
    x0: f32, y0: f32,
    x1: f32, y1: f32,
    x2: f32, y2: f32,
    x3: f32, y3: f32,
    tolerance: f32,
    output: &mut Vec<(f32, f32)>,
) {
    // Check if the curve is flat enough
    let dx = x3 - x0;
    let dy = y3 - y0;
    let d2 = ((x1 - x3) * dy - (y1 - y3) * dx).abs();
    let d3 = ((x2 - x3) * dy - (y2 - y3) * dx).abs();

    let flatness = (d2 + d3) * (d2 + d3);
    let tolerance_sq = tolerance * tolerance * (dx * dx + dy * dy);

    if flatness <= tolerance_sq {
        output.push((x3, y3));
        return;
    }

    // Subdivide at t=0.5
    let x01 = (x0 + x1) * 0.5;
    let y01 = (y0 + y1) * 0.5;
    let x12 = (x1 + x2) * 0.5;
    let y12 = (y1 + y2) * 0.5;
    let x23 = (x2 + x3) * 0.5;
    let y23 = (y2 + y3) * 0.5;

    let x012 = (x01 + x12) * 0.5;
    let y012 = (y01 + y12) * 0.5;
    let x123 = (x12 + x23) * 0.5;
    let y123 = (y12 + y23) * 0.5;

    let x0123 = (x012 + x123) * 0.5;
    let y0123 = (y012 + y123) * 0.5;

    flatten_cubic_bezier(x0, y0, x01, y01, x012, y012, x0123, y0123, tolerance, output);
    flatten_cubic_bezier(x0123, y0123, x123, y123, x23, y23, x3, y3, tolerance, output);
}

/// Convert contour commands to line segments (flattening curves)
fn contour_to_edges(contour: &PfrContour, tolerance: f32) -> Vec<(f32, f32)> {
    let mut points: Vec<(f32, f32)> = Vec::new();
    let mut cur_x: f32 = 0.0;
    let mut cur_y: f32 = 0.0;

    for cmd in &contour.commands {
        match cmd.cmd_type {
            PfrCmdType::MoveTo => {
                cur_x = cmd.x;
                cur_y = cmd.y;
                points.push((cur_x, cur_y));
            }
            PfrCmdType::LineTo => {
                cur_x = cmd.x;
                cur_y = cmd.y;
                points.push((cur_x, cur_y));
            }
            PfrCmdType::CurveTo => {
                flatten_cubic_bezier(
                    cur_x, cur_y,
                    cmd.x1, cmd.y1,
                    cmd.x2, cmd.y2,
                    cmd.x, cmd.y,
                    tolerance,
                    &mut points,
                );
                cur_x = cmd.x;
                cur_y = cmd.y;
            }
            PfrCmdType::Close => {
                // Close is handled implicitly by the polygon
            }
        }
    }

    points
}

/// Rasterize a single glyph outline to a bitmap using winding number fill rule
fn rasterize_glyph_to_bitmap(
    glyph: &OutlineGlyph,
    width: usize,
    height: usize,
    scale_x: f32,
    scale_y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Vec<u8> {
    rasterize_glyph_to_bitmap_oversampled(glyph, width, height, scale_x, scale_y, offset_x, offset_y, 1)
}

fn rasterize_glyph_to_alpha_mask(
    glyph: &OutlineGlyph,
    width: usize,
    height: usize,
    scale_x: f32,
    scale_y: f32,
    offset_x: f32,
    offset_y: f32,
    oversample: usize,
) -> Vec<u8> {
    if oversample <= 1 {
        let bitmap = rasterize_glyph_to_bitmap_raw(glyph, width, height, scale_x, scale_y, offset_x, offset_y);
        let bytes_per_row = (width + 7) / 8;
        let mut alpha = vec![0u8; width * height];
        for y in 0..height {
            for x in 0..width {
                let byte_idx = y * bytes_per_row + x / 8;
                let bit_idx = 7 - (x % 8);
                if byte_idx < bitmap.len() && (bitmap[byte_idx] & (1 << bit_idx)) != 0 {
                    alpha[y * width + x] = 255;
                }
            }
        }
        return alpha;
    }

    let hi_w = width * oversample;
    let hi_h = height * oversample;
    let hi_scale_x = scale_x * oversample as f32;
    let hi_scale_y = scale_y * oversample as f32;
    let hi_offset_x = offset_x * oversample as f32;
    let hi_offset_y = offset_y * oversample as f32;

    let hi_bitmap = rasterize_glyph_to_bitmap_raw(glyph, hi_w, hi_h, hi_scale_x, hi_scale_y, hi_offset_x, hi_offset_y);
    let hi_bpr = (hi_w + 7) / 8;

    let mut alpha = vec![0u8; width * height];
    let block = (oversample * oversample) as u32;

    for y in 0..height {
        for x in 0..width {
            let mut count = 0u32;
            let base_x = x * oversample;
            let base_y = y * oversample;
            for oy in 0..oversample {
                let hy = base_y + oy;
                let row_base = hy * hi_bpr;
                for ox in 0..oversample {
                    let hx = base_x + ox;
                    let byte_idx = row_base + hx / 8;
                    let bit_idx = 7 - (hx % 8);
                    if byte_idx < hi_bitmap.len() && (hi_bitmap[byte_idx] & (1 << bit_idx)) != 0 {
                        count += 1;
                    }
                }
            }
            let raw_coverage = (count * 255 / block) as u8;
            // Apply gamma boost (gamma=0.5) to make anti-aliased edges more prominent.
            // This makes text bolder and more readable at small sizes.
            let coverage = if raw_coverage == 0 || raw_coverage == 255 {
                raw_coverage
            } else {
                let norm = raw_coverage as f32 / 255.0;
                (norm.sqrt() * 255.0).round().min(255.0) as u8
            };
            alpha[y * width + x] = coverage;
        }
    }

    alpha
}

fn rasterize_glyph_to_bitmap_oversampled(
    glyph: &OutlineGlyph,
    width: usize,
    height: usize,
    scale_x: f32,
    scale_y: f32,
    offset_x: f32,
    offset_y: f32,
    oversample: usize,
) -> Vec<u8> {
    if oversample <= 1 {
        return rasterize_glyph_to_bitmap_raw(glyph, width, height, scale_x, scale_y, offset_x, offset_y);
    }

    let hi_w = width * oversample;
    let hi_h = height * oversample;
    let hi_scale_x = scale_x * oversample as f32;
    let hi_scale_y = scale_y * oversample as f32;
    let hi_offset_x = offset_x * oversample as f32;
    let hi_offset_y = offset_y * oversample as f32;

    let hi_bitmap = rasterize_glyph_to_bitmap_raw(glyph, hi_w, hi_h, hi_scale_x, hi_scale_y, hi_offset_x, hi_offset_y);
    let hi_bpr = (hi_w + 7) / 8;

    let bytes_per_row = (width + 7) / 8;
    let mut bitmap = vec![0u8; bytes_per_row * height];

    let block = oversample * oversample;
    let threshold = 1; // any sub-pixel filled → pixel filled (captures thin strokes)

    for y in 0..height {
        for x in 0..width {
            let mut count = 0usize;
            let base_x = x * oversample;
            let base_y = y * oversample;
            for oy in 0..oversample {
                let hy = base_y + oy;
                let row_base = hy * hi_bpr;
                for ox in 0..oversample {
                    let hx = base_x + ox;
                    let byte_idx = row_base + hx / 8;
                    let bit_idx = 7 - (hx % 8);
                    if byte_idx < hi_bitmap.len() && (hi_bitmap[byte_idx] & (1 << bit_idx)) != 0 {
                        count += 1;
                    }
                }
            }
            if count >= threshold {
                let byte_idx = y * bytes_per_row + x / 8;
                let bit_idx = 7 - (x % 8);
                if byte_idx < bitmap.len() {
                    bitmap[byte_idx] |= 1 << bit_idx;
                }
            }
        }
    }

    bitmap
}

fn rasterize_glyph_to_bitmap_raw(
    glyph: &OutlineGlyph,
    width: usize,
    height: usize,
    scale_x: f32,
    scale_y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Vec<u8> {
    let bytes_per_row = (width + 7) / 8;
    let mut bitmap = vec![0u8; bytes_per_row * height];

    // Flatten all contours to polygon edges
    let mut all_polygons: Vec<Vec<(f32, f32)>> = Vec::new();
    for contour in &glyph.contours {
        let points = contour_to_edges(contour, 0.5);
        if points.len() >= 3 {
            all_polygons.push(points);
        }
    }

    if all_polygons.is_empty() {
        return bitmap;
    }

    // Scanline rasterization with non-zero winding fill rule
    for y in 0..height {
        let scan_y = y as f32 + 0.5;

        // Collect edge crossings with winding direction
        let mut crossings: Vec<(f32, i32)> = Vec::new();

        for polygon in &all_polygons {
            let n = polygon.len();
            for i in 0..n {
                let (mut x0, mut y0) = polygon[i];
                let (mut x1, mut y1) = polygon[(i + 1) % n];

                // Transform to bitmap coordinates
                x0 = x0 * scale_x + offset_x;
                y0 = y0 * scale_y + offset_y;
                x1 = x1 * scale_x + offset_x;
                y1 = y1 * scale_y + offset_y;

                // Skip horizontal edges
                if (y0 - y1).abs() < 0.001 {
                    continue;
                }

                // Check if scanline crosses this edge
                if (y0 <= scan_y && y1 > scan_y) || (y1 <= scan_y && y0 > scan_y) {
                    let t = (scan_y - y0) / (y1 - y0);
                    let x_cross = x0 + t * (x1 - x0);
                    // Direction: +1 if edge goes upward (y0 < y1), -1 if downward
                    let dir = if y0 < y1 { 1 } else { -1 };
                    crossings.push((x_cross, dir));
                }
            }
        }

        // Sort crossings by x
        crossings.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        // Fill using non-zero winding rule
        let mut winding = 0i32;
        for i in 0..crossings.len() {
            winding += crossings[i].1;

            // Fill span when winding number is non-zero
            if i + 1 < crossings.len() {
                if winding != 0 {
                    let x0 = crossings[i].0;
                    let x1 = crossings[i + 1].0;
                    let x_start = x0.max(0.0).min(width as f32) as usize;
                    let x_end = x1.max(0.0).min(width as f32) as usize;

                    for bx in x_start..x_end {
                        let byte_idx = y * bytes_per_row + bx / 8;
                        let bit_idx = 7 - (bx % 8);
                        if byte_idx < bitmap.len() {
                            bitmap[byte_idx] |= 1 << bit_idx;
                        }
                    }
                }
            }
        }
    }

    bitmap
}

/// Result of rasterizing a PFR1 font
pub struct RasterizedFont {
    /// RGBA bitmap data for the entire glyph grid
    pub bitmap_data: Vec<u8>,
    /// Width of the bitmap in pixels
    pub bitmap_width: usize,
    /// Height of the bitmap in pixels
    pub bitmap_height: usize,
    /// Width of each grid cell
    pub cell_width: usize,
    /// Height of each grid cell
    pub cell_height: usize,
    /// Number of grid columns
    pub grid_columns: usize,
    /// Number of grid rows
    pub grid_rows: usize,
    /// Per-character advance widths (in pixels)
    pub char_widths: Vec<u16>,
    /// First char code in the grid
    pub first_char: u8,
    /// Number of chars
    pub num_chars: usize,
}

/// Rasterize a parsed PFR1 font into a grid bitmap
/// Returns RGBA bitmap data + per-character advance widths
pub fn rasterize_pfr1_font(
    parsed_font: &Pfr1ParsedFont,
    target_height: usize,
) -> RasterizedFont {
    let phys = &parsed_font.physical_font;

    let outline_res = phys.outline_resolution as f32;
    let target_em_px = parsed_font.target_em_px as f32;
    let coords_scaled = target_em_px > 0.0 && outline_res > 0.0;

    let scale = if coords_scaled {
        // Coordinates are already in target pixel space (parsed at actual target size).
        1.0
    } else {
        let metric_height = (phys.metrics.ascender as f32 - phys.metrics.descender as f32).abs();
        if metric_height > 0.0 {
            target_height as f32 / metric_height
        } else if outline_res > 0.0 {
            target_height as f32 / outline_res
        } else {
            1.0
        }
    };

    // Apply font matrix (mA, mB, mC, mD at 1/256 scale)
    let font_matrix = if !parsed_font.logical_fonts.is_empty() {
        parsed_font.logical_fonts[0].font_matrix
    } else {
        [256, 0, 0, 256]
    };

    let matrix_scale_x = font_matrix[0] as f32 / 256.0;
    let matrix_scale_y = font_matrix[3] as f32 / 256.0;

    // Use magnitude for sizing; we apply a single Y flip below.
    let scale_x = scale * matrix_scale_x.abs();
    let scale_y = scale * matrix_scale_y.abs();

    // Determine cell dimensions
    // Find max glyph width from set_widths and from actual glyph bbox widths.
    let max_set_width = parsed_font.glyphs.values()
        .map(|g| g.set_width)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(phys.standard_set_width as f32);

    let base_set_width_scale = if coords_scaled {
        // set_width is in orus; scale directly to actual target pixel size
        target_height as f32 / outline_res
    } else {
        scale_x.abs()
    };
    let set_width_scale = base_set_width_scale;

    let mut max_bbox_width = 0.0f32;
    for glyph in parsed_font.glyphs.values() {
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        for contour in &glyph.contours {
            for cmd in &contour.commands {
                min_x = min_x.min(cmd.x);
                max_x = max_x.max(cmd.x);
                if cmd.cmd_type == PfrCmdType::CurveTo {
                    min_x = min_x.min(cmd.x1).min(cmd.x2);
                    max_x = max_x.max(cmd.x1).max(cmd.x2);
                }
            }
        }
        if min_x < max_x {
            let w = (max_x - min_x).abs();
            if w > max_bbox_width {
                max_bbox_width = w;
            }
        }
    }
    let max_bbox_width_px = max_bbox_width * scale_x.abs();

    let cell_width = ((max_set_width * base_set_width_scale).ceil() as usize)
        .max(max_bbox_width_px.ceil() as usize)
        .max(1);
    let cell_height = {
        let pixel_scale_metric = if outline_res > 0.0 {
            target_height as f32 / outline_res
        } else {
            1.0
        };
        let descender_px = if phys.metrics.descender < 0 && outline_res > 0.0 {
            (phys.metrics.descender.abs() as f32 * pixel_scale_metric).ceil() as usize
        } else {
            0
        };
        // Baseline row = floor(ascender * pixel_scale). Cell must contain baseline + descender.
        // When ascender > outline_res, baseline exceeds target_height and the old formula
        // (target_height + descender_px) would clip the bottom of every character.
        let baseline_row = if phys.metrics.ascender > 0 && outline_res > 0.0 {
            (phys.metrics.ascender as f32 * pixel_scale_metric).floor() as usize
        } else {
            target_height
        };
        (baseline_row + descender_px + 1).max(target_height)
    };

    // Determine character range
    let first_char: u8 = 0;
    let num_chars: usize = 128; // ASCII range
    let grid_columns: usize = 16;
    let grid_rows: usize = (num_chars + grid_columns - 1) / grid_columns;

    let bitmap_width = cell_width * grid_columns;
    let bitmap_height = cell_height * grid_rows;

    log(&format!("Rasterizing PFR1 font: {}x{} cells, {}x{} grid, {}x{} bitmap, scale={:.4}",
        cell_width, cell_height, grid_columns, grid_rows, bitmap_width, bitmap_height, scale));
    log(&format!(
        "Rasterize metrics: asc={} desc={} cell_height={} target_height={} scale_y={} coords_scaled={} px_scale={:.6}",
        phys.metrics.ascender,
        phys.metrics.descender,
        cell_height,
        target_height,
        scale_y,
        coords_scaled,
        if outline_res > 0.0 { target_em_px / outline_res } else { 0.0 }
    ));

    // Summary diagnostic (always emit)
    log(&format!(
        "PFR1 rasterize '{}': target_em={}px cell={}x{} outline_glyphs={} bitmap_glyphs={} coords_scaled={}",
        parsed_font.font_name,
        target_em_px,
        cell_width, cell_height,
        parsed_font.glyphs.len(),
        parsed_font.bitmap_glyphs.len(),
        coords_scaled
    ));

    // Create RGBA bitmap (transparent white background)
    let mut rgba = vec![0u8; bitmap_width * bitmap_height * 4];
    for i in 0..(bitmap_width * bitmap_height) {
        let idx = i * 4;
        rgba[idx] = 255;
        rgba[idx + 1] = 255;
        rgba[idx + 2] = 255;
        rgba[idx + 3] = 0;
    }

    // Per-character advance widths
    let mut char_widths = vec![cell_width as u16; num_chars];
    let trace_bitmap_debug = parsed_font
        .font_name
        .to_ascii_lowercase()
        .contains("tiki magic");
    let mut bitmap_overlap_outline = 0usize;
    let mut bitmap_only = 0usize;
    let mut bitmap_pixels_drawn = 0usize;

    // Render each glyph
    let font_min_x = phys.metrics.x_min as f32;
    let font_asc = phys.metrics.ascender as f32;

    // One-time metrics diagnostic
    {
        let diag_pixel_scale = if coords_scaled { target_em_px / outline_res } else { 0.0 };
        let diag_baseline = if coords_scaled { (font_asc * diag_pixel_scale).floor() } else { font_asc };
        let diag_offset_x = if coords_scaled { (-font_min_x * diag_pixel_scale).floor() } else { 0.0 };
        log(&format!(
            "[DIAG] font='{}' asc={} x_min={} px_scale={:.6} baseline={:.2} offset_x={:.2} cell={}x{} target={} blue_values={:?} blue_fuzz={} blue_scale={}",
            parsed_font.font_name, font_asc, font_min_x, diag_pixel_scale,
            diag_baseline, diag_offset_x, cell_width, cell_height, target_em_px,
            phys.blue_values, phys.blue_fuzz, phys.blue_scale
        ));
    }

    for (&char_code, glyph) in &parsed_font.glyphs {
        let idx = char_code as usize;
        if idx >= num_chars { continue; }

        let col = idx % grid_columns;
        let row = idx / grid_columns;
        let cell_x = col * cell_width;
        let cell_y = row * cell_height;

        // Calculate proportional width for this glyph (rounded, matching bitmap glyph path)
        let glyph_pixel_width = (glyph.set_width * set_width_scale).round() as usize;

        // Find glyph bounding box
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for contour in &glyph.contours {
            for cmd in &contour.commands {
                min_x = min_x.min(cmd.x);
                min_y = min_y.min(cmd.y);
                max_x = max_x.max(cmd.x);
                max_y = max_y.max(cmd.y);
                if cmd.cmd_type == PfrCmdType::CurveTo {
                    min_x = min_x.min(cmd.x1).min(cmd.x2);
                    min_y = min_y.min(cmd.y1).min(cmd.y2);
                    max_x = max_x.max(cmd.x1).max(cmd.x2);
                    max_y = max_y.max(cmd.y1).max(cmd.y2);
                }
            }
        }

        if min_x >= max_x || min_y >= max_y {
            if glyph_pixel_width > 0 {
                char_widths[idx] = glyph_pixel_width as u16;
            }
            continue;
        }

        // Use set_width-based advance (SetWidth * setWidthScale)
        if glyph_pixel_width > 0 {
            char_widths[idx] = glyph_pixel_width as u16;
        }

        // Rasterize this glyph
        // The glyph coordinates need to be mapped into the cell
        // PFR uses Y-up, bitmap uses Y-down, so flip Y
        let glyph_scale_x = scale_x;
        // y' = baseline - ty. If the font matrix already flipped Y (D<0),
        // then ty is already inverted, so we should not flip again.
        let glyph_scale_y = if matrix_scale_y < 0.0 { scale_y } else { -scale_y };
        // Use font metrics for a stable baseline across glyphs.
        let pixel_scale = if coords_scaled {
            // Final rendering scale: orus → actual target pixels
            target_height as f32 / outline_res
        } else {
            scale_y.abs()
        };
        // use font-wide origin, not per-glyph min_x.
        // Per-glyph origin heuristics can shift some glyphs out of cell bounds (e.g. 'e','s').
        // When coords_scaled, glyph coordinates are already in pixel space (integers for pixel
        // fonts at their native size). Round offsets to ensure pixel-aligned placement so
        // integer glyph points land exactly on pixel boundaries in the 1-bit bitmap.
        let glyph_offset_x = if coords_scaled {
            (-font_min_x * pixel_scale).floor()
        } else {
            -font_min_x * glyph_scale_x
        };
        let glyph_offset_y = if font_asc != 0.0 {
            let mut baseline = font_asc * pixel_scale;
            if coords_scaled {
                baseline = baseline.floor();
            }
            if !baseline.is_finite() {
                baseline = 0.0;
            }
            baseline
        } else if glyph_scale_y < 0.0 {
            max_y * (-glyph_scale_y)
        } else {
            -min_y * glyph_scale_y
        };

        // Anti-aliased rendering: use alpha mask with 4x oversampling
        let alpha_mask = rasterize_glyph_to_alpha_mask(
            glyph,
            cell_width,
            cell_height,
            glyph_scale_x,
            glyph_scale_y,
            glyph_offset_x,
            glyph_offset_y,
            4,  // 4x oversampling for smooth anti-aliased edges
        );

        // Copy alpha mask to RGBA grid (anti-aliased text)
        for gy in 0..cell_height {
            for gx in 0..cell_width {
                let coverage = alpha_mask[gy * cell_width + gx];
                if coverage > 0 {
                    let px = cell_x + gx;
                    let py = cell_y + gy;
                    if px < bitmap_width && py < bitmap_height {
                        let rgba_idx = (py * bitmap_width + px) * 4;
                        if rgba_idx + 3 < rgba.len() {
                            rgba[rgba_idx] = 0;         // R (black text)
                            rgba[rgba_idx + 1] = 0;     // G
                            rgba[rgba_idx + 2] = 0;     // B
                            rgba[rgba_idx + 3] = coverage; // A (anti-aliased)
                        }
                    }
                }
            }
        }
    }

    // Also render bitmap glyphs if any
    for (&char_code, bmp_glyph) in &parsed_font.bitmap_glyphs {
        if parsed_font.glyphs.contains_key(&char_code) {
            bitmap_overlap_outline += 1;
            continue; // Outline glyphs preferred - they support anti-aliasing
        } else {
            bitmap_only += 1;
        }

        if trace_bitmap_debug {
            log(&format!(
                "[pfr1.bitmap] char={} x_size={} y_size={} x_pos={} y_pos={} set_width={} overlap_outline={}",
                char_code,
                bmp_glyph.x_size,
                bmp_glyph.y_size,
                bmp_glyph.x_pos,
                bmp_glyph.y_pos,
                bmp_glyph.set_width,
                parsed_font.glyphs.contains_key(&char_code)
            ));
        }

        let idx = char_code as usize;
        if idx >= num_chars { continue; }

        let col = idx % grid_columns;
        let row = idx / grid_columns;
        let cell_x = col * cell_width;
        let cell_y = row * cell_height;

        // Guard against malformed/unsupported bitmap glyph metrics.
        // Oversized bitmap glyphs can spill far outside their cell and create block artifacts.
        let max_reasonable_w = cell_width.saturating_mul(4);
        let max_reasonable_h = cell_height.saturating_mul(4);
        if bmp_glyph.x_size == 0
            || bmp_glyph.y_size == 0
            || (bmp_glyph.x_size as usize) > max_reasonable_w
            || (bmp_glyph.y_size as usize) > max_reasonable_h
        {
            if trace_bitmap_debug {
                log(&format!(
                    "[pfr1.bitmap] skip char={} unreasonable size {}x{} for cell {}x{}",
                    char_code,
                    bmp_glyph.x_size,
                    bmp_glyph.y_size,
                    cell_width,
                    cell_height
                ));
            }
            continue;
        }

        // Bitmap glyph set_width is in font units; convert to pixel advance.
        let bmp_adv = ((bmp_glyph.set_width as f32) * set_width_scale).round().max(1.0) as u16;
        char_widths[idx] = bmp_adv;

        // Copy bitmap data to RGBA grid
        let glyph_bits_per_row = bmp_glyph.x_size as usize;
        for gy in 0..bmp_glyph.y_size as usize {
            for gx in 0..bmp_glyph.x_size as usize {
                let bit_index = gy * glyph_bits_per_row + gx;
                let byte_idx = bit_index / 8;
                let bit_idx = 7 - (bit_index % 8);
                if byte_idx < bmp_glyph.image_data.len() {
                    let mut bit = (bmp_glyph.image_data[byte_idx] & (1 << bit_idx)) != 0;
                    if !parsed_font.pfr_black_pixel {
                        bit = !bit;
                    }
                    if !bit {
                        continue;
                    }
                    bitmap_pixels_drawn += 1;
                    let px = cell_x + gx + bmp_glyph.x_pos.max(0) as usize;
                    let py = cell_y + gy + bmp_glyph.y_pos.max(0) as usize;
                    if px < bitmap_width
                        && py < bitmap_height
                        && px < cell_x + cell_width
                        && py < cell_y + cell_height
                    {
                        let rgba_idx = (py * bitmap_width + px) * 4;
                        if rgba_idx + 3 < rgba.len() {
                            rgba[rgba_idx] = 0;
                            rgba[rgba_idx + 1] = 0;
                            rgba[rgba_idx + 2] = 0;
                            rgba[rgba_idx + 3] = 255;
                        }
                    }
                }
            }
        }
    }

    log(&format!("Rasterized {} outline + {} bitmap glyphs",
        parsed_font.glyphs.len(), parsed_font.bitmap_glyphs.len()));
    if trace_bitmap_debug {
        log(&format!(
            "[pfr1.bitmap] summary overlap_outline={} bitmap_only={} pixels_drawn={} pfr_black_pixel={}",
            bitmap_overlap_outline,
            bitmap_only,
            bitmap_pixels_drawn,
            parsed_font.pfr_black_pixel
        ));
    }

    // Fallback for caps-only PFR fonts:
    // if a lowercase cell rendered empty, copy from a non-empty letter glyph.
    let cell_has_ink = |rgba: &[u8], cx: usize, cy: usize| -> bool {
        for gy in 0..cell_height {
            for gx in 0..cell_width {
                let p = ((cy + gy) * bitmap_width + (cx + gx)) * 4;
                if p + 3 < rgba.len() {
                    let r = rgba[p];
                    let g = rgba[p + 1];
                    let b = rgba[p + 2];
                    let a = rgba[p + 3];
                    if a > 0 && !(r >= 250 && g >= 250 && b >= 250) {
                        return true;
                    }
                }
            }
        }
        false
    };


    let cell_bbox_h = |rgba: &[u8], cx: usize, cy: usize| -> usize {
        let mut min_y = cell_height as i32;
        let mut max_y = -1i32;
        for gy in 0..cell_height {
            for gx in 0..cell_width {
                let p = ((cy + gy) * bitmap_width + (cx + gx)) * 4;
                if p + 3 < rgba.len() {
                    let r = rgba[p];
                    let g = rgba[p + 1];
                    let b = rgba[p + 2];
                    let a = rgba[p + 3];
                    if a > 0 && !(r >= 250 && g >= 250 && b >= 250) {
                        min_y = min_y.min(gy as i32);
                        max_y = max_y.max(gy as i32);
                    }
                }
            }
        }
        if max_y >= min_y {
            (max_y - min_y + 1) as usize
        } else {
            0
        }
    };
    let cell_origin = |idx: usize| -> (usize, usize) {
        let col = idx % grid_columns;
        let row = idx / grid_columns;
        (col * cell_width, row * cell_height)
    };

    let copy_cell = |rgba: &mut Vec<u8>, src_idx: usize, dst_idx: usize| {
        let (src_x, src_y) = cell_origin(src_idx);
        let (dst_x, dst_y) = cell_origin(dst_idx);
        for gy in 0..cell_height {
            for gx in 0..cell_width {
                let s = ((src_y + gy) * bitmap_width + (src_x + gx)) * 4;
                let d = ((dst_y + gy) * bitmap_width + (dst_x + gx)) * 4;
                let px = [rgba[s], rgba[s + 1], rgba[s + 2], rgba[s + 3]];
                rgba[d..d + 4].copy_from_slice(&px);
            }
        }
    };

    for lc in b'a'..=b'z' {
        let li = lc as usize;
        if li >= num_chars {
            continue;
        }
        let (lcx, lcy) = cell_origin(li);
        if cell_has_ink(&rgba, lcx, lcy) {
            let lc_h = cell_bbox_h(&rgba, lcx, lcy);
            if lc_h > 2 {
                continue;
            }
        }

        let mut src_idx_opt: Option<usize> = None;
        let ui = (lc - 32) as usize;
        if ui < num_chars {
            let (ucx, ucy) = cell_origin(ui);
            if cell_has_ink(&rgba, ucx, ucy) {
                src_idx_opt = Some(ui);
            }
        }


        if let Some(src_idx) = src_idx_opt {
            copy_cell(&mut rgba, src_idx, li);
            char_widths[li] = char_widths[src_idx];
        }
    }

    RasterizedFont {
        bitmap_data: rgba,
        bitmap_width,
        bitmap_height,
        cell_width,
        cell_height,
        grid_columns,
        grid_rows,
        char_widths,
        first_char,
        num_chars,
    }
}
