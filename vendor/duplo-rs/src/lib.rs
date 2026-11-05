/// Package duplo-rs provides tools to efficiently query large sets of images for
/// visual duplicates. The technique is based on the paper "Fast Multiresolution
/// Image Querying" by Charles E. Jacobs, Adam Finkelstein, and David H. Salesin,
/// with a few modifications and additions, such as the addition of a width to
/// height ratio, the dHash metric by Dr. Neal Krawetz as well as some
/// histogram-based metrics.
///
/// Quering the data structure will return a list of potential matches, sorted by
/// the score described in the main paper. The user can make searching for
/// duplicates stricter, however, by filtering based on the additional metrics.
///
/// This project is a reimplementation of the project https://github.com/rivo/duplo
/// in rust.
///
/// It is intended for recurring runs and for finding similar parts in videos.
/// So the data structures will be different.
//use image;
mod candidate;
pub mod files;
mod haar;
mod hamming;
mod hash;
mod marshal;
mod matches;
pub mod store;
pub mod videocandidate;
pub mod videomatches;
pub mod videostore;

/// processes all the images in the list.
/// fills the data structure one by one.
/// compares to all the previously hashed images.
//pub fn compare

#[cfg(test)]
mod tests;
