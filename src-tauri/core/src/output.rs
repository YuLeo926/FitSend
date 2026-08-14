use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct OutputTransaction {
    final_path: PathBuf,
    temporary_path: PathBuf,
    published: bool,
}

impl OutputTransaction {
    pub fn new(requested: &Path) -> Result<Self, String> {
        let final_path = non_overwriting_path(requested);
        let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| format!("FitSend could not create the output folder: {error}"))?;
        let stem = final_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("fitsend");
        let extension = final_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("tmp");
        let temporary_path = parent.join(format!(
            "{stem}.fitsend-working-{}-{}.{}",
            std::process::id(),
            unique_suffix(),
            extension
        ));
        register_working_path(&temporary_path)?;
        Ok(Self {
            final_path,
            temporary_path,
            published: false,
        })
    }

    pub fn temporary_path(&self) -> &Path {
        &self.temporary_path
    }

    pub fn publish(mut self) -> Result<PathBuf, String> {
        fs::rename(&self.temporary_path, &self.final_path)
            .map_err(|error| format!("FitSend could not publish the verified output: {error}"))?;
        self.published = true;
        unregister_working_path(&self.temporary_path);
        Ok(self.final_path.clone())
    }
}

impl Drop for OutputTransaction {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.temporary_path);
        }
        unregister_working_path(&self.temporary_path);
    }
}

pub fn cleanup_stale_outputs() {
    let registry = registry_path();
    let Ok(contents) = fs::read_to_string(&registry) else {
        return;
    };
    if let Ok(paths) = serde_json::from_str::<Vec<String>>(&contents) {
        for value in paths {
            let path = PathBuf::from(value);
            let safe_marker = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".fitsend-working-"));
            if safe_marker {
                let _ = fs::remove_file(path);
            }
        }
    }
    let _ = fs::remove_file(registry);
}

pub fn non_overwriting_path(requested: &Path) -> PathBuf {
    if !requested.exists() {
        return requested.to_path_buf();
    }
    let parent = requested.parent().unwrap_or_else(|| Path::new("."));
    let stem = requested
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("fitsend");
    let extension = requested.extension().and_then(|value| value.to_str());
    for index in 2..10_000 {
        let name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}-{}", unique_suffix()))
}

fn registry_path() -> PathBuf {
    std::env::temp_dir().join("fitsend-working-registry.json")
}

fn register_working_path(path: &Path) -> Result<(), String> {
    let registry = registry_path();
    let mut paths = fs::read_to_string(&registry)
        .ok()
        .and_then(|contents| serde_json::from_str::<Vec<String>>(&contents).ok())
        .unwrap_or_default();
    let value = path.to_string_lossy().to_string();
    if !paths.contains(&value) {
        paths.push(value);
    }
    let contents = serde_json::to_vec(&paths)
        .map_err(|error| format!("FitSend could not record its temporary output: {error}"))?;
    fs::write(registry, contents)
        .map_err(|error| format!("FitSend could not record its temporary output: {error}"))
}

fn unregister_working_path(path: &Path) {
    let registry = registry_path();
    let Ok(contents) = fs::read_to_string(&registry) else {
        return;
    };
    let Ok(mut paths) = serde_json::from_str::<Vec<String>>(&contents) else {
        return;
    };
    let value = path.to_string_lossy();
    paths.retain(|candidate| candidate != value.as_ref());
    if paths.is_empty() {
        let _ = fs::remove_file(registry);
    } else if let Ok(contents) = serde_json::to_vec(&paths) {
        let _ = fs::write(registry, contents);
    }
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_to_the_next_collision_safe_name() {
        let directory = tempfile::tempdir().unwrap();
        let requested = directory.path().join("photo.fitsend.jpg");
        fs::write(&requested, b"existing").unwrap();
        let transaction = OutputTransaction::new(&requested).unwrap();
        fs::write(transaction.temporary_path(), b"new").unwrap();
        let published = transaction.publish().unwrap();
        assert_eq!(published.file_name().unwrap(), "photo.fitsend-2.jpg");
        assert_eq!(fs::read(requested).unwrap(), b"existing");
        assert_eq!(fs::read(published).unwrap(), b"new");
    }

    #[test]
    fn removes_unpublished_temporary_output_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let requested = directory.path().join("photo.fitsend.png");
        let temporary = {
            let transaction = OutputTransaction::new(&requested).unwrap();
            fs::write(transaction.temporary_path(), b"partial").unwrap();
            transaction.temporary_path().to_path_buf()
        };
        assert!(!temporary.exists());
        assert!(!requested.exists());
    }
}
