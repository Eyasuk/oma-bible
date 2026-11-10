use std::{fs, path::Path};

use super::{LoadError, Translation};

/// Holds the translations available to the application.
#[derive(Debug, Default)]
pub struct Library {
    translations: Vec<Translation>,
}

impl Library {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads the built-in King James Version and every translation
    /// downloaded from the Bible API. A downloaded copy of the KJV replaces
    /// the built-in one. A missing directory is ignored, so a fresh install
    /// opens on the KJV.
    pub fn load_default() -> Self {
        let mut library = Self::load_dir_lenient(crate::user_data_dir()).unwrap_or_default();
        // Decoding the built-in copy is only worth it when there is no
        // downloaded one to replace it.
        if library
            .translation(crate::bundle::DEFAULT_TRANSLATION)
            .is_none()
            && let Some(kjv) = crate::bundle::translation()
        {
            library.insert(kjv);
        }
        library.sort();
        library
    }

    /// Loads every `.json` file in a directory, in filename order.
    pub fn load_dir(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let path = path.as_ref();
        let mut paths = Vec::new();

        for entry in fs::read_dir(path).map_err(|error| LoadError::read(path, error))? {
            let entry = entry.map_err(|error| LoadError::read(path, error))?;
            let path = entry.path();

            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                paths.push(path);
            }
        }

        paths.sort();

        let mut library = Self::new();
        for path in paths {
            library.add(Translation::from_file(path)?);
        }

        library.sort();

        Ok(library)
    }

    /// Like [`Library::load_dir`], but skips files that cannot be parsed.
    /// Files are parsed in parallel: whole Bibles are several megabytes of
    /// JSON each, and startup waits for all of them.
    fn load_dir_lenient(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let path = path.as_ref();
        let mut files = Vec::new();
        for entry in fs::read_dir(path).map_err(|error| LoadError::read(path, error))? {
            let entry = entry.map_err(|error| LoadError::read(path, error))?;
            let file = entry.path();
            if file.is_file()
                && file
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                files.push(file);
            }
        }
        files.sort();

        let workers = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(4)
            .clamp(1, 8);
        let chunk = files.len().div_ceil(workers).max(1);
        let parsed: Vec<Translation> = std::thread::scope(|scope| {
            let handles: Vec<_> = files
                .chunks(chunk)
                .map(|chunk| {
                    scope.spawn(move || {
                        chunk
                            .iter()
                            .filter_map(|file| Translation::from_file(file).ok())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap_or_default())
                .collect()
        });

        let mut library = Self::new();
        for translation in parsed {
            library.insert(translation);
        }
        Ok(library)
    }

    /// Adds a translation, replacing an existing one with the same abbreviation.
    /// Files saved before a translation was renamed get the new name.
    fn insert(&mut self, mut translation: Translation) {
        if let Some(name) = crate::remote::preferred_name(&translation.abbreviation) {
            translation.name = name.to_string();
        }
        match self.translations.iter_mut().find(|existing| {
            existing
                .abbreviation
                .eq_ignore_ascii_case(&translation.abbreviation)
        }) {
            Some(existing) => *existing = translation,
            None => self.translations.push(translation),
        }
    }

    fn sort(&mut self) {
        self.translations.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.abbreviation.cmp(&right.abbreviation))
        });
    }

    pub fn add(&mut self, translation: Translation) {
        self.translations.push(translation);
    }

    /// The translation to open when none was chosen yet: the King James
    /// Version when it is there, else the first.
    pub fn default_index(&self) -> usize {
        self.translations
            .iter()
            .position(|translation| {
                translation
                    .abbreviation
                    .eq_ignore_ascii_case(crate::bundle::DEFAULT_TRANSLATION)
            })
            .unwrap_or(0)
    }

    /// Finds a translation by abbreviation, ignoring ASCII letter case.
    pub fn translation(&self, abbreviation: &str) -> Option<&Translation> {
        self.translations
            .iter()
            .find(|translation| translation.abbreviation.eq_ignore_ascii_case(abbreviation))
    }

    pub fn translations(&self) -> &[Translation] {
        &self.translations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_every_json_file_in_a_directory() {
        let library = Library::load_dir(crate::fixtures_dir()).unwrap();

        assert!(library.translation("KJV").is_some());
        assert!(library.translation("AMH").is_some());
        assert_eq!(library.translations().len(), 2);
    }

    #[test]
    fn translations_are_sorted_by_name() {
        let library = Library::load_dir(crate::fixtures_dir()).unwrap();

        assert_eq!(library.translations()[0].abbreviation, "KJV");
    }

    #[test]
    fn translation_lookup_ignores_case() {
        let library = Library::load_dir(crate::fixtures_dir()).unwrap();

        assert_eq!(
            library
                .translation("kjv")
                .map(|translation| translation.name.as_str()),
            Some("King James Version")
        );
        assert!(library.translation("NIV").is_none());
    }
}
