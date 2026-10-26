//use std::cell::{Ref, RefCell, RefMut};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::tab::Item;

#[derive(Debug, Default)]
pub struct ScanMetaData {
    known_files: Mutex<std::collections::BTreeMap<PathBuf, crate::sql::FileMetadata>>,
    special_files: Mutex<std::collections::BTreeSet<PathBuf>>,
    items: Mutex<Vec<Item>>,
    tvshows: Mutex<Vec<PathBuf>>,
}

impl ScanMetaData {
    pub fn new() -> Self {
        ScanMetaData {
            ..Default::default()
        }
    }
    pub fn known_files_contains(&self, p: PathBuf) -> bool {
        match self.known_files.lock() {
            Ok(bm) => return bm.contains_key(&p),
            Err(error) => log::error!("could not lock known_files for reading! {}", error),
        }
        false
    }
    pub fn known_files_get(&self, p: PathBuf) -> crate::sql::FileMetadata {
        match self.known_files.lock() {
            Ok(bm) => return bm[&p].clone(),
            Err(error) => log::error!("could not lock known_files for reading! {}", error),
        }
        crate::sql::FileMetadata {
            ..Default::default()
        }
    }
    pub fn special_files_contains(&self, p: PathBuf) -> bool {
        match self.special_files.lock() {
            Ok(bm) => return bm.contains(&p),
            Err(error) => log::error!("could not lock known_files for reading! {}", error),
        }
        false
    }
    pub fn known_files_insert(&self, p: PathBuf, m: crate::sql::FileMetadata) {
        match self.known_files.lock() {
            Ok(mut bm) => {
                bm.insert(p, m);
            }
            Err(error) => log::error!("could not lock known_files for insert! {}", error),
        }
    }
    pub fn known_files_extend(
        &self,
        b: std::collections::BTreeMap<PathBuf, crate::sql::FileMetadata>,
    ) {
        match self.known_files.lock() {
            Ok(mut bm) => {
                for (k, v) in b.iter() {
                    if !bm.contains_key(k) {
                        bm.insert(k.to_path_buf(), v.to_owned());
                    }
                }
            }
            Err(error) => log::error!("could not lock known_files for insert! {}", error),
        }
    }

    pub fn known_files_clone(
        &self,
    ) -> std::collections::BTreeMap<PathBuf, crate::sql::FileMetadata> {
        match self.known_files.lock() {
            Ok(bm) => {
                let mut b = std::collections::BTreeMap::new();
                for (k, v) in bm.iter() {
                    b.insert(k.to_owned(), v.to_owned());
                }
                return b;
            }
            Err(error) => {
                log::error!("could not lock known_files for clone! {}", error);
                std::collections::BTreeMap::new()
            }
        }
    }
    pub fn known_files_remove(&self, p: PathBuf) {
        match self.known_files.lock() {
            Ok(mut bm) => {
                bm.remove(&p);
            }
            Err(error) => log::error!("could not lock known_files for remove! {}", error),
        }
    }
    pub fn special_files_insert(&self, p: PathBuf) {
        match self.special_files.lock() {
            Ok(mut bm) => {
                bm.insert(p);
            }
            Err(error) => log::error!("could not lock special_files for insert! {}", error),
        }
    }
    pub fn special_files_extend(&self, v: std::collections::BTreeSet<PathBuf>) {
        match self.special_files.lock() {
            Ok(mut bm) => {
                for k in v.iter() {
                    if !bm.contains(k) {
                        bm.insert(k.to_path_buf());
                    }
                }
                bm.extend(v);
            }
            Err(error) => log::error!("could not lock known_files for insert! {}", error),
        }
    }
    pub fn special_files_remove(&self, p: PathBuf) {
        match self.special_files.lock() {
            Ok(mut bm) => {
                bm.remove(&p);
            }
            Err(error) => log::error!("could not lock special_files for remove! {}", error),
        }
    }
    pub fn special_files_clone(&self) -> std::collections::BTreeSet<PathBuf> {
        match self.special_files.lock() {
            Ok(bm) => {
                let mut v = std::collections::BTreeSet::new();
                for k in bm.iter() {
                    v.insert(k.to_owned());
                }
                return v;
            }
            Err(error) => {
                log::error!("could not lock known_files for clone! {}", error);
                std::collections::BTreeSet::new()
            }
        }
    }
    pub fn items_clone(&self) -> Vec<Item> {
        match self.items.lock() {
            Ok(bm) => {
                let mut v = Vec::new();
                for p in bm.iter() {
                    v.push(p.to_owned());
                }
                return v;
            }
            Err(error) => {
                log::error!("could not lock known_files for clone! {}", error);
                Vec::new()
            }
        }
    }
    pub fn items_push(&self, i: Item) {
        match self.items.lock() {
            Ok(mut bm) => {
                bm.push(i);
            }
            Err(error) => log::error!("could not lock items for push! {}", error),
        }
    }
    pub fn items_pop(&self) -> Option<Item> {
        match self.items.lock() {
            Ok(mut bm) => {
                if bm.len() == 0 {
                    return None;
                }
                let last = bm.len() - 1;
                let item = bm.remove(last);
                return Some(item);
            }
            Err(error) => log::error!("could not lock items for push! {}", error),
        }
        None
    }

    pub fn tvshows_clone(&self) -> Vec<PathBuf> {
        match self.tvshows.lock() {
            Ok(bm) => {
                let mut v = Vec::new();
                for p in bm.iter() {
                    v.push(p.to_path_buf());
                }
                return v;
            }
            Err(error) => {
                log::error!("could not lock known_files for clone! {}", error);
                Vec::new()
            }
        }
    }
    pub fn tvshows_push(&self, p: PathBuf) {
        match self.tvshows.lock() {
            Ok(mut bm) => {
                bm.push(p);
            }
            Err(error) => log::error!("could not lock justdirs for push! {}", error),
        }
    }
    pub fn tvshows_clear(&self) {
        match self.tvshows.lock() {
            Ok(mut bm) => {
                bm.clear();
            }
            Err(error) => log::error!("could not lock justdirs for push! {}", error),
        }
    }
}
