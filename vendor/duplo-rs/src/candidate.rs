#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Candidate {
    pub id: String,
    // scaleCoef is the scaling function coefficient, the coefficients at index
    // (0,0) of the Haar matrix.
    pub scale_coeff: crate::haar::Coef,
    // ratio is image width / image height.
    pub ratio: f64,
    // The dHash bit vector (see Hash for more information).
    pub dhash: Vec<u64>,
    pub histogram: u64,
    histo_max: Vec<f32>,
}

impl Candidate {
    pub fn new() -> Self {
        let v: Candidate = Candidate {
            ..Default::default()
        };
        v
    }
    pub fn from(id: &str, h: &crate::hash::Hash) -> Self {
        let mut v: Candidate = Candidate {
            ..Default::default()
        };
        v.id = id.to_string();
        v.ratio = h.ratio;
        v.dhash = h.dhash.clone();
        v.histogram = h.histogram;
        v.histo_max = h.histo_max.clone();
        v
    }

    pub fn encode(&self, to: &mut Vec<u8>) {
        crate::marshal::store_string(&self.id, to);
        crate::marshal::store_f64(self.ratio, to);
        crate::marshal::store_vec_u64(&self.dhash, to);
        crate::marshal::store_u64(self.histogram, to);
        crate::marshal::store_vec_f32(&self.histo_max, to);
    }

    pub fn decode(&mut self, from: &mut std::io::Cursor<Vec<u8>>) {
        self.id = crate::marshal::restore_string(from);
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

impl Default for Candidate {
    fn default() -> Candidate {
        Candidate {
            id: String::new(),
            scale_coeff: crate::haar::Coef::new(),
            ratio: 0.0,
            dhash: Vec::new(),
            histogram: 0,
            histo_max: Vec::new(),
        }
    }
}
