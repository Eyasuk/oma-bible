//! A felt-tip marker stroke painted under each wrapped line of a verse.
//!
//! Instead of a box behind the whole verse, the highlight follows the text:
//! one stroke per visual line, as wide as the words on that line, with
//! slightly uneven edges, ragged ends and a darker overlapping pass, so it
//! reads like a real highlighter on paper.

use gpui_kit::*;

/// How tall the stroke is, as a share of the font size.
const STROKE_HEIGHT: f32 = 1.06;
/// How far the stroke sits below the line's centre, as a share of font size.
const STROKE_DROP: f32 = 0.05;
/// How far the stroke runs past the first and last letter.
const OVERHANG: f32 = 5.0;
/// Edge points per 100 px of stroke; more points make a rougher edge.
const EDGE_DENSITY: f32 = 7.0;

/// An element that fills its parent and paints marker strokes under `text`.
///
/// Put it first inside a `relative()` wrapper that also holds the text and
/// carries the same `text_size` and `line_height`, so both wrap identically.
pub fn marker(text: SharedString, color: Hsla, alpha: f32, seed: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _cx| paint(&text, color, alpha, seed, bounds, window),
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

fn paint(
    text: &SharedString,
    color: Hsla,
    alpha: f32,
    seed: u32,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    let line_height = style.line_height_in_pixels(window.rem_size());
    let run = style.to_run(text.len());
    let Ok(lines) = window.text_system().shape_text(
        text.clone(),
        font_size,
        &[run],
        Some(bounds.size.width),
        None,
    ) else {
        return;
    };

    let mut row = 0usize;
    for line in &lines {
        for segment in segments(line, text) {
            let Some((left, right)) = segment else {
                row += 1;
                continue;
            };
            stroke_line(
                f32::from(bounds.left()) + left,
                f32::from(bounds.left()) + right,
                bounds.top() + line_height * row as f32,
                line_height,
                font_size,
                seed.wrapping_mul(97).wrapping_add(row as u32),
                color,
                alpha,
                window,
            );
            row += 1;
        }
    }
}

/// Paints the stroke for one line of words spanning `left..right`, with the
/// line box starting at `top`.
#[allow(clippy::too_many_arguments)]
pub fn stroke_line(
    left: f32,
    right: f32,
    top: Pixels,
    line_height: Pixels,
    font_size: Pixels,
    seed: u32,
    color: Hsla,
    alpha: f32,
    window: &mut Window,
) {
    let font = f32::from(font_size);
    let centre = f32::from(top) + f32::from(line_height) / 2.0 + font * STROKE_DROP;
    stroke(
        left - OVERHANG,
        right + OVERHANG,
        centre,
        font * STROKE_HEIGHT,
        seed,
        color,
        alpha,
        window,
    );
}

/// The left and right x of the words on each visual line of `line`,
/// relative to the text's origin, with edge spaces trimmed.
fn segments(line: &WrappedLine, text: &str) -> Vec<Option<(f32, f32)>> {
    let layout = &line.unwrapped_layout;
    // Every glyph as (x, byte index), plus an end sentinel.
    let mut glyphs: Vec<(f32, usize)> = Vec::new();
    let mut breaks: Vec<usize> = Vec::new();
    for (run_ix, run) in layout.runs.iter().enumerate() {
        for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
            if line
                .wrap_boundaries
                .iter()
                .any(|b| b.run_ix == run_ix && b.glyph_ix == glyph_ix)
            {
                breaks.push(glyphs.len());
            }
            glyphs.push((f32::from(glyph.position.x), glyph.index));
        }
    }
    glyphs.push((f32::from(layout.width), layout.len));

    let line_text = &text[..line.text.len().min(text.len())];
    let is_space = |glyph: usize| {
        let index = glyphs[glyph].1;
        line_text[index..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    };

    let last = glyphs.len() - 1;
    let mut starts = vec![0];
    starts.extend(breaks.iter().copied());
    let mut ends = breaks;
    ends.push(last);

    starts
        .into_iter()
        .zip(ends)
        .map(|(line_start, mut end)| {
            // Wrapped lines restart at x = 0, so measure from the line's
            // own first glyph.
            let origin = glyphs[line_start].0;
            let mut start = line_start;
            while start < end && is_space(start) {
                start += 1;
            }
            while end > start && is_space(end - 1) {
                end -= 1;
            }
            (start < end).then(|| (glyphs[start].0 - origin, glyphs[end].0 - origin))
        })
        .collect()
}

/// One marker pass: a filled band with wobbling edges and slanted, ragged
/// ends, then a narrower second pass that darkens where it overlaps.
#[allow(clippy::too_many_arguments)]
fn stroke(
    x0: f32,
    x1: f32,
    centre: f32,
    height: f32,
    seed: u32,
    color: Hsla,
    alpha: f32,
    window: &mut Window,
) {
    let base = band(x0, x1, centre, height, seed, 1.0);
    let streak = band(
        x0 + 2.0 + noise(seed, 900) * 3.0,
        x1 - 3.0 + noise(seed, 901) * 4.0,
        centre + noise(seed, 902) * height * 0.08,
        height * 0.62,
        seed.wrapping_add(7_919),
        0.7,
    );
    if let Some(path) = base {
        window.paint_path(path, color.opacity(alpha));
    }
    if let Some(path) = streak {
        window.paint_path(path, color.opacity(alpha * 0.55));
    }
}

/// Builds the outline of one band. `rough` scales how much the edges wobble.
fn band(x0: f32, x1: f32, centre: f32, height: f32, seed: u32, rough: f32) -> Option<Path<Pixels>> {
    let width = (x1 - x0).max(height * 0.5);
    let x1 = x0 + width;
    let half = height / 2.0;
    // The pen drifts a little up or down across the line.
    let tilt = noise(seed, 1) * height * 0.06;
    let steps = ((width / 100.0) * EDGE_DENSITY).ceil().max(3.0) as usize;
    let wobble = height * 0.045 * rough;
    let slant = height * 0.18;

    let mut points = Vec::with_capacity(steps * 2 + 8);
    // Top edge, left to right.
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = x0 + slant + (width - slant * 2.0) * t;
        let y = centre - half + tilt * (t - 0.5) + noise(seed, 10 + i as u32) * wobble;
        points.push(point(px(x), px(y)));
    }
    // Right end: the pen lifts at an angle, leaving a frayed tip.
    let right = [
        (x1 - slant * 0.2 + noise(seed, 3) * 2.0, -0.55),
        (x1 + noise(seed, 4) * 2.5, -0.1),
        (x1 - slant * 0.5 + noise(seed, 5) * 2.0, 0.35),
        (x1 - slant * 1.1, 0.95),
    ];
    for (x, k) in right {
        points.push(point(px(x), px(centre + tilt * 0.5 + half * k)));
    }
    // Bottom edge, right to left.
    for i in (0..=steps).rev() {
        let t = i as f32 / steps as f32;
        let x = x0 + slant * 0.4 + (width - slant * 1.5) * t;
        let y = centre + half + tilt * (t - 0.5) + noise(seed, 400 + i as u32) * wobble;
        points.push(point(px(x), px(y)));
    }
    // Left end: the pen lands at the opposite angle.
    let left = [
        (x0 - slant * 0.1 + noise(seed, 6) * 2.0, 0.6),
        (x0 + noise(seed, 7) * 2.5, 0.1),
        (x0 + slant * 0.4 + noise(seed, 8) * 2.0, -0.45),
        (x0 + slant * 0.9, -0.95),
    ];
    for (x, k) in left {
        points.push(point(px(x), px(centre - tilt * 0.5 + half * k)));
    }

    let mut builder = PathBuilder::fill();
    builder.add_polygon(&points, true);
    builder.build().ok()
}

/// Deterministic noise in [-1, 1], so a stroke looks the same every frame.
fn noise(seed: u32, salt: u32) -> f32 {
    let mut x = seed ^ salt.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}
