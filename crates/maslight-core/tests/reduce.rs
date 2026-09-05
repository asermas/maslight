use maslight_core::layout::Rect;
use maslight_core::reduce::{detect_black_bars, FrameView, Insets, PixelFormat, Reducer};

/// Build a BGRA test frame from a per-pixel closure.
fn frame(width: u32, height: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let mut data = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let p = ((y * width + x) * 4) as usize;
            let [r, g, b] = f(x, y);
            data[p] = b;
            data[p + 1] = g;
            data[p + 2] = r;
            data[p + 3] = 255;
        }
    }
    data
}

#[test]
fn a_solid_colour_reduces_to_that_colour() {
    let data = frame(64, 64, |_, _| [255, 0, 0]);
    let view = FrameView::new(&data, 64, 64, 64 * 4, PixelFormat::Bgra8);
    let out = Reducer::default().reduce_one(&view, Rect::FULL);
    assert!((out.r - 1.0).abs() < 1e-3, "{out:?}");
    assert!(out.g < 1e-3 && out.b < 1e-3, "{out:?}");
}

#[test]
fn averaging_happens_in_linear_light_not_in_srgb() {
    // Left half black, right half white.
    let data = frame(
        64,
        64,
        |x, _| if x < 32 { [0, 0, 0] } else { [255, 255, 255] },
    );
    let view = FrameView::new(&data, 64, 64, 64 * 4, PixelFormat::Bgra8);
    let out = Reducer::default().reduce_one(&view, Rect::FULL);
    // Linear average of 0.0 and 1.0 is 0.5. Averaging the sRGB codes instead
    // would give 127/255 = 0.498 encoded, which is only 0.21 in linear light.
    assert!(
        (out.r - 0.5).abs() < 0.02,
        "expected 0.5 linear, got {}",
        out.r
    );
}

#[test]
fn each_rectangle_only_sees_its_own_region() {
    // Left half red, right half blue.
    let data = frame(
        100,
        40,
        |x, _| if x < 50 { [255, 0, 0] } else { [0, 0, 255] },
    );
    let view = FrameView::new(&data, 100, 40, 100 * 4, PixelFormat::Bgra8);

    let rects = [Rect::new(0.0, 0.0, 0.4, 1.0), Rect::new(0.6, 0.0, 0.4, 1.0)];
    let mut out = Vec::new();
    Reducer::default().reduce(&view, &rects, Insets::default(), &mut out);

    assert_eq!(out.len(), 2);
    assert!(
        out[0].r > 0.9 && out[0].b < 0.05,
        "left should be red: {:?}",
        out[0]
    );
    assert!(
        out[1].b > 0.9 && out[1].r < 0.05,
        "right should be blue: {:?}",
        out[1]
    );
}

#[test]
fn rgba_and_bgra_frames_agree() {
    let width = 16;
    let height = 16;
    let mut bgra = vec![0u8; width * height * 4];
    let mut rgba = vec![0u8; width * height * 4];
    for i in 0..width * height {
        let p = i * 4;
        // A distinctive colour so a channel swap cannot pass by accident.
        let (r, g, b) = (200u8, 120u8, 40u8);
        bgra[p] = b;
        bgra[p + 1] = g;
        bgra[p + 2] = r;
        rgba[p] = r;
        rgba[p + 1] = g;
        rgba[p + 2] = b;
    }
    let reducer = Reducer::default();
    let a = reducer.reduce_one(
        &FrameView::new(
            &bgra,
            width as u32,
            height as u32,
            width * 4,
            PixelFormat::Bgra8,
        ),
        Rect::FULL,
    );
    let b = reducer.reduce_one(
        &FrameView::new(
            &rgba,
            width as u32,
            height as u32,
            width * 4,
            PixelFormat::Rgba8,
        ),
        Rect::FULL,
    );
    assert!((a.r - b.r).abs() < 1e-6 && (a.g - b.g).abs() < 1e-6 && (a.b - b.b).abs() < 1e-6);
}

#[test]
fn padded_stride_is_respected() {
    // Capture APIs hand back rows padded to a hardware alignment. Getting this
    // wrong shears the image diagonally, so it is worth a test.
    let width = 10u32;
    let height = 4u32;
    let stride = 64usize;
    let mut data = vec![0u8; stride * height as usize];
    for y in 0..height {
        for x in 0..width {
            let p = y as usize * stride + x as usize * 4;
            // Green only in the first row.
            if y == 0 {
                data[p + 1] = 255;
            }
            data[p + 3] = 255;
        }
    }
    let view = FrameView::new(&data, width, height, stride, PixelFormat::Bgra8);
    let top = Reducer::default().reduce_one(&view, Rect::new(0.0, 0.0, 1.0, 0.25));
    let bottom = Reducer::default().reduce_one(&view, Rect::new(0.0, 0.5, 1.0, 0.5));
    assert!(top.g > 0.9, "first row should be green: {top:?}");
    assert!(bottom.g < 0.05, "lower rows should be black: {bottom:?}");
}

#[test]
fn letterbox_bars_are_detected_and_cropped_out() {
    // 200x100 frame with 20-pixel black bars top and bottom, white in between.
    let data = frame(200, 100, |_, y| {
        if (20..80).contains(&y) {
            [255, 255, 255]
        } else {
            [0, 0, 0]
        }
    });
    let view = FrameView::new(&data, 200, 100, 200 * 4, PixelFormat::Bgra8);

    let insets = detect_black_bars(&view, 0.01);
    assert!((insets.top - 0.2).abs() < 0.02, "{insets:?}");
    assert!((insets.bottom - 0.2).abs() < 0.02, "{insets:?}");
    assert_eq!(insets.left, 0.0);
    assert_eq!(insets.right, 0.0);

    // A strip along the bottom edge sees black without cropping, and the
    // actual picture once the bars are removed.
    let bottom_strip = Rect::new(0.0, 0.9, 1.0, 0.1);
    let reducer = Reducer::default();
    let raw = reducer.reduce_one(&view, bottom_strip);
    let cropped = reducer.reduce_one(&view, insets.apply(bottom_strip));
    assert!(raw.g < 0.05, "uncropped strip should be black: {raw:?}");
    assert!(
        cropped.g > 0.9,
        "cropped strip should see the picture: {cropped:?}"
    );
}

#[test]
fn a_full_frame_is_never_treated_as_one_big_bar() {
    let data = frame(64, 64, |_, _| [0, 0, 0]);
    let view = FrameView::new(&data, 64, 64, 64 * 4, PixelFormat::Bgra8);
    let insets = detect_black_bars(&view, 0.01);
    // Even for an all-black screen the detector must stop at 40% per side.
    assert!(insets.top <= 0.4 && insets.bottom <= 0.4);
    assert!(insets.apply(Rect::FULL).w > 0.0);
}

#[test]
fn an_undersized_buffer_is_rejected_instead_of_panicking() {
    let data = vec![0u8; 10];
    let view = FrameView::new(&data, 1920, 1080, 1920 * 4, PixelFormat::Bgra8);
    assert!(!view.is_valid());

    let mut out = Vec::new();
    Reducer::default().reduce(&view, &[Rect::FULL; 3], Insets::default(), &mut out);
    assert_eq!(out.len(), 3);
    assert!(out.iter().all(|c| *c == maslight_core::Rgb::BLACK));
}

#[test]
fn tap_count_is_bounded_by_the_grid_not_by_resolution() {
    // 4K frame, tiny grid: the reducer must still return promptly and give the
    // right answer. This is the property that keeps CPU flat across resolutions.
    let data = frame(
        3840,
        16,
        |x, _| if x < 1920 { [255, 0, 0] } else { [0, 255, 0] },
    );
    let view = FrameView::new(&data, 3840, 16, 3840 * 4, PixelFormat::Bgra8);
    let reducer = Reducer::new(8);
    let left = reducer.reduce_one(&view, Rect::new(0.0, 0.0, 0.5, 1.0));
    assert!(left.r > 0.9 && left.g < 0.05, "{left:?}");
}
