//! CSS `color-mix()` in the `srgb` and `lab` spaces, so the diff colors can be derived from the
//! app tokens with the same formulas the web client's stylesheets use.

/// An sRGB color with straight (not premultiplied) alpha, channels in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    /// A color from `0xRRGGBBAA`, the format of `docs/spec/tokens.json`.
    pub const fn hex(rgba: u32) -> Self {
        Self {
            r: ((rgba >> 24) & 0xff) as f32 / 255.,
            g: ((rgba >> 16) & 0xff) as f32 / 255.,
            b: ((rgba >> 8) & 0xff) as f32 / 255.,
            a: (rgba & 0xff) as f32 / 255.,
        }
    }

    /// An opaque color from `0xRRGGBB`.
    pub const fn rgb(rgb: u32) -> Self {
        Self::hex((rgb << 8) | 0xff)
    }

    /// The color as `0xRRGGBBAA`, each channel rounded to 8 bits.
    pub fn to_hex(self) -> u32 {
        let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u32;
        (channel(self.r) << 24) | (channel(self.g) << 16) | (channel(self.b) << 8) | channel(self.a)
    }

    /// Tailwind's `color/NN`: the same color with its alpha scaled by `factor`.
    pub fn alpha(self, factor: f32) -> Self {
        Self {
            a: self.a * factor,
            ..self
        }
    }

    /// `color-mix(in srgb, self weight, other)`.
    pub fn mix_srgb(self, weight: f32, other: Self) -> Self {
        let mixed = mix_premultiplied(
            [self.r, self.g, self.b].map(f64::from),
            f64::from(self.a),
            [other.r, other.g, other.b].map(f64::from),
            f64::from(other.a),
            f64::from(weight),
        );
        Self::from_parts(mixed.0, mixed.1)
    }

    /// `color-mix(in lab, self weight, other)`, interpolating CIE Lab (D50) as CSS does.
    pub fn mix_lab(self, weight: f32, other: Self) -> Self {
        let (lab, alpha) = mix_premultiplied(
            srgb_to_lab(self),
            f64::from(self.a),
            srgb_to_lab(other),
            f64::from(other.a),
            f64::from(weight),
        );
        Self::from_parts(lab_to_srgb(lab), alpha)
    }

    fn from_parts(rgb: [f64; 3], alpha: f64) -> Self {
        let clamp = |value: f64| value.clamp(0., 1.) as f32;
        Self {
            r: clamp(rgb[0]),
            g: clamp(rgb[1]),
            b: clamp(rgb[2]),
            a: clamp(alpha),
        }
    }
}

/// Premultiplied-alpha interpolation used by `color-mix()`; `weight` is the share of `a`.
fn mix_premultiplied(
    a: [f64; 3],
    a_alpha: f64,
    b: [f64; 3],
    b_alpha: f64,
    weight: f64,
) -> ([f64; 3], f64) {
    let alpha = a_alpha * weight + b_alpha * (1. - weight);
    if alpha <= 0. {
        return ([0.; 3], 0.);
    }
    let channel = |ix: usize| (a[ix] * a_alpha * weight + b[ix] * b_alpha * (1. - weight)) / alpha;
    ([channel(0), channel(1), channel(2)], alpha)
}

const D50_WHITE: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
const EPSILON: f64 = 216. / 24389.;
const KAPPA: f64 = 24389. / 27.;

fn srgb_to_lab(color: Rgba) -> [f64; 3] {
    let linear = [color.r, color.g, color.b].map(|channel| {
        let channel = f64::from(channel);
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    let xyz_d65 = multiply(
        [
            [
                0.412_390_799_265_959_34,
                0.357_584_339_383_878,
                0.180_480_788_401_834_3,
            ],
            [
                0.212_639_005_871_510_27,
                0.715_168_678_767_756,
                0.072_192_315_360_733_71,
            ],
            [
                0.019_330_818_715_591_82,
                0.119_194_779_794_625_98,
                0.950_532_152_249_660_7,
            ],
        ],
        linear,
    );
    let xyz_d50 = multiply(
        [
            [
                1.047_929_820_840_548_8,
                0.022_946_793_341_019_088,
                -0.050_192_229_543_135_57,
            ],
            [
                0.029_627_815_688_159_344,
                0.990_434_484_573_249,
                -0.017_073_825_029_385_14,
            ],
            [
                -0.009_243_058_152_591_178,
                0.015_055_144_896_577_895,
                0.751_874_289_958_000_8,
            ],
        ],
        xyz_d65,
    );
    let f = |value: f64| {
        if value > EPSILON {
            value.cbrt()
        } else {
            (KAPPA * value + 16.) / 116.
        }
    };
    let [fx, fy, fz] = [0, 1, 2].map(|ix| f(xyz_d50[ix] / D50_WHITE[ix]));
    [116. * fy - 16., 500. * (fx - fy), 200. * (fy - fz)]
}

fn lab_to_srgb([l, a, b]: [f64; 3]) -> [f64; 3] {
    let fy = (l + 16.) / 116.;
    let fx = a / 500. + fy;
    let fz = fy - b / 200.;
    let x = if fx.powi(3) > EPSILON {
        fx.powi(3)
    } else {
        (116. * fx - 16.) / KAPPA
    };
    let y = if l > KAPPA * EPSILON {
        fy.powi(3)
    } else {
        l / KAPPA
    };
    let z = if fz.powi(3) > EPSILON {
        fz.powi(3)
    } else {
        (116. * fz - 16.) / KAPPA
    };
    let xyz_d50 = [x * D50_WHITE[0], y * D50_WHITE[1], z * D50_WHITE[2]];
    let xyz_d65 = multiply(
        [
            [
                0.955_473_421_488_075,
                -0.023_098_454_948_764_71,
                0.063_259_243_200_570_72,
            ],
            [
                -0.028_369_709_333_863_7,
                1.009_995_398_081_304_1,
                0.021_041_441_191_917_323,
            ],
            [
                0.012_314_014_864_481_998,
                -0.020_507_649_298_898_964,
                1.330_365_926_242_124,
            ],
        ],
        xyz_d50,
    );
    let linear = multiply(
        [
            [
                3.240_969_941_904_522_6,
                -1.537_383_177_570_094,
                -0.498_610_760_293_003_4,
            ],
            [
                -0.969_243_636_280_879_6,
                1.875_967_501_507_720_2,
                0.041_555_057_407_175_59,
            ],
            [
                0.055_630_079_696_993_66,
                -0.203_976_958_888_976_52,
                1.056_971_514_242_878_6,
            ],
        ],
        xyz_d65,
    );
    linear.map(|channel| {
        let sign = channel.signum();
        let channel = channel.abs();
        sign * if channel <= 0.003_130_8 {
            channel * 12.92
        } else {
            1.055 * channel.powf(1. / 2.4) - 0.055
        }
    })
}

fn multiply(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

#[cfg(test)]
mod tests {
    //! Failure modes: wrong weight direction (weight is the share of `self`), mixing in linear
    //! instead of gamma-encoded sRGB, Lab with the wrong white point (CSS uses D50), ignoring
    //! alpha premultiplication when one side is translucent, and channel overflow on round trip.
    //! Expected values come from culori 4.0.2 (the tool that generated tokens.json).
    use super::*;

    fn hex(color: Rgba) -> u32 {
        color.to_hex()
    }

    #[test]
    fn srgb_mix_weight_is_share_of_self() {
        assert_eq!(
            hex(Rgba::rgb(0x1b1b1b).mix_srgb(0.92, Rgba::rgb(0x161616))),
            0x1b1b1bff
        );
        assert_eq!(
            hex(Rgba::rgb(0x202020).mix_srgb(0.1, Rgba::rgb(0x000000))),
            0x030303ff
        );
        assert_eq!(
            hex(Rgba::rgb(0x161616).mix_srgb(0.92, Rgba::rgb(0x00bc7d))),
            0x14231eff
        );
    }

    #[test]
    fn lab_mix_matches_css() {
        assert_eq!(
            hex(Rgba::rgb(0x1a1a1a).mix_lab(0.8, Rgba::rgb(0x14231e))),
            0x191c1bff
        );
        assert_eq!(
            hex(Rgba::rgb(0xfafafa).mix_lab(0.65, Rgba::rgb(0x1a1a1a))),
            0xa3a3a3ff
        );
        assert_eq!(
            hex(Rgba::rgb(0xffffff).mix_lab(0.88, Rgba::rgb(0xffeeef))),
            0xfffdfdff
        );
    }

    #[test]
    fn translucent_mixes_premultiply() {
        assert_eq!(
            hex(Rgba::hex(0xffffff0f).mix_srgb(0.5, Rgba::rgb(0x161616))),
            0x23232387
        );
        assert_eq!(
            hex(Rgba::hex(0xff000080).mix_lab(0.5, Rgba::rgb(0x0000ff))),
            0xa400afc0
        );
    }

    #[test]
    fn round_trip_is_stable() {
        for value in [0x000000ff, 0xffffffff, 0x07c480ff, 0xff2e3fff, 0x009fffff] {
            assert_eq!(hex(Rgba::hex(value).mix_lab(1.0, Rgba::rgb(0))), value);
        }
        assert_eq!(hex(Rgba::hex(0xffffff0f).alpha(0.5)), 0xffffff08);
    }
}
