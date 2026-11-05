/// Package haar provides a Haar wavelet function for bitmap images.
use image;
use image::Pixel;

pub const COLOURCHANNELS: u32 = 3;

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Coef {
    pub c: Vec<f64>,
}

impl Coef {
    pub fn add(&mut self, offset: Coef) {
        self.c[0] += offset.c[0];
        self.c[1] += offset.c[1];
        self.c[2] += offset.c[2];
    }

    pub fn subtract(&mut self, offset: Coef) {
        self.c[0] -= offset.c[0];
        self.c[1] -= offset.c[1];
        self.c[2] -= offset.c[2];
    }

    pub fn divide(&mut self, value: f64) {
        let factor = 1.0 / value;
        self.c[0] *= factor;
        self.c[1] *= factor;
        self.c[2] *= factor;
    }

    pub fn new() -> Self {
        let v = Coef {
            ..Default::default()
        };
        v
    }

    pub fn from(r: f64, g: f64, b: f64) -> Self {
        let mut v = Coef {
            ..Default::default()
        };
        v.c[0] += r;
        v.c[1] += g;
        v.c[2] += b;
        v
    }

    pub fn encode(&self, to: &mut Vec<u8>) {
        crate::marshal::store_vec_f64(&self.c, to);
    }

    pub fn decode(&mut self, from: &mut std::io::Cursor<Vec<u8>>) {
        // Clear first so decode is idempotent (a fresh `default()` starts with
        // three zeros, and `extend` alone would leave stale entries behind).
        self.c.clear();
        self.c.extend(crate::marshal::restore_vec_f64(from));
    }
}

impl Default for Coef {
    fn default() -> Coef {
        Coef {
            c: Vec::from([0.0; COLOURCHANNELS as usize]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct CoefMatrix {
    pub coefs: Vec<Coef>,
    pub width: u32,
    pub height: u32,
}

impl CoefMatrix {
    pub fn new() -> Self {
        let v = CoefMatrix {
            ..Default::default()
        };
        v
    }

    pub fn encode(&self, to: &mut Vec<u8>) {
        let s = self.coefs.len();
        crate::marshal::store_usize(s, to);
        for elem in &self.coefs {
            elem.encode(to);
        }
        crate::marshal::store_u32(self.width, to);
        crate::marshal::store_u32(self.height, to);
    }

    pub fn decode(&mut self, from: &mut std::io::Cursor<Vec<u8>>) {
        // Clear first so decode is idempotent (re-decoding the same stream into
        // a non-empty matrix would otherwise duplicate every coefficient).
        self.coefs.clear();
        if let Some(s) = crate::marshal::restore_usize(from) {
            for _i in 0..s {
                let mut elem = Coef {
                    ..Default::default()
                };
                elem.decode(from);
                self.coefs.push(elem);
            }
        }
        if let Some(r) = crate::marshal::restore_u32(from) {
            self.width = r;
        } else {
            return;
        }
        if let Some(r) = crate::marshal::restore_u32(from) {
            self.height = r;
        } else {
            return;
        }
    }
}

impl Default for CoefMatrix {
    fn default() -> CoefMatrix {
        CoefMatrix {
            coefs: Vec::new(),
            width: 0,
            height: 0,
        }
    }
}

/// colorToCoef converts a native Color type into a YIQ Coef. We are using
/// YIQ because we only have weights for them. (Apart from the score weights,
/// the store is built to handle different sized Coef's so any length may be
/// returned.)
pub fn color_to_coef(color: &image::Rgba<u8>) -> Coef {
    let mut coef = Coef {
        ..Default::default()
    };

    // Convert into YIQ. (We may want to convert from YCbCr directly one day.)
    //color.
    //r32, g32, b32, _ := gen.RGBA()
    let rgb = Vec::from(color.channels());
    let ru = rgb[0] as usize;
    let r = ru as f64;
    let gu = rgb[1] as usize;
    let g = gu as f64;
    let bu = rgb[2] as usize;
    let b = bu as f64;
    //r, g, b := float64(r32>>8), float64(g32>>8), float64(b32>>8)

    coef.c[0] = (0.299900 * r + 0.587000 * g + 0.114000 * b) / 0x100 as f64;
    coef.c[1] = (0.595716 * r - 0.274453 * g - 0.321263 * b) / 0x100 as f64;
    coef.c[2] = (0.211456 * r - 0.522591 * g + 0.311135 * b) / 0x100 as f64;
    coef
}

pub fn transform(img: &image::RgbaImage) -> CoefMatrix {
    let mut matrix = CoefMatrix {
        ..Default::default()
    };
    let mut width = img.width();
    let mut height = img.height();
    if width % 2 != 0 {
        width = width | 1;
    }
    if height % 2 != 0 {
        height = height | 1;
    }
    let reservelen: usize = width as usize * height as usize;
    for _ in 0..reservelen {
        matrix.coefs.push(Coef::new());
    }
    matrix.width = width;
    matrix.height = height;

    // Convert colours to coefficients.
    for row in 0..height as usize {
        for column in 0..width as usize {
            matrix.coefs[row * width as usize + column] =
                color_to_coef(img.get_pixel(column as u32, row as u32));
        }
    }

    // Apply 1D Haar transform on rows.
    let mut temp_row = Vec::new();
    for _ in 0..width {
        temp_row.push(Coef {
            ..Default::default()
        });
    }
    for row in 0..height {
        let mut step = width / 2;
        while step >= 1 {
            for column in 0..step {
                let mut high = matrix.coefs[(row * width + 2 * column) as usize].clone();
                let mut low = high.clone();
                let offset = matrix.coefs[(row * width + 2 * column + 1) as usize].clone();
                high.add(offset.clone());
                low.subtract(offset.clone());
                high.divide(2.0_f64.sqrt());
                low.divide(2.0_f64.sqrt());
                temp_row[column as usize] = high;
                temp_row[(column + step) as usize] = low;
            }
            for column in 0..width {
                matrix.coefs[(row * width + column) as usize] = temp_row[column as usize].clone();
            }
            step /= 2;
        }
    }

    // Apply 1D Haar transform on columns.
    let mut temp_column = Vec::new();
    for _ in 0..height {
        temp_column.push(Coef {
            ..Default::default()
        });
    }
    for column in 0..width {
        let mut step = height / 2;
        while step >= 1 {
            for row in 0..step {
                let mut high = matrix.coefs[(2 * row * width + column) as usize].clone();
                let mut low = high.clone();
                let offset = matrix.coefs[((2 * row + 1) * width + column) as usize].clone();
                high.add(offset.clone());
                low.subtract(offset.clone());
                high.divide(2.0_f64.sqrt());
                low.divide(2.0_f64.sqrt());
                temp_column[row as usize] = high;
                temp_column[(row + step) as usize] = low;
            }
            for row in 0..height {
                matrix.coefs[(row * width + column) as usize] = temp_column[row as usize].clone();
            }
            step /= 2;
        }
    }

    matrix
}

#[cfg(test)]

// Whether or not the two coefficients are equal to an epsilon difference.
fn equal(slice1: &Coef, slice2: &Coef) -> bool {
    for index in 0..slice1.c.len() {
        if (slice1.c[index] - slice2.c[index]).abs() > 0.002 {
            return false;
        }
    }
    return true;
}

// Whether or not the two matrices are equal (uses equal() function).
fn _equal_matrices(matrix1: &CoefMatrix, matrix2: &CoefMatrix) -> bool {
    if matrix1.width != matrix2.width {
        return false;
    }
    if matrix1.height != matrix2.height {
        return false;
    }
    if matrix1.coefs.len() != matrix2.coefs.len() {
        return false;
    }
    for index in 0..matrix1.coefs.len() {
        if matrix1.coefs[index].c.len() != matrix2.coefs[index].c.len() {
            return false;
        }
        for coefid in 0..matrix1.coefs[index].c.len() {
            if (matrix1.coefs[index].c[coefid] - matrix2.coefs[index].c[coefid]) > 0.002 {
                return false;
            }
        }
    }
    true
}

// Converts a slice of floats to a Coefs slice as found in a one-value matrix.
fn _floats_to_coefs(floats: &Vec<f64>) -> CoefMatrix {
    let mut coefs = CoefMatrix::new();
    for index in 0..floats.len() {
        coefs.coefs.push(Coef::from(floats[index], 0.0, 0.0));
    }
    coefs
}

// Test coefficients.
#[test]
fn test_coef() {
    let mut coef = Coef::from(1.0, 2.0, 3.0);
    let copy_coef = coef.clone();
    assert_eq!(equal(&copy_coef, &Coef::from(1.0, 2.0, 3.0)), true);

    let offset = Coef::from(2.0, 4.0, 6.0);
    coef.add(offset.clone());
    assert_eq!(equal(&coef, &Coef::from(3.0, 6.0, 9.0)), true);

    coef.subtract(offset);
    assert_eq!(equal(&coef, &Coef::from(1.0, 2.0, 3.0)), true);

    coef.divide(2.0);
    assert_eq!(equal(&coef, &Coef::from(0.5, 1.0, 1.5)), true);
}

// Test the proper RGB-YIQ conversion.
#[test]
fn test_color_conversion() {
    let rgb = image::Rgba::from([64_u8, 0, 128, 255]);
    let coef = color_to_coef(&rgb);
    assert_eq!(
        equal(&coef, &Coef::from(0.131975, -0.0117025, 0.2084315)),
        true
    );
}

// Essentially a 1D Haar Wavelet test.
#[test]
fn test_single_row() {
    // This is a rough approximation to a 4px by 1px YIQ image with pixels
    // .04, .02, .05, .05. Y, I, and Q all have the same value.
    let ret = image::load_from_memory(&[26_u8, 1, 16, 1, 13, 0, 8, 1, 33, 1, 20, 1, 33, 1, 20, 1]);
    if ret.is_ok() {
        let input = ret.unwrap();
        let transopt = input.as_rgba8();
        if transopt.is_some() {
            let output = transform(transopt.unwrap());
            let expected = _floats_to_coefs(&vec![0.08, -0.02, 0.02 / 2.0_f64.sqrt(), 0.0]);
            assert_eq!(_equal_matrices(&output, &expected), true);
        }
    }
}

// Basic 2D Haar Wavelet test.
#[test]
fn test_matrix4x4() {
    // This is a rough approximation to a 4px by 4px YIQ image with consecutive
    // pixels increasing by one each (.01, .02, .03, .04, ..., .16) and Y, I, and
    // Q having the same values.
    let ret = image::load_from_memory(&[
        7, 0, 4, 1, 13, 0, 8, 1, 20, 1, 12, 1, 26, 1, 16, 1, 33, 1, 20, 1, 39, 1, 24, 1, 46, 1, 29,
        1, 53, 2, 33, 1, 59, 2, 37, 1, 66, 2, 41, 1, 72, 2, 45, 1, 79, 2, 49, 1, 85, 3, 53, 1, 92,
        3, 57, 1, 99, 3, 61, 1, 105, 3, 65, 1,
    ]);
    if ret.is_ok() {
        let input = ret.unwrap();
        let transopt = input.as_rgba8();
        if transopt.is_some() {
            let output = transform(transopt.unwrap());
            let expected = _floats_to_coefs(&vec![
                0.34_f64,
                -0.04,
                -2.0_f64.sqrt() / 100.0,
                0.0,
                -0.16,
                0.0,
                0.0,
                0.0,
                -0.04 * 2.0_f64.sqrt(),
                0.0,
                0.0,
                0.0,
                -0.04 * 2.0_f64.sqrt(),
                0.0,
                0.0,
                0.0,
            ]);
            assert_eq!(_equal_matrices(&output, &expected), true);
        }
    }
}
