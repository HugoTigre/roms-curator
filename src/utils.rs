use std::fs;
use std::path::Path;

pub fn create_dir(path: &Path) {
    match path.try_exists() {
        Ok(exists) => {
            if !exists {
                fs::create_dir_all(path)
                    .expect(&("Error creating ".to_string() + path.to_str().unwrap() + " directory"));
            } else {
                panic!("{} directory already exists.", path.to_str().unwrap());
            }
        }
        Err(err) => {
            panic!("Error creating directory {}", err);
        }
    }
}

pub fn remove_dir(path: &Path) {
    if let Ok(exists) = path.try_exists() {
        if exists {
            fs::remove_dir_all(path).expect("Error deleting folder.");
        }
    }
}