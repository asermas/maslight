//! The whole discovery, against synthetic photographs with known answers.
//!
//! A camera cannot be part of a test suite, so the photographs are rendered:
//! a dark room, a screen quad seen at an angle, and a dot for every LED that
//! the pattern says is lit. If the pipeline can recover the chain order and
//! the positions from those, it is doing the job the camera would give it.

use maslight_calibrate::{
    bits_needed, build_layout, detect_blobs, discover, plan, DetectSettings, Frame, Homography,
    Step,
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;

/// The screen as it appears in the photograph: not square on, because nobody
/// photographs a monitor square on.
const SCREEN_QUAD: [(f32, f32); 4] = [
    (150.0, 120.0), // top left
    (500.0, 100.0), // top right
    (520.0, 380.0), // bottom right
    (130.0, 360.0), // bottom left
];

/// Where each LED sits in the screen's own coordinates, going round the edge.
///
/// Values outside `0..1` are the point: the strip is behind the screen, not on
/// it.
fn ground_truth(n: usize) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(n);
    let per_edge = n / 4;
    let step = 1.0 / per_edge as f32;
    let outside = -0.06;
    // Bottom edge, left to right.
    for i in 0..per_edge {
        out.push((step * (i as f32 + 0.5), 1.0 - outside));
    }
    // Right edge, bottom to top.
    for i in 0..per_edge {
        out.push((1.0 - outside, 1.0 - step * (i as f32 + 0.5)));
    }
    // Top edge, right to left.
    for i in 0..per_edge {
        out.push((1.0 - step * (i as f32 + 0.5), outside));
    }
    // Left edge, top to bottom.
    for i in 0..(n - per_edge * 3) {
        out.push((outside, step * (i as f32 + 0.5)));
    }
    out
}

/// The transform from screen coordinates into the photograph.
fn screen_to_image() -> Homography {
    Homography::from_correspondences(
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
        SCREEN_QUAD,
    )
    .expect("the quad should be solvable")
}

/// Render one photograph.
///
/// `missing` names LEDs the camera cannot see, which is what an LED behind a
/// monitor stand looks like.
fn render(step: &Step, positions: &[(f32, f32)], missing: &[usize], noise: u8) -> Frame {
    let mut luma = vec![8u8; (WIDTH * HEIGHT) as usize];

    // A little fixed pattern noise, so the thresholds are not tested against a
    // perfectly clean image.
    if noise > 0 {
        for (i, pixel) in luma.iter_mut().enumerate() {
            *pixel = pixel.saturating_add(((i * 2654435761) % (noise as usize + 1)) as u8);
        }
    }

    let to_image = screen_to_image();
    for (index, (u, v)) in positions.iter().enumerate() {
        if missing.contains(&index) || !step.lights(index) {
            continue;
        }
        let (x, y) = to_image.map(*u, *v);
        draw_dot(&mut luma, x, y, 3.0, 230);
    }
    Frame::new(luma, WIDTH, HEIGHT)
}

fn draw_dot(luma: &mut [u8], cx: f32, cy: f32, radius: f32, value: u8) {
    let r = radius.ceil() as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if (dx * dx + dy * dy) as f32 > radius * radius {
                continue;
            }
            let x = cx.round() as i32 + dx;
            let y = cy.round() as i32 + dy;
            if x < 0 || y < 0 || x >= WIDTH as i32 || y >= HEIGHT as i32 {
                continue;
            }
            luma[y as usize * WIDTH as usize + x as usize] = value;
        }
    }
}

fn photograph(n: usize, missing: &[usize], noise: u8) -> Vec<Frame> {
    let positions = ground_truth(n);
    plan(n)
        .iter()
        .map(|step| render(step, &positions, missing, noise))
        .collect()
}

// --- the sequence --------------------------------------------------------

#[test]
fn the_sequence_is_logarithmic_in_the_led_count() {
    // This is what makes it practical: sixty LEDs is eight photographs, not
    // sixty.
    assert_eq!(plan(60).len(), 2 + 6, "64 needs six bits plus two brackets");
    assert_eq!(plan(300).len(), 2 + 9);
    assert_eq!(bits_needed(1), 1);
    assert_eq!(bits_needed(64), 7, "the code carries index plus one");
    assert_eq!(bits_needed(0), 0);
}

#[test]
fn the_first_led_is_lit_in_the_first_bit_frame() {
    // Index plus one is what saves LED zero from being invisible.
    assert!(Step::Bit(0).lights(0), "index 0 encodes as 1");
    assert!(!Step::Bit(1).lights(0));
    assert!(Step::Bit(1).lights(1), "index 1 encodes as 2");
    assert!(!Step::AllOff.lights(0));
    assert!(Step::AllOn.lights(999));
}

// --- detection -----------------------------------------------------------

#[test]
fn every_led_is_found_in_the_lit_frame() {
    let positions = ground_truth(40);
    let off = render(&Step::AllOff, &positions, &[], 0);
    let on = render(&Step::AllOn, &positions, &[], 0);
    let blobs = detect_blobs(&off, &on, &DetectSettings::default()).unwrap();
    assert_eq!(blobs.len(), 40, "one blob per LED");
}

#[test]
fn specks_and_floodlights_are_ignored() {
    let positions = ground_truth(8);
    let off = render(&Step::AllOff, &positions, &[], 0);
    let mut on = render(&Step::AllOn, &positions, &[], 0);

    // A single hot pixel, and a large bright rectangle standing in for a
    // window or a reflection.
    on.luma[100 * WIDTH as usize + 20] = 255;
    for y in 400..470 {
        for x in 20..200 {
            on.luma[y * WIDTH as usize + x] = 240;
        }
    }

    let blobs = detect_blobs(
        &off,
        &on,
        &DetectSettings {
            min_area: 4,
            max_area: 2_000,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(blobs.len(), 8, "only the LEDs should survive: {blobs:?}");
}

// --- the whole pipeline --------------------------------------------------

#[test]
fn the_chain_order_comes_back_exactly() {
    let n = 40;
    let frames = photograph(n, &[], 0);
    let discovery = discover(n, &frames, &DetectSettings::default()).unwrap();

    assert_eq!(discovery.found, n, "every LED should be numbered");
    assert!(
        discovery.unassigned.is_empty(),
        "{:?}",
        discovery.unassigned
    );
    assert_eq!(discovery.coverage(), 1.0);

    // Every LED should have landed where it was drawn.
    let to_image = screen_to_image();
    for (index, truth) in ground_truth(n).iter().enumerate() {
        let (want_x, want_y) = to_image.map(truth.0, truth.1);
        let (got_x, got_y) = discovery.positions[index].expect("should have a position");
        assert!(
            (got_x - want_x).abs() < 1.5 && (got_y - want_y).abs() < 1.5,
            "LED {index} landed at {got_x},{got_y} instead of {want_x},{want_y}"
        );
    }
}

#[test]
fn it_still_works_with_a_noisy_camera() {
    let n = 32;
    let frames = photograph(n, &[], 24);
    let discovery = discover(n, &frames, &DetectSettings::default()).unwrap();
    assert_eq!(discovery.found, n, "noise should not lose an LED");
}

#[test]
fn an_led_the_camera_cannot_see_is_reported_rather_than_guessed() {
    // Behind a monitor stand is the usual reason.
    let n = 24;
    let hidden = [5usize, 6, 17];
    let frames = photograph(n, &hidden, 0);
    let discovery = discover(n, &frames, &DetectSettings::default()).unwrap();

    assert_eq!(discovery.found, n - hidden.len());
    for index in hidden {
        assert!(
            discovery.positions[index].is_none(),
            "LED {index} was hidden and must not be invented"
        );
    }
    assert!(discovery.coverage() > 0.8);
}

#[test]
fn the_wrong_number_of_photographs_is_refused() {
    let frames = photograph(16, &[], 0);
    let err = discover(16, &frames[..3], &DetectSettings::default()).unwrap_err();
    assert!(err.to_string().contains("frames"), "{err}");
}

// --- the layout ----------------------------------------------------------

#[test]
fn the_layout_puts_each_led_on_the_edge_it_was_photographed_on() {
    let n = 40;
    let frames = photograph(n, &[], 0);
    let discovery = discover(n, &frames, &DetectSettings::default()).unwrap();

    let screen = Homography::from_screen_corners(SCREEN_QUAD).expect("solvable");
    let layout = build_layout(&discovery, &screen, "DISPLAY1", 0.12);

    assert_eq!(layout.leds.len(), n);
    let per_edge = n / 4;

    // The ground truth walks the bottom edge, then the right, then the top,
    // then the left. The layout should agree without anyone saying so.
    let centre = |i: usize| layout.leds[i].rect.centre();

    let bottom = centre(per_edge / 2);
    assert!(
        bottom.1 > 0.8,
        "the first quarter should be at the bottom: {bottom:?}"
    );

    let right = centre(per_edge + per_edge / 2);
    assert!(
        right.0 > 0.8,
        "the second quarter should be on the right: {right:?}"
    );

    let top = centre(per_edge * 2 + per_edge / 2);
    assert!(
        top.1 < 0.2,
        "the third quarter should be at the top: {top:?}"
    );

    let left = centre(per_edge * 3 + per_edge / 2);
    assert!(
        left.0 < 0.2,
        "the last quarter should be on the left: {left:?}"
    );
}

#[test]
fn the_bottom_edge_runs_the_way_it_was_wound() {
    let n = 40;
    let frames = photograph(n, &[], 0);
    let discovery = discover(n, &frames, &DetectSettings::default()).unwrap();
    let screen = Homography::from_screen_corners(SCREEN_QUAD).unwrap();
    let layout = build_layout(&discovery, &screen, "DISPLAY1", 0.12);

    // Ground truth winds the bottom edge left to right, so x has to increase.
    let first = layout.leds[0].rect.centre().0;
    let last = layout.leds[n / 4 - 1].rect.centre().0;
    assert!(
        last > first + 0.5,
        "the bottom edge should run left to right: {first} then {last}"
    );
}

#[test]
fn a_hidden_led_stays_in_the_chain_but_dark() {
    // Removing it would shift every LED after it by one, which is the worst
    // possible outcome.
    let n = 24;
    let frames = photograph(n, &[7], 0);
    let discovery = discover(n, &frames, &DetectSettings::default()).unwrap();
    let screen = Homography::from_screen_corners(SCREEN_QUAD).unwrap();
    let layout = build_layout(&discovery, &screen, "DISPLAY1", 0.12);

    assert_eq!(layout.leds.len(), n, "the chain keeps its length");
    assert!(!layout.leds[7].enabled, "the hidden one is disabled");
    assert!(layout.leds[8].enabled, "and the next one is not shifted");
    for (i, led) in layout.leds.iter().enumerate() {
        assert_eq!(led.index, i as u32, "indices stay contiguous");
    }
}

// --- homography ----------------------------------------------------------

#[test]
fn a_square_screen_maps_to_the_unit_square() {
    let h = Homography::from_screen_corners([
        (100.0, 100.0),
        (300.0, 100.0),
        (300.0, 200.0),
        (100.0, 200.0),
    ])
    .unwrap();

    let (x, y) = h.map(100.0, 100.0);
    assert!(x.abs() < 1e-4 && y.abs() < 1e-4, "{x},{y}");
    let (x, y) = h.map(300.0, 200.0);
    assert!((x - 1.0).abs() < 1e-4 && (y - 1.0).abs() < 1e-4, "{x},{y}");
    let (x, y) = h.map(200.0, 150.0);
    assert!(
        (x - 0.5).abs() < 1e-4 && (y - 0.5).abs() < 1e-4,
        "the centre: {x},{y}"
    );
}

#[test]
fn a_perspective_quad_round_trips() {
    let forward = screen_to_image();
    let back = Homography::from_screen_corners(SCREEN_QUAD).unwrap();
    for (u, v) in [(0.25, 0.25), (0.5, 0.5), (0.9, 0.1), (-0.1, 1.1)] {
        let (x, y) = forward.map(u, v);
        let (u2, v2) = back.map(x, y);
        assert!(
            (u - u2).abs() < 1e-3 && (v - v2).abs() < 1e-3,
            "{u},{v} became {u2},{v2}"
        );
    }
}

#[test]
fn degenerate_corners_are_refused_rather_than_producing_nonsense() {
    // Three corners in a line has no solution, and a wrong answer here would
    // put every LED in the wrong place.
    let flat =
        Homography::from_screen_corners([(0.0, 0.0), (100.0, 0.0), (200.0, 0.0), (300.0, 0.0)]);
    assert!(flat.is_none());
}
