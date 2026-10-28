//! CPU pixel conversion for the local preview: BGRA8 rows (as Windows.Graphics.Capture
//! produces them) to tightly packed RGBA8, area-averaged down to a smaller size.
//!
//! Safe code only; the platform module hands in a borrowed slice of mapped GPU memory.

use tp_contracts::capture::PixelSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleError {
    EmptySource,
    StrideTooSmall,
    SourceTooShort,
    /// The destination is empty or larger than the source (this never scales up).
    InvalidDestination,
}

/// The largest size with the aspect ratio of `src` that fits within `max`. Never larger
/// than `src`, never zero in a dimension when `src` and `max` are non-zero.
pub fn fit_within(src: PixelSize, max: PixelSize) -> PixelSize {
    if src.width == 0 || src.height == 0 || max.width == 0 || max.height == 0 {
        return PixelSize {
            width: 0,
            height: 0,
        };
    }
    if src.width <= max.width && src.height <= max.height {
        return src;
    }
    let (sw, sh) = (u64::from(src.width), u64::from(src.height));
    let (mw, mh) = (u64::from(max.width), u64::from(max.height));
    // Compare sw/sh with mw/mh without floating point.
    let (w, h) = if sw * mh >= sh * mw {
        (mw, (sh * mw / sw).max(1))
    } else {
        ((sw * mh / sh).max(1), mh)
    };
    PixelSize {
        width: u32::try_from(w).unwrap_or(max.width),
        height: u32::try_from(h).unwrap_or(max.height),
    }
}

/// Source span `[start, end)` covered by each destination index.
fn spans(src_len: u32, dst_len: u32) -> Vec<(usize, usize)> {
    let (s, d) = (u64::from(src_len), u64::from(dst_len));
    (0..d)
        .map(|i| {
            let start = i * s / d;
            let end = ((i + 1) * s / d).max(start + 1);
            (start as usize, end as usize)
        })
        .collect()
}

/// Appends `dst.width × dst.height` RGBA8 pixels to `out`. Each output pixel is the mean
/// of the source pixels it covers; alpha is always 255 (capture alpha is not
/// meaningful for display).
pub fn bgra_to_rgba_scaled(
    src: &[u8],
    src_size: PixelSize,
    stride: usize,
    dst: PixelSize,
    out: &mut Vec<u8>,
) -> Result<(), ScaleError> {
    let (sw, sh) = (src_size.width as usize, src_size.height as usize);
    if sw == 0 || sh == 0 {
        return Err(ScaleError::EmptySource);
    }
    if stride < sw * 4 {
        return Err(ScaleError::StrideTooSmall);
    }
    if src.len() < stride * (sh - 1) + sw * 4 {
        return Err(ScaleError::SourceTooShort);
    }
    if dst.width == 0
        || dst.height == 0
        || dst.width > src_size.width
        || dst.height > src_size.height
    {
        return Err(ScaleError::InvalidDestination);
    }
    let (dw, dh) = (dst.width as usize, dst.height as usize);
    out.reserve(dw * dh * 4);

    if dw == sw && dh == sh {
        for y in 0..sh {
            let (row, _) = src[y * stride..y * stride + sw * 4].as_chunks::<4>();
            for px in row {
                out.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        return Ok(());
    }

    let columns = spans(src_size.width, dst.width);
    let rows = spans(src_size.height, dst.height);
    let mut acc = vec![0u32; dw * 3];
    for &(y0, y1) in &rows {
        acc.fill(0);
        for y in y0..y1 {
            let row = &src[y * stride..y * stride + sw * 4];
            for (dx, &(x0, x1)) in columns.iter().enumerate() {
                let (mut b, mut g, mut r) = (0u32, 0u32, 0u32);
                for px in row[x0 * 4..x1 * 4].as_chunks::<4>().0 {
                    b += u32::from(px[0]);
                    g += u32::from(px[1]);
                    r += u32::from(px[2]);
                }
                acc[dx * 3] += r;
                acc[dx * 3 + 1] += g;
                acc[dx * 3 + 2] += b;
            }
        }
        let rows_covered = (y1 - y0) as u32;
        for (dx, &(x0, x1)) in columns.iter().enumerate() {
            let n = rows_covered * (x1 - x0) as u32;
            let mean = |sum: u32| ((sum + n / 2) / n) as u8;
            out.extend_from_slice(&[
                mean(acc[dx * 3]),
                mean(acc[dx * 3 + 1]),
                mean(acc[dx * 3 + 2]),
                255,
            ]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn size(width: u32, height: u32) -> PixelSize {
        PixelSize { width, height }
    }

    #[test]
    fn fit_keeps_aspect_ratio_and_never_scales_up() {
        assert_eq!(
            fit_within(size(1920, 1080), size(1280, 720)),
            size(1280, 720)
        );
        assert_eq!(
            fit_within(size(1920, 1200), size(1280, 720)),
            size(1152, 720)
        );
        assert_eq!(
            fit_within(size(1080, 1920), size(1280, 720)),
            size(405, 720)
        );
        assert_eq!(fit_within(size(800, 600), size(1280, 720)), size(800, 600));
        assert_eq!(fit_within(size(5000, 1), size(1280, 720)), size(1280, 1));
        assert_eq!(fit_within(size(0, 10), size(1280, 720)), size(0, 0));
    }

    #[test]
    fn same_size_swizzles_bgra_to_rgba_with_opaque_alpha() {
        // 2×1 source with a 12-byte stride (4 bytes of row padding).
        let src = [1, 2, 3, 0, 10, 20, 30, 7, 99, 99, 99, 99];
        let mut out = Vec::new();
        bgra_to_rgba_scaled(&src, size(2, 1), 12, size(2, 1), &mut out).unwrap();
        assert_eq!(out, vec![3, 2, 1, 255, 30, 20, 10, 255]);
    }

    #[test]
    fn downscale_averages_the_covered_area() {
        // 2×2 → 1×1: mean of four pixels.
        let src = [
            0, 0, 200, 255, 0, 0, 100, 255, //
            0, 40, 0, 255, 0, 0, 0, 255,
        ];
        let mut out = Vec::new();
        bgra_to_rgba_scaled(&src, size(2, 2), 8, size(1, 1), &mut out).unwrap();
        assert_eq!(out, vec![75, 10, 0, 255]);
    }

    #[test]
    fn non_integer_ratio_covers_every_source_pixel_once() {
        // 1920×1080 → 1280×720 of a uniform colour stays that colour exactly.
        let (w, h) = (1920usize, 1080usize);
        let src: Vec<u8> = [17u8, 34, 51, 255].repeat(w * h);
        let mut out = vec![0xAA; 3]; // existing content must be kept (append contract)
        bgra_to_rgba_scaled(&src, size(1920, 1080), w * 4, size(1280, 720), &mut out).unwrap();
        assert_eq!(out.len(), 3 + 1280 * 720 * 4);
        assert_eq!(&out[..3], &[0xAA; 3]);
        assert!(
            out[3..]
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p == &[51, 34, 17, 255])
        );
    }

    #[test]
    fn invalid_inputs_are_rejected_without_writing() {
        let src = [0u8; 16];
        let mut out = Vec::new();
        let cases = [
            (size(0, 1), 16, size(1, 1), ScaleError::EmptySource),
            (size(2, 2), 4, size(1, 1), ScaleError::StrideTooSmall),
            (size(2, 3), 8, size(1, 1), ScaleError::SourceTooShort),
            (size(2, 2), 8, size(3, 1), ScaleError::InvalidDestination),
            (size(2, 2), 8, size(0, 1), ScaleError::InvalidDestination),
        ];
        for (s, stride, d, expected) in cases {
            assert_eq!(
                bgra_to_rgba_scaled(&src, s, stride, d, &mut out),
                Err(expected)
            );
        }
        assert!(out.is_empty());
    }
}
