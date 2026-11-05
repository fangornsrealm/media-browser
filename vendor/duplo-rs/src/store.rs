//use array2d::{Array2D, Error};
use std::io::{Read, Write};
use log;

static WEIGHTS: [[f64;6];3] = [[5.00_f64, 0.83, 1.01, 0.52, 0.47, 0.30], 
                [19.21, 1.26, 0.44, 0.53, 0.28, 0.14],
                [34.37, 0.36, 0.45, 0.14, 0.18, 0.27]];
pub const IMAGESCALE: u32 = 128;
pub const INDICESMAX: u32 = 98400;
pub static TOPCOEFS: i32 = 40;
static WEIGHTSUMS: [f64;6] = [58.58 as f64, 2.45, 1.9, 1.19, 0.93, 0.71];
pub static CTRL_C_PRESSED: bool = false;

/// Store is a data structure that holds references to images. It holds visual
/// hashes and references to the images but the images themselves are not held
/// in the data structure.
///
/// A general limit to the store is that it can hold no more than 4,294,967,295
/// images. This is to save RAM space but may be easy to extend by modifying its
/// data structures to hold uint64 indices instead of uint32 indices.
///
/// candidates contains all images in the store or, rather, the candidates for a query.
/// 
/// All IDs in the store, mapping to candidate indices.
/// 
/// indices  contains references to the images in the store. It is a slice
/// of slices which contains image indices (into the "candidates" slice).
/// Use the following formula to access an index slice:
///
///	store.indices[sign*ImageScale*ImageScale*haar.ColourChannels + coefIdx*haar.ColourChannels + channel]
///
/// where the variables are as follows:
///
///	sign: Either 0 (positive) or 1 (negative)
///	coefIdx: The index of the coefficient (from 0 to (ImageScale*ImageScale)-1)
///	channel: The colour channel (from 0 to haar.ColourChannels-1)
///     
/// sentivity (Hamming distance threshold) for the perceptual hashes. 
/// Larger values will allow more images to be seen as *similar*
/// 
/// modified tells Whether this store was modified since it was loaded/created.
///
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Store {
	//sync.RWMutex,

	pub candidates: Vec<crate::candidate::Candidate>,

	pub ids: std::collections::BTreeMap<String, usize>,

	pub indices: Vec<Vec<u32>>,

    sensitivity: f64,

    pub modified: bool,
}

impl Default for Store {
    fn default() -> Store {
        Store {
            candidates: Vec::new(),
            ids: std::collections::BTreeMap::new(),
            indices: Vec::new(),
            sensitivity: -60.0,
            modified: false,
       }
    }
}

impl Store {
    pub fn new(sensitivity: f64) -> Self {
        let mut v = Store {..Default::default()};
        v.sensitivity = sensitivity;
        for _ in 0..INDICESMAX {
            // prefill the outer Vec so we can directly access the inner Vec
            v.indices.push(Vec::new()); 
        } 
        v
    }

    pub fn has(&self, id: &str) -> bool {
        if self.ids.contains_key(id) {
            return true;
        }
        false
    }

    /// Add adds an image (via its hash) to the store. The provided ID is the value
    /// that will be returned as the result of a similarity query. If an ID is
    /// already in the store, it is not added again.
    pub fn add(&mut self, id: &str, hash: &crate::hash::Hash) {
        if self.ids.contains_key(id) {
            return;
        }
        let index = self.candidates.len();
        self.candidates.push(crate::candidate::Candidate::from(id, hash));
        self.ids.insert(id.to_string(), index);
        if hash.matrix.coefs.len() < 2 {
            return;
        }
        for coefindex in 1..hash.matrix.coefs.len() {
            let coef = &hash.matrix.coefs[coefindex];
            for colorindex in 0..coef.c.len() {
                let colorcoef = coef.c[colorindex];
                if !crate::hash::coefficient_is_significant(colorcoef, hash.thresholds.c[colorindex]) {
                    continue;
                }
                let mut sign = 0;
                if colorcoef < 0.0 {
                    sign = 1;
                }
                let location = sign * IMAGESCALE *IMAGESCALE * crate::haar::COLOURCHANNELS 
                                    + coefindex as u32 * crate::haar::COLOURCHANNELS + colorindex as u32;
                self.indices[location as usize].push(index as u32);
            }
        }
        self.modified = true;
    }

    pub fn ids(&self) -> Vec<String> {
        let mut v = Vec::new();
        for (id, _) in self.ids.iter() {
            v.push(id.clone());
        }
        v
    }

    /// Delete removes an image from the store so it will not be returned during a
    /// query anymore. Note that the candidate slot still remains occupied but its
    /// index will be removed from all index lists. This also means that Size() will
    /// not decrease. This is an expensive operation. If the provided ID could not be
    /// found, nothing happens.
    pub fn delete(&mut self, id: &str) {
        if !self.ids.contains_key(id) {
            return;
        }
        // Get the index.
        let index = self.ids[id];
        self.modified = true;
        // clear the entry in the candidates list without deleting it
        self.candidates[index] = crate::candidate::Candidate::new();
        self.ids.remove(id);
        // Remove from all index lists.
        for i in 0..self.indices.len() {
            for j in (0..self.indices[i].len()).rev() {
                if self.indices[i][j] == index as u32 {
                    self.indices[i].remove(j);
                }
            }
        }
    }

    /// Exchange exchanges the ID of an image for a new one. If the old ID could not
    /// be found, nothing happens. If the new ID already existed prior to the
    /// exchange, the function returns immediately.
    /// 
    pub fn exchange(&mut self, oldid: &str, newid: &str) -> bool {
        if !self.ids.contains_key(oldid) {
            return false;
        }
        if self.ids.contains_key(newid) {
            return false;
        }
        let index = self.ids[oldid];
        // update the ids
        self.ids.remove(oldid);
        self.ids.insert(newid.to_string(), index);

        // Update the candidate.
        self.candidates[index].id = newid.to_string();
        self.modified = true;
        true
    }

    /// Query performs a similarity search on the given image hashes and returns
    /// all potential matches. The returned slice will sort it so the match with the best score is its
    /// first element.
    pub fn query(&self, hash: &crate::hash::Hash) -> crate::matches::Matches {
        let mut ms = crate::matches::Matches::new();
        if self.candidates.len() == 0 {
            return ms;
        }

        let mut scores: Vec<f64> = Vec::new();
        for _ in 0..self.candidates.len() {
            scores.push(f64::NAN);
        }
        //let mut nummatches: usize;

        // Examine hash buckets.
        for coefindex in 0..hash.matrix.coefs.len() {
            let coef = &hash.matrix.coefs[coefindex];
            if coefindex == 0 {
                continue; // igore scaling function coefficient for now
            }
            // Calculate the weight bin outside the main loop.
            let y = coefindex / hash.matrix.width as usize;
            let x = coefindex % hash.matrix.width as usize;
            let mut bin = y;
            if x > y {
                bin = x;
            }
            if bin > 5 {
                bin = 5;
            }
            for colorindex in 0..coef.c.len() {
                let colorcoef = coef.c[colorindex];
                if !crate::hash::coefficient_is_significant(colorcoef, hash.thresholds.c[colorindex]) {
                    // Coef is too small. Ignore.
				    continue;
                }
                // At this point, we have a coefficient which we want to look up
			    // in the index buckets.
                let mut sign = 0;
                if colorcoef < 0.0 {
                    sign = 1;
                }
                let location = sign * IMAGESCALE *IMAGESCALE * crate::haar::COLOURCHANNELS 
                                    + coefindex as u32 * crate::haar::COLOURCHANNELS + colorindex as u32;
                let arr = &self.indices[location as usize];
                for i in 0..arr.len() {
                    let sindex = arr[i];
                    if scores[sindex as usize].is_nan() {
                        // calculated initial score
                        let mut score: f64  = 0.0;
                        for colorid in 0..coef.c.len() {
                            score += WEIGHTS[colorid][0];
                        }
                        scores[sindex as usize] = score;
                    }
                    // At this point, we have an entry in matches. Simply subtract the
                    // corresponding weight.
                    scores[sindex as usize] -= WEIGHTSUMS[bin];
                }
            }
        }
        // Create matches. If the dhash_distance is lower than the sensitivity threshold it is a *valid* match.
        for index in 0..scores.len() {
            if !scores[index].is_nan() {
                let mut m = crate::matches::Match::new();
                m.id = self.candidates[index].id.clone();
                m.score = scores[index];
                m.ratio_diff = self.candidates[index].ratio.log(10.0).abs() - hash.ratio.log(10.0);
                m.dhash_distance = crate::hamming::hamming_distance(
                                    self.candidates[index].dhash[0], hash.dhash[0])
                                    + crate::hamming::hamming_distance(
                                        self.candidates[index].dhash[1], hash.dhash[1]);
                m.histogram_distance = crate::hamming::hamming_distance(
                                            self.candidates[index].histogram, hash.histogram);
                if m.score < self.sensitivity {
                    ms.m.push(m);                
                }
            }
        }
        // sort the vector so the first match is the one with the lowest value --> the best match
        // The number of matches should be small, so the bubble sort is about the fastest algorithm.
        ms.sort();
        ms
    }

    pub fn size(&self) -> usize {
        self.candidates.len()
    }

    pub fn modified(&self) -> bool {
        self.modified
    }

    /// encode the data structure to binary stream
    pub fn encode(&self, to: &mut Vec<u8>) {
        let s = self.candidates.len();
        crate::marshal::store_usize(s, to);
        for elem in &self.candidates {
            elem.encode(to);
        }
        crate::marshal::store_hash_string_usize(&self.ids, to);
        let s = self.indices.len();
        crate::marshal::store_usize(s, to);
        crate::marshal::store_f64(self.sensitivity, to);
        for elem in &self.indices {
            crate::marshal::store_vec_u32(elem, to);
        }
        crate::marshal::store_bool(self.modified, to);
    }

    /// decode data structure from binary stream
    pub fn decode(&mut self, from: &mut std::io::Cursor<Vec<u8>>) {
        if let Some(s) = crate::marshal::restore_usize(from) {
            for _i in 0..s {
                let mut elem = crate::candidate::Candidate::new();
                elem.decode(from);
                self.candidates.push(elem);
            }
        } else {return;}
        self.ids.extend(crate::marshal::restore_hash_string_usize(from));
        // NOTE: field order must stay in sync with `encode` (indices count,
        // then sensitivity, then the indices themselves).
        if let Some(s) = crate::marshal::restore_usize(from) {
            if s != INDICESMAX as usize {
                // Corrupt or version-mismatched stream; refuse to loop over
                // a garbage count (a huge count used to grow `self.indices`
                // until the process was OOM-killed).
                return;
            }
            if let Some(r) = crate::marshal::restore_f64(from) {
                self.sensitivity = r;
            } else {return;}
            // `decode` must replace the indices: a fresh `Store::new` already
            // prefills INDICESMAX empty buckets, so appending would leave the
            // loaded buckets unreachable (queries read locations < INDICESMAX).
            self.indices.clear();
            for _i in 0..s {
                let mut elem = Vec::new();
                elem.extend(crate::marshal::restore_vec_u32(from));
                self.indices.push(elem);
            }
        } else {return;}
        if let Some(r) = crate::marshal::restore_bool(from) {
            self.modified = r;
        } else {return;}
        self.modified = false;
    }

    /// Write binary stream to file
    pub fn dump_binary(&mut self, storefile: &str) {
        let mut buffer = Vec::new();
        self.encode(&mut buffer);
        let path = std::path::Path::new(&storefile);
        let display = path.display();
        // Open a file in write-only mode, returns `io::Result<File>`
        let mut write_file = match std::fs::File::create(&path) {
            Err(why) => panic!("couldn't create {}: {}", display, why),
            Ok(write_file) => write_file,
        };
        let ret = write_file.write(&mut buffer);
        if ret.is_err() {
            log::error!("failed to write binary data to file {}!", &storefile)
        }
        let ret = write_file.flush();
        if ret.is_err() {
            log::error!(
                "failed to empty the write buffer for binary data to file {}!",
                &storefile
            )
        }
    }

    /// read binary stream from file
    pub fn slurp_binary(&mut self, storefile: &str) {
        let path = std::path::Path::new(&storefile);
        let display = path.display();
        let mut input_file = match std::fs::File::open(&path) {
            Err(why) => panic!("couldn't open {}: {}", display, why),
            Ok(read_file) => read_file,
        };
        let mut buf = Vec::new();
        let ret = input_file.read_to_end(&mut buf);
        if ret.is_err() {
            log::error!("Failed to read test file back in.");
            return;
        }
        let mut read_file = std::io::Cursor::new(buf);
        self.decode(&mut read_file);
    }

}

