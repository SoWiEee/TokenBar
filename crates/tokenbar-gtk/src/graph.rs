//! Pure data → bar-geometry layer for the 3D contribution graph.
//!
//! Kept free of any GL so it is unit-testable on its own: the renderer draws
//! whatever `Vec<Bar>` this produces. Phase 1 Step B feeds it a demo pattern;
//! Step C swaps `demo_grid` for a mapping from `tb_reports::usage_graph`.

/// One contribution cell rendered as a bar. `x`/`z` are world-space grid centers
/// (the whole grid is centered on the origin); `height` scales the unit cube
/// upward from y = 0; `color` is linear RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub x: f32,
    pub z: f32,
    pub height: f32,
    pub color: [f32; 3],
}

/// Spacing between adjacent bar centers (bars are drawn slightly narrower so a
/// gap shows between them — see the cube shrink in the renderer).
pub const SPACING: f32 = 1.0;
/// Height of an empty cell and the tallest cell, in world units.
const MIN_HEIGHT: f32 = 0.12;
const MAX_HEIGHT: f32 = 4.2;

/// Map a normalized intensity `v` in [0, 1] to a bar height.
fn height_for(v: f32) -> f32 {
    MIN_HEIGHT + v.clamp(0.0, 1.0) * (MAX_HEIGHT - MIN_HEIGHT)
}

/// Low→high color ramp (dark slate → blue → teal-green), interpolated in linear
/// RGB. Empty-ish cells stay dim so busy cells pop.
fn color_for(v: f32) -> [f32; 3] {
    const STOPS: [[f32; 3]; 3] = [
        [0.11, 0.16, 0.27], // low
        [0.18, 0.49, 0.85], // mid
        [0.30, 0.88, 0.66], // high
    ];
    let v = v.clamp(0.0, 1.0);
    let (a, b, t) = if v < 0.5 {
        (STOPS[0], STOPS[1], v / 0.5)
    } else {
        (STOPS[1], STOPS[2], (v - 0.5) / 0.5)
    };
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Build a `cols` × `rows` grid of bars centered on the origin from a per-cell
/// intensity function `intensity(col, row) -> [0, 1]`.
pub fn grid_from<F>(cols: usize, rows: usize, intensity: F) -> Vec<Bar>
where
    F: Fn(usize, usize) -> f32,
{
    let x0 = -((cols as f32 - 1.0) * SPACING) / 2.0;
    let z0 = -((rows as f32 - 1.0) * SPACING) / 2.0;
    let mut bars = Vec::with_capacity(cols * rows);
    for col in 0..cols {
        for row in 0..rows {
            let v = intensity(col, row).clamp(0.0, 1.0);
            bars.push(Bar {
                x: x0 + col as f32 * SPACING,
                z: z0 + row as f32 * SPACING,
                height: height_for(v),
                color: color_for(v),
            });
        }
    }
    bars
}

/// Demo pattern for Step B: a smooth, varied field so the grid looks like real
/// lumpy usage before `tb_reports` is wired in.
pub fn demo_grid(cols: usize, rows: usize) -> Vec<Bar> {
    grid_from(cols, rows, |col, row| {
        let c = col as f32;
        let r = row as f32;
        0.5 + 0.32 * (c * 0.4).sin() + 0.22 * (r * 1.1 + c * 0.18).cos()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_has_one_bar_per_cell_and_is_centered() {
        let bars = grid_from(4, 2, |_, _| 0.5);
        assert_eq!(bars.len(), 8);
        // Centered on origin: mean x and z are ~0.
        let sx: f32 = bars.iter().map(|b| b.x).sum::<f32>() / bars.len() as f32;
        let sz: f32 = bars.iter().map(|b| b.z).sum::<f32>() / bars.len() as f32;
        assert!(sx.abs() < 1e-4, "grid not x-centered: {sx}");
        assert!(sz.abs() < 1e-4, "grid not z-centered: {sz}");
    }

    #[test]
    fn intensity_maps_to_height_range() {
        // Empty cell → min height; full cell → max height; clamped beyond [0,1].
        let empty = grid_from(1, 1, |_, _| 0.0)[0];
        let full = grid_from(1, 1, |_, _| 1.0)[0];
        let over = grid_from(1, 1, |_, _| 5.0)[0];
        assert!((empty.height - MIN_HEIGHT).abs() < 1e-5);
        assert!((full.height - MAX_HEIGHT).abs() < 1e-5);
        assert_eq!(full.height, over.height, "intensity must clamp at 1.0");
        assert!(full.height > empty.height);
    }
}
