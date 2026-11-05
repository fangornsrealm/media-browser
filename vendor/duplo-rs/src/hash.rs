use image::{self, Pixel};

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Hash {
    pub matrix: crate::haar::CoefMatrix,
    // Thresholds contains the coefficient threholds. If you discard all
    // coefficients with abs(coef) < threshold, you end up with TopCoefs
    // coefficients.
    pub thresholds: crate::haar::Coef,

    // Ratio is image width / image height or 0 if height is 0.
    pub ratio: f64,

    // DHash is a 128 bit vector where each bit value depends on the monotonicity
    // of two adjacent pixels. The first 64 bits are based on a 8x8 version of
    // the Y colour channel. The other two 32 bits are each based on a 8x4 version
    // of the Cb, and Cr colour channel, respectively.
    pub dhash: Vec<u64>,

    // Histogram is histogram quantized into 64 bits (32 for Y and 16 each for
    // Cb and Cr). A bit is set to 1 if the intensity's occurence count is large
    // than the median (for that colour channel) and set to 0 otherwise.
    pub histogram: u64,

    // HistoMax is the maximum value of the histogram (for each channel Y, Cb,
    // and Cr).
    pub histo_max: Vec<f32>,
}

impl Hash {
    pub fn new() -> Self {
        let v = Hash {
            ..Default::default()
        };
        v
    }

    pub fn from(
        matrix: crate::haar::CoefMatrix,
        thresholds: crate::haar::Coef,
        ratio: f64,
        dhash: Vec<u64>,
        histogram: u64,
        histo_max: Vec<f32>,
    ) -> Self {
        let mut v = Hash {
            ..Default::default()
        };
        v.matrix = matrix;
        v.thresholds = thresholds;
        v.ratio = ratio;
        v.dhash = dhash;
        v.histogram = histogram;
        v.histo_max = histo_max;
        v
    }

    pub fn encode(&self, to: &mut Vec<u8>) {
        self.matrix.encode(to);
        self.thresholds.encode(to);
        crate::marshal::store_f64(self.ratio, to);
        crate::marshal::store_vec_u64(&self.dhash, to);
        crate::marshal::store_u64(self.histogram, to);
        crate::marshal::store_vec_f32(&self.histo_max, to);
    }

    pub fn decode(&mut self, from: &mut std::io::Cursor<Vec<u8>>) {
        self.matrix.decode(from);
        self.thresholds.decode(from);
        if let Some(r) = crate::marshal::restore_f64(from) {
            self.ratio = r;
        } else {
            return;
        }
        self.dhash.extend(crate::marshal::restore_vec_u64(from));
        if let Some(r) = crate::marshal::restore_u64(from) {
            self.histogram = r;
        } else {
            return;
        }
        self.histo_max.extend(crate::marshal::restore_vec_f32(from));
    }

    /// Compact encoding: everything *except* the large `CoefMatrix`.
    /// This is the format persisted in the SQLite BLOB — the matrix is
    /// only needed to derive the significant bucket locations at `add()`
    /// time and is dead weight thereafter (~400 KB/screenshot).
    pub fn encode_compact(&self, to: &mut Vec<u8>) {
        self.thresholds.encode(to);
        crate::marshal::store_f64(self.ratio, to);
        crate::marshal::store_vec_u64(&self.dhash, to);
        crate::marshal::store_u64(self.histogram, to);
        crate::marshal::store_vec_f32(&self.histo_max, to);
    }

    /// Decode a compact hash (no matrix). The `matrix` field is left empty;
    /// callers that need it (e.g. re-deriving bucket locations) should
    /// decode from the full format or recompute via `create_hash`.
    pub fn decode_compact(&mut self, from: &mut std::io::Cursor<Vec<u8>>) {
        self.thresholds.decode(from);
        if let Some(r) = crate::marshal::restore_f64(from) {
            self.ratio = r;
        } else {
            return;
        }
        self.dhash.extend(crate::marshal::restore_vec_u64(from));
        if let Some(r) = crate::marshal::restore_u64(from) {
            self.histogram = r;
        } else {
            return;
        }
        self.histo_max.extend(crate::marshal::restore_vec_f32(from));
    }
}

impl Default for Hash {
    fn default() -> Hash {
        Hash {
            matrix: crate::haar::CoefMatrix::new(),
            thresholds: crate::haar::Coef::new(),
            ratio: 0.0,
            dhash: Vec::new(),
            histogram: 0,
            histo_max: Vec::new(),
        }
    }
}

/// CreateHash calculates and returns the visual hash of the provided image as
/// well as a resized version of it (ImageScale x ImageScale) which may be
/// ignored if not needed anymore.
pub fn create_hash(img: &image::RgbaImage) -> (Hash, image::RgbaImage) {
    let mut h = Hash {
        ..Default::default()
    };
    let mut smallimg = image::RgbaImage::new(1, 1);
    if img.height() == 0 {
        return (h, smallimg);
    }
    h.ratio = (img.width() / img.height()) as f64;
    smallimg = image::imageops::resize(
        img,
        crate::store::IMAGESCALE,
        crate::store::IMAGESCALE,
        image::imageops::FilterType::Lanczos3,
    );
    // Then perform a 2D Haar Wavelet transform.
    h.matrix = crate::haar::transform(&smallimg);
    // Find the kth largest coefficients for each colour channel.
    h.thresholds = coef_thresholds(&mut h.matrix.coefs, crate::store::TOPCOEFS);
    // Create the dHash bit vector.
    h.dhash = dhash(&smallimg);
    (h.histogram, h.histo_max) = histogram(&smallimg);

    (h, smallimg)
}

/// coefThreshold returns, for the given coefficients, the kth largest absolute
/// value. Only the nth element in each Coef is considered. If you discard all
/// values v with abs(v) < threshold, you will end up with k values.
pub fn coef_threshold(coefs: &Vec<crate::haar::Coef>, k: i32, n: usize) -> f64 {
    if coefs.len() == 0 {
        return 0.0;
    }
    let mut v = Vec::new();
    for i in 0..coefs.len() {
        v.push(coefs[i].c[n]);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.len() <= k as usize {
        // Not enough values to find the kth-largest: return the minimum.
        // (v is already sorted ascending from the sort above.)
        return v[0];
    } else {
        // kth-largest == (len-k)th smallest, i.e. index (len-k-1).
        // `select_nth_unstable` places it in its final sorted position in
        // expected O(n) time without sorting the rest of the vector.
        let index = v.len() - k as usize - 1;
        v.select_nth_unstable_by(index, |a, b| a.partial_cmp(b).unwrap());
        return v[index];
    }
}

/// coefThresholds returns, for the given coefficients, the kth largest absolute
/// values per colour channel. If you discard all values v with
/// abs(v) < threshold, you will end up with k values.
pub fn coef_thresholds(coefs: &mut Vec<crate::haar::Coef>, k: i32) -> crate::haar::Coef {
    let mut thresholds = crate::haar::Coef::new();
    if coefs.len() == 0 {
        return thresholds;
    }
    for i in 0..thresholds.c.len() {
        thresholds.c[i] = coef_threshold(coefs, k, i);
    }
    thresholds
}

/// Decide whether a single wavelet coefficient is "significant" enough to be
/// indexed and to contribute to a similarity score.
///
/// The threshold is the kth-largest magnitude for the channel; coefficients
/// whose magnitude is at or below the threshold are considered noise and are
/// ignored.
///
/// This is the single source of truth for the significance test, shared by the
/// image store and the video store. It is deliberately `<=` (not `<`) so that a
/// coefficient whose magnitude is *exactly* the threshold is excluded, and —
/// critically — so that a **zero** coefficient is *never* treated as
/// significant, even in the degenerate case where a channel's threshold is
/// `0.0` (which happens for very flat frames where 40+ coefficients round to
/// zero). With `<`, a zero coefficient would slip through (`0.0 < 0.0` is
/// false), polluting the buckets with coefficients that carry no visual
/// information and producing spurious similarity.
pub fn coefficient_is_significant(coefficient: f64, threshold: f64) -> bool {
    coefficient.abs() > threshold
}

fn clamp(val: i32) -> u8 {
    match val {
        ref v if *v < 0 => 0,
        ref v if *v > 255 => 255,
        v => v as u8,
    }
}

/// ycbcr returns the YCbCr values for the given colour, converting to them if
/// necessary.
fn ycbcr(color: &image::Rgba<u8>) -> (u8, u8, u8) {
    //let rgb = Vec::from(color.channels());
    let rgb = color.channels();
    let r = i32::from(rgb[0]);
    let g = i32::from(rgb[1]);
    let b = i32::from(rgb[2]);
    let mut yuv = vec![0; 3];
    yuv[0] = clamp((77 * r + 150 * g + 29 * b + 128) >> 8);
    yuv[1] = clamp(((-43 * r - 84 * g + 127 * b + 128) >> 8) + 128);
    yuv[2] = clamp(((127 * r - 106 * g - 21 * b + 128) >> 8) + 128);

    (yuv[0], yuv[1], yuv[2])
}

/// dHash computes a 128 bit vector by comparing adjacent pixels of a downsized
/// version of img. The first 64 bits correspond to a 8x8 version of the Y colour
/// channel. A bit is set to 1 if a pixel value is higher than that of its left
/// neighbour (the first bit is 1 if its colour value is > 0.5). The other two 32
/// bits correspond to the Cb and Cr colour channels, based on a 8x4 version
/// each.
pub fn dhash(img: &image::RgbaImage) -> Vec<u64> {
    let mut bits = Vec::from([0, 0]);
    let scaled = image::imageops::resize(img, 8, 8, image::imageops::FilterType::Lanczos3);
    // scan the thumbnail
    let mut ypos = 0_usize;
    let mut cbpos = 0_usize;
    let mut crpos = 32_usize;
    for y in 0..8 {
        for x in 0..8 {
            let (ytr, cbtr, crtr) = ycbcr(scaled.get_pixel(x, y));
            if x == 0 {
                // The first bit is a rough approximation of the colour value.
                if ytr & 0x80 > 0 {
                    bits[0] |= 1 << ypos;
                    ypos += 1;
                }
                if y & 1 == 0 && y + 1 < scaled.height() {
                    let (_, cbbr, crbr) = ycbcr(scaled.get_pixel(x, y + 1));
                    if (cbbr as u32 + cbtr as u32) >> 1 & 0x80 > 0 {
                        bits[1] |= 1 << cbpos;
                        cbpos += 1;
                    }
                    if (crbr as u32 + crtr as u32) >> 1 & 0x80 > 0 {
                        bits[1] |= 1 << crpos;
                        crpos += 1;
                    }
                }
            } else {
                // Use a rough first derivative for the other bits.
                let (ytl, cbtl, crtl) = ycbcr(scaled.get_pixel(x - 1, y));
                if ytr > ytl {
                    bits[0] |= 1 << ypos;
                    ypos += 1;
                }
                if y & 1 == 0 && y + 1 < scaled.height() {
                    let (_, cbbr, crbr) = ycbcr(scaled.get_pixel(x, y + 1));
                    let (_, cbbl, crbl) = ycbcr(scaled.get_pixel(x - 1, y + 1));
                    if (cbbr as u32 + cbtr as u32) >> 1 > (cbbl as u32 + cbtl as u32) >> 1 {
                        bits[1] |= 1 << cbpos;
                        cbpos += 1;
                    }
                    if (crbr as u32 + crtr as u32) >> 1 > (crbl as u32 + crtl as u32) >> 1 {
                        bits[1] |= 1 << crpos;
                        crpos += 1;
                    }
                }
            }
        }
    }
    bits
}

fn histogram_median(v: &[i32]) -> (usize, f32) {
    let mut sorted = Vec::from(v);
    //sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sorted.sort();
    let length = sorted.len();
    let half = length / 2;
    if length % 2 == 0 {
        return (half, (sorted[half] + sorted[half + 1]) as f32 / 2.0);
    } else {
        return (half, sorted[half] as f32);
    }
}

/// histogram calculates a histogram based on the YCbCr values of img and returns
/// a rough approximation of it in 64 bits. For each colour channel, a bit is
/// set if a histogram value is greater than the median. The Y channel gets 32
/// bits, the Cb and Cr values each get 16 bits.
pub fn histogram(img: &image::RgbaImage) -> (u64, Vec<f32>) {
    let mut bits = 0_u64;
    let mut histo_max = vec![0.0_f32; 64];
    let mut h = vec![0_i32; 64];

    for y in 0..img.height() {
        for x in 0..img.width() {
            let (ytr, cbtr, crtr) = ycbcr(img.get_pixel(x, y));
            let index = ytr as usize >> 3;
            h[index] += 1;
            let index = 32 + cbtr as usize >> 4;
            h[index] += 1;
            let index = 48 + crtr as usize >> 4;
            h[index] += 1;
        }
    }

    // Calculate medians and maximums.
    let (my, ymax) = histogram_median(&h[0..32]);
    let (mcb, cbmax) = histogram_median(&h[32..48]);
    let (mcr, crmax) = histogram_median(&h[48..64]);
    histo_max[0] = ymax;
    histo_max[1] = cbmax;
    histo_max[2] = crmax;

    // Quantize histogram.
    for index in 0..h.len() {
        let value = h[index];
        if index < 32 {
            if value as usize > my {
                bits |= 1 << index
            }
        } else if index < 48 {
            if value as usize > mcb {
                //bits |= 1 << index-32;
                bits |= 1 << index;
            }
        } else {
            if value as usize > mcr {
                //bits |= 1 << index-32;
                bits |= 1 << index;
            }
        }
    }

    (bits, histo_max)
}
