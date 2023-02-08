use std::{env, fs, io};
use std::path::{Path};

pub fn create_dir(path: &Path, ignore_if_exists: bool) {
    match path.try_exists() {
        Ok(exists) => {
            if !exists {
                fs::create_dir_all(path)
                    .expect(&("Error creating ".to_string() + path.to_str().unwrap() + " directory"));
            } else if !ignore_if_exists {
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

pub fn sanitize_path(path: &str) -> String {
    if env::consts::OS.eq("windows") {
        path.replace('/', "\\")
    } else {
        path.replace('\\', "/")
    }
}

pub fn copy_dir_recursive(path: &Path, destination: &Path) -> io::Result<()> {
    create_dir(destination, true);
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let filetype = entry.file_type()?;
        if filetype.is_dir() {
            copy_dir_recursive(&entry.path(), &destination.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}
