use maslight_core::layout::{Corner, Direction, EdgeCounts, Rect};
use maslight_core::{Layout, LedSpec, WizardParams};

fn wizard(counts: EdgeCounts, corner: Corner, direction: Direction) -> Layout {
    Layout::from_wizard(&WizardParams {
        display: String::from("DISPLAY1"),
        counts,
        start_corner: corner,
        direction,
        depth: 0.1,
        ..Default::default()
    })
}

#[test]
fn wizard_produces_exactly_the_requested_number_of_leds() {
    let layout = wizard(
        EdgeCounts {
            top: 20,
            right: 12,
            bottom: 20,
            left: 12,
        },
        Corner::BottomLeft,
        Direction::CounterClockwise,
    );
    assert_eq!(layout.len(), 64);
    // Indices are contiguous and start at zero.
    for (i, led) in layout.leds.iter().enumerate() {
        assert_eq!(led.index, i as u32);
    }
}

#[test]
fn counter_clockwise_from_bottom_left_runs_along_the_bottom_first() {
    let layout = wizard(
        EdgeCounts {
            top: 4,
            right: 2,
            bottom: 4,
            left: 2,
        },
        Corner::BottomLeft,
        Direction::CounterClockwise,
    );
    let first = layout.leds[0].rect.centre();
    // Bottom-left corner: low x, high y (y grows downwards).
    assert!(first.0 < 0.3, "first LED should be on the left: {first:?}");
    assert!(
        first.1 > 0.7,
        "first LED should be at the bottom: {first:?}"
    );

    // The fourth LED is the last of the bottom edge, so it sits on the right.
    let last_bottom = layout.leds[3].rect.centre();
    assert!(last_bottom.0 > 0.7, "bottom edge should run left to right");

    // Then it climbs the right edge.
    let up = layout.leds[4].rect.centre();
    assert!(
        up.0 > 0.7 && up.1 < last_bottom.1,
        "should climb the right edge"
    );
}

#[test]
fn clockwise_from_bottom_left_climbs_the_left_edge_first() {
    let layout = wizard(
        EdgeCounts {
            top: 4,
            right: 2,
            bottom: 4,
            left: 2,
        },
        Corner::BottomLeft,
        Direction::Clockwise,
    );
    let first = layout.leds[0].rect.centre();
    let second = layout.leds[1].rect.centre();
    assert!(first.0 < 0.3, "should start on the left edge");
    assert!(second.1 < first.1, "should climb upwards");
}

#[test]
fn chain_offset_rotates_the_chain_without_changing_its_shape() {
    let base = WizardParams {
        display: String::from("D"),
        counts: EdgeCounts {
            top: 3,
            right: 3,
            bottom: 3,
            left: 3,
        },
        ..Default::default()
    };
    let plain = Layout::from_wizard(&base);
    let rotated = Layout::from_wizard(&WizardParams {
        chain_offset: 2,
        ..base.clone()
    });

    assert_eq!(plain.len(), rotated.len());
    // Physically the LED that used to be at position 0 is now at position 2.
    assert_eq!(plain.leds[0].rect, rotated.leds[2].rect);
}

#[test]
fn reversing_the_chain_flips_the_order() {
    let base = WizardParams {
        display: String::from("D"),
        counts: EdgeCounts {
            top: 2,
            right: 1,
            bottom: 2,
            left: 1,
        },
        ..Default::default()
    };
    let plain = Layout::from_wizard(&base);
    let reversed = Layout::from_wizard(&WizardParams {
        reverse_chain: true,
        ..base
    });
    let n = plain.len();
    assert_eq!(plain.leds[0].rect, reversed.leds[n - 1].rect);
}

#[test]
fn sample_rectangles_stay_inside_the_screen() {
    let layout = wizard(
        EdgeCounts {
            top: 30,
            right: 18,
            bottom: 30,
            left: 18,
        },
        Corner::TopRight,
        Direction::Clockwise,
    );
    for led in &layout.leds {
        let r = led.rect;
        assert!(r.x >= 0.0 && r.y >= 0.0, "{r:?}");
        assert!(r.x + r.w <= 1.0001, "{r:?}");
        assert!(r.y + r.h <= 1.0001, "{r:?}");
        assert!(r.w > 0.0 && r.h > 0.0, "{r:?}");
    }
}

#[test]
fn depth_controls_how_far_the_sample_reaches_into_the_screen() {
    let shallow = Layout::from_wizard(&WizardParams {
        display: String::from("D"),
        counts: EdgeCounts {
            bottom: 4,
            ..Default::default()
        },
        depth: 0.05,
        ..Default::default()
    });
    let deep = Layout::from_wizard(&WizardParams {
        display: String::from("D"),
        counts: EdgeCounts {
            bottom: 4,
            ..Default::default()
        },
        depth: 0.3,
        ..Default::default()
    });
    assert!(deep.leds[0].rect.h > shallow.leds[0].rect.h);
}

#[test]
fn json_round_trip_is_lossless() {
    let layout = wizard(
        EdgeCounts {
            top: 10,
            right: 6,
            bottom: 10,
            left: 6,
        },
        Corner::BottomRight,
        Direction::CounterClockwise,
    );
    let json = layout.to_json().unwrap();
    let back = Layout::from_json(&json).unwrap();
    assert_eq!(layout, back);
}

#[test]
fn disabled_and_effect_only_leds_are_excluded_from_screen_sampling() {
    let mut layout = Layout {
        displays: vec![maslight_core::DisplayRegion::new("D")],
        leds: vec![
            LedSpec::new(0, "D", Rect::new(0.0, 0.0, 0.1, 0.1)),
            LedSpec {
                enabled: false,
                ..LedSpec::new(1, "D", Rect::new(0.1, 0.0, 0.1, 0.1))
            },
            // Empty display id means the LED is driven by effects only.
            LedSpec::new(2, "", Rect::FULL),
        ],
        ..Default::default()
    };
    layout.normalise();

    let sampled: Vec<u32> = layout.leds_for_display("D").map(|l| l.index).collect();
    assert_eq!(sampled, vec![0]);
    assert_eq!(layout.active_displays(), vec![String::from("D")]);
}

#[test]
fn rect_to_pixels_never_produces_an_empty_or_out_of_bounds_box() {
    let tiny = Rect::new(0.999, 0.999, 0.0005, 0.0005);
    let (x0, y0, x1, y1) = tiny.to_pixels(1920, 1080);
    assert!(x1 > x0 && y1 > y0, "{x0},{y0} -> {x1},{y1}");
    assert!(x1 <= 1920 && y1 <= 1080);

    let full = Rect::FULL.to_pixels(1920, 1080);
    assert_eq!(full, (0, 0, 1920, 1080));
}
