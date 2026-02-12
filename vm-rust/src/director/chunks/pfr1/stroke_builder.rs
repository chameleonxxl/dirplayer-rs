/// PFR1 Stroke Builder

use super::types::{PfrContour, PfrCmd, PfrStroke, PfrStrokeType};

pub fn build_contours_from_strokes(strokes: &[PfrStroke]) -> Vec<PfrContour> {
    let mut contours = Vec::new();

    for stroke in strokes {
        if let Some(contour) = convert_stroke_to_contour(stroke) {
            if !contour.commands.is_empty() {
                contours.push(contour);
            }
        }
    }

    contours
}

fn convert_stroke_to_contour(stroke: &PfrStroke) -> Option<PfrContour> {
    let mut contour = PfrContour::new();

    match stroke.stroke_type {
        PfrStrokeType::Line => build_line_stroke(&mut contour, stroke),
        PfrStrokeType::Diagonal => build_line_stroke(&mut contour, stroke),
        PfrStrokeType::Curve => build_curve_stroke(&mut contour, stroke),
    }

    if contour.commands.is_empty() {
        None
    } else {
        Some(contour)
    }
}

fn build_line_stroke(contour: &mut PfrContour, stroke: &PfrStroke) {
    let width = stroke.width;
    let half_width = width * 0.5;

    let dx = stroke.end_x - stroke.start_x;
    let dy = stroke.end_y - stroke.start_y;
    let len = (dx * dx + dy * dy).sqrt();

    if len < 0.001 {
        contour.commands.push(PfrCmd::move_to(stroke.start_x - half_width, stroke.start_y - half_width));
        contour.commands.push(PfrCmd::line_to(stroke.start_x + half_width, stroke.start_y - half_width));
        contour.commands.push(PfrCmd::line_to(stroke.start_x + half_width, stroke.start_y + half_width));
        contour.commands.push(PfrCmd::line_to(stroke.start_x - half_width, stroke.start_y + half_width));
        contour.commands.push(PfrCmd::close());
        return;
    }

    let perp_x = -dy / len * half_width;
    let perp_y = dx / len * half_width;

    let x1 = stroke.start_x + perp_x;
    let y1 = stroke.start_y + perp_y;
    let x2 = stroke.end_x + perp_x;
    let y2 = stroke.end_y + perp_y;
    let x3 = stroke.end_x - perp_x;
    let y3 = stroke.end_y - perp_y;
    let x4 = stroke.start_x - perp_x;
    let y4 = stroke.start_y - perp_y;

    contour.commands.push(PfrCmd::move_to(x1, y1));
    contour.commands.push(PfrCmd::line_to(x2, y2));
    contour.commands.push(PfrCmd::line_to(x3, y3));
    contour.commands.push(PfrCmd::line_to(x4, y4));
    contour.commands.push(PfrCmd::close());
}

fn build_curve_stroke(contour: &mut PfrContour, stroke: &PfrStroke) {
    let width = stroke.width;
    let half_width = width * 0.5;

    let points = flatten_cubic_bezier(
        stroke.start_x, stroke.start_y,
        stroke.control1_x, stroke.control1_y,
        stroke.control2_x, stroke.control2_y,
        stroke.end_x, stroke.end_y,
        0.5,
    );

    if points.len() < 2 {
        return;
    }

    let mut left_side: Vec<(f32, f32)> = Vec::new();
    let mut right_side: Vec<(f32, f32)> = Vec::new();

    for i in 0..points.len() - 1 {
        let p0 = points[i];
        let p1 = points[i + 1];

        let dx = p1.0 - p0.0;
        let dy = p1.1 - p0.1;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.001 {
            continue;
        }

        let perp_x = -dy / len * half_width;
        let perp_y = dx / len * half_width;

        if i == 0 {
            left_side.push((p0.0 + perp_x, p0.1 + perp_y));
            right_side.push((p0.0 - perp_x, p0.1 - perp_y));
        }

        left_side.push((p1.0 + perp_x, p1.1 + perp_y));
        right_side.push((p1.0 - perp_x, p1.1 - perp_y));
    }

    if left_side.len() < 2 {
        return;
    }

    contour.commands.push(PfrCmd::move_to(left_side[0].0, left_side[0].1));

    for i in 1..left_side.len() {
        contour.commands.push(PfrCmd::line_to(left_side[i].0, left_side[i].1));
    }

    for i in (0..right_side.len()).rev() {
        contour.commands.push(PfrCmd::line_to(right_side[i].0, right_side[i].1));
    }

    contour.commands.push(PfrCmd::close());
}

fn flatten_cubic_bezier(
    x0: f32, y0: f32,
    x1: f32, y1: f32,
    x2: f32, y2: f32,
    x3: f32, y3: f32,
    tolerance: f32,
) -> Vec<(f32, f32)> {
    let mut points = Vec::new();
    points.push((x0, y0));
    flatten_cubic_bezier_recursive(
        x0, y0, x1, y1, x2, y2, x3, y3, tolerance, 0, &mut points,
    );
    points.push((x3, y3));
    points
}

fn flatten_cubic_bezier_recursive(
    x0: f32, y0: f32,
    x1: f32, y1: f32,
    x2: f32, y2: f32,
    x3: f32, y3: f32,
    tolerance: f32,
    depth: u32,
    out: &mut Vec<(f32, f32)>,
) {
    if depth > 12 {
        return;
    }

    let dx = x3 - x0;
    let dy = y3 - y0;
    let d1 = ((x1 - x3) * dy - (y1 - y3) * dx).abs();
    let d2 = ((x2 - x3) * dy - (y2 - y3) * dx).abs();
    let d = d1 + d2;
    let len_sq = dx * dx + dy * dy;

    if d * d <= tolerance * tolerance * len_sq {
        return;
    }

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

    flatten_cubic_bezier_recursive(x0, y0, x01, y01, x012, y012, x0123, y0123, tolerance, depth + 1, out);
    out.push((x0123, y0123));
    flatten_cubic_bezier_recursive(x0123, y0123, x123, y123, x23, y23, x3, y3, tolerance, depth + 1, out);
}
