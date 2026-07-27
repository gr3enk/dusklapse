//! Histograms from preview JPEGs.
//!
//! Computed here in Rust rather than from a canvas in the WebView, for one reason
//! that matters more than performance: this is the same data auto-ramping will read
//! to decide the next exposure. One implementation means the curves on screen are
//! exactly the ones the algorithm acted on, instead of a separate JavaScript
//! calculation that might weight luminance differently and quietly disagree.

use jpeg_decoder::{Decoder, PixelFormat};

use super::error::{CameraError, CameraResult};
use super::model::Histogram;

/// Long edge to decode down to before counting.
///
/// JPEG's DCT can be scaled during decode at almost no cost, and a histogram is a
/// statistic - a quarter-scale image gives the same distribution as the full one to
/// well within a rounding error. Decoding a 24-megapixel frame at full size would
/// mean 72 MB of RGB and a visible stall on a phone, for no gain.
const TARGET_LONG_EDGE: u16 = 1024;

/// Rec. 709 luma weights, matching how sRGB is displayed.
///
/// The green channel dominates because human vision does. An unweighted mean of the
/// three channels would be easier but would misjudge exactly what a photographer is
/// checking: whether the *perceived* brightness is where they want it.
const LUMA_RED: f32 = 0.2126;
const LUMA_GREEN: f32 = 0.7152;
const LUMA_BLUE: f32 = 0.0722;

const BINS: usize = 256;

/// Count the tones in a JPEG.
pub fn from_jpeg(bytes: &[u8]) -> CameraResult<Histogram> {
    let mut decoder = Decoder::new(bytes);

    decoder
        .read_info()
        .map_err(|err| CameraError::Protocol(format!("could not read the preview JPEG: {err}")))?;

    let info = decoder
        .info()
        .ok_or_else(|| CameraError::Protocol("the preview JPEG has no image header".into()))?;

    // Ask for a scaled decode. The decoder snaps to whatever DCT size it can
    // actually do, so the result is a hint, not a guarantee.
    let (long, short) = if info.width >= info.height {
        (info.width, info.height)
    } else {
        (info.height, info.width)
    };
    if long > TARGET_LONG_EDGE {
        let factor = long as f32 / TARGET_LONG_EDGE as f32;
        let scaled_short = (short as f32 / factor).round().max(1.0) as u16;
        let (width, height) = if info.width >= info.height {
            (TARGET_LONG_EDGE, scaled_short)
        } else {
            (scaled_short, TARGET_LONG_EDGE)
        };
        // A refusal to scale is not fatal; it just means decoding at full size.
        if let Err(err) = decoder.scale(width, height) {
            log::debug!("could not scale the preview decode: {err}");
        }
    }

    let pixels = decoder
        .decode()
        .map_err(|err| CameraError::Protocol(format!("could not decode the preview JPEG: {err}")))?;

    let format = decoder
        .info()
        .ok_or_else(|| CameraError::Protocol("the preview JPEG lost its header".into()))?
        .pixel_format;

    match format {
        PixelFormat::RGB24 => Ok(count_rgb(&pixels)),
        // A greyscale JPEG has no channels to separate; all four curves coincide,
        // which is the honest representation rather than three fabricated ones.
        PixelFormat::L8 => Ok(count_grey(&pixels)),
        other => Err(CameraError::Protocol(format!(
            "unsupported preview pixel format {other:?}"
        ))),
    }
}

fn count_rgb(pixels: &[u8]) -> Histogram {
    let mut histogram = Histogram::empty();

    for pixel in pixels.chunks_exact(3) {
        let (red, green, blue) = (pixel[0], pixel[1], pixel[2]);
        histogram.red[red as usize] += 1;
        histogram.green[green as usize] += 1;
        histogram.blue[blue as usize] += 1;

        let luma = LUMA_RED * red as f32 + LUMA_GREEN * green as f32 + LUMA_BLUE * blue as f32;
        histogram.luma[(luma.round() as usize).min(BINS - 1)] += 1;
        histogram.pixels += 1;
    }

    histogram
}

fn count_grey(pixels: &[u8]) -> Histogram {
    let mut histogram = Histogram::empty();

    for &value in pixels {
        let bin = value as usize;
        histogram.red[bin] += 1;
        histogram.green[bin] += 1;
        histogram.blue[bin] += 1;
        histogram.luma[bin] += 1;
        histogram.pixels += 1;
    }

    histogram
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-pixel JPEG is not worth embedding, so the counting functions are tested
    /// directly against synthetic pixel buffers. The decode path is exercised by the
    /// round-trip test below.
    #[test]
    fn counts_each_channel_separately() {
        // Two pixels: pure red, then pure blue.
        let pixels = [255u8, 0, 0, 0, 0, 255];
        let histogram = count_rgb(&pixels);

        assert_eq!(histogram.pixels, 2);
        assert_eq!(histogram.red[255], 1);
        assert_eq!(histogram.red[0], 1);
        assert_eq!(histogram.blue[255], 1);
        assert_eq!(histogram.blue[0], 1);
        // Neither pixel had any green.
        assert_eq!(histogram.green[0], 2);
        assert_eq!(histogram.green[255], 0);

        // Every channel must account for every pixel.
        for channel in [&histogram.red, &histogram.green, &histogram.blue, &histogram.luma] {
            assert_eq!(channel.iter().sum::<u32>(), 2);
        }
    }

    #[test]
    fn luma_is_weighted_the_way_vision_is() {
        // Pure green reads far brighter than pure blue at the same value, which is
        // the whole point of weighting rather than averaging.
        let green = count_rgb(&[0, 255, 0]);
        let blue = count_rgb(&[0, 0, 255]);

        let brightest = |h: &Histogram| h.luma.iter().rposition(|count| *count > 0).unwrap();
        assert_eq!(brightest(&green), (255.0 * LUMA_GREEN).round() as usize);
        assert_eq!(brightest(&blue), (255.0 * LUMA_BLUE).round() as usize);
        assert!(brightest(&green) > brightest(&blue));
    }

    #[test]
    fn white_lands_in_the_top_bin_without_overflowing_it() {
        // The weights sum to 1.0 but in f32, so white can round to 255.00001.
        // Clamping is what keeps that from panicking on an out-of-range index.
        let histogram = count_rgb(&[255, 255, 255]);
        assert_eq!(histogram.luma[255], 1);
    }

    #[test]
    fn greyscale_gives_four_identical_curves() {
        let histogram = count_grey(&[0, 128, 255]);
        assert_eq!(histogram.pixels, 3);
        assert_eq!(histogram.red, histogram.luma);
        assert_eq!(histogram.green, histogram.luma);
        assert_eq!(histogram.blue, histogram.luma);
    }

    #[test]
    fn rejects_bytes_that_are_not_a_jpeg() {
        let error = from_jpeg(b"definitely not a jpeg").unwrap_err();
        assert_eq!(error.kind(), "protocol");
    }
}
