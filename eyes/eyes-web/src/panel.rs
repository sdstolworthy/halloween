//! What the LEDs actually show for a rendered colour, for the emulator's optional display stage.
//!
//! The HUB75 driver maps each 8-bit channel through a CIE 1931 lightness table to a 16-bit PWM duty and
//! keeps the top `PIXEL_COLOR_DEPTH_BITS` (8 by default) bits, so low values go dark or vanish. The LEDs'
//! light is linear in that duty, while a monitor decodes its input with the sRGB curve, so to show the
//! same light the duty is re-encoded as sRGB. Panel brightness (a global dimmer) is not modelled.

/// `lumConvTab` from mrcodetastic/ESP32-HUB75-MatrixPanel-DMA 3.0.11, `ESP32-HUB75-MatrixPanel-I2S-DMA.h`.
const LUM_CONV_TAB: [u16; 256] = [
    0, 27, 56, 84, 113, 141, 170, 198, 227, 255, 284, 312, 340, 369, 397, 426, 454, 483, 511, 540,
    568, 597, 626, 657, 688, 720, 754, 788, 824, 860, 898, 936, 976, 1017, 1059, 1102, 1146, 1191,
    1238, 1286, 1335, 1385, 1436, 1489, 1543, 1598, 1655, 1713, 1772, 1833, 1895, 1958, 2023, 2089,
    2156, 2225, 2296, 2368, 2441, 2516, 2592, 2670, 2750, 2831, 2914, 2998, 3084, 3171, 3260, 3351,
    3443, 3537, 3633, 3731, 3830, 3931, 4034, 4138, 4245, 4353, 4463, 4574, 4688, 4803, 4921, 5040,
    5161, 5284, 5409, 5536, 5665, 5796, 5929, 6064, 6201, 6340, 6482, 6625, 6770, 6917, 7067, 7219,
    7372, 7528, 7687, 7847, 8010, 8174, 8341, 8511, 8682, 8856, 9032, 9211, 9392, 9575, 9761, 9949,
    10139, 10332, 10527, 10725, 10925, 11127, 11332, 11540, 11750, 11963, 12178, 12395, 12616,
    12839, 13064, 13292, 13523, 13757, 13993, 14231, 14473, 14717, 14964, 15214, 15466, 15722,
    15980, 16240, 16504, 16771, 17040, 17312, 17587, 17865, 18146, 18430, 18717, 19006, 19299,
    19595, 19894, 20195, 20500, 20808, 21119, 21433, 21750, 22070, 22393, 22720, 23049, 23382,
    23718, 24057, 24400, 24745, 25094, 25446, 25802, 26160, 26522, 26888, 27256, 27628, 28004,
    28382, 28765, 29150, 29539, 29932, 30328, 30727, 31130, 31536, 31946, 32360, 32777, 33197,
    33622, 34049, 34481, 34916, 35354, 35797, 36243, 36692, 37146, 37603, 38064, 38528, 38996,
    39469, 39945, 40424, 40908, 41395, 41886, 42382, 42881, 43383, 43890, 44401, 44916, 45434,
    45957, 46484, 47014, 47549, 48088, 48630, 49177, 49728, 50283, 50842, 51406, 51973, 52545,
    53120, 53700, 54284, 54873, 55465, 56062, 56663, 57269, 57878, 58492, 59111, 59733, 60360,
    60992, 61627, 62268, 62912, 63561, 64215, 64873, 65535,
];

const COLOUR_DEPTH_BITS: u32 = 8;

/// Rendered channel value -> sRGB value that shows the same light the panel emits.
pub fn display_lut() -> [u8; 256] {
    core::array::from_fn(|value| {
        let duty = LUM_CONV_TAB[value] >> (16 - COLOUR_DEPTH_BITS);
        let linear = f32::from(duty) / ((1 << COLOUR_DEPTH_BITS) - 1) as f32;
        (srgb_encode(linear) * 255.0).round() as u8
    })
}

fn srgb_encode(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_extremes() {
        let lut = display_lut();
        assert_eq!((lut[0], lut[255]), (0, 255));
    }

    #[test]
    fn is_monotonic() {
        let lut = display_lut();
        assert!(lut.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn the_dimmest_values_go_dark_on_the_panel() {
        // Duty below 1/256 truncates to zero at 8-bit colour depth.
        assert_eq!(display_lut()[9], 0);
    }

    #[test]
    fn mid_grey_is_darker_than_on_a_monitor() {
        let lut = display_lut();
        assert!(lut[128] < 128 && lut[128] > 64, "{}", lut[128]);
    }
}
