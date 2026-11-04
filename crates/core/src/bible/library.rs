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

    /// Loads bundled translations followed by downloaded ones.
    ///
    /// Missing directories are ignored so a fresh install can start with no
    /// translations and offer the download screen. Downloaded translations win
    /// when they share an abbreviation with a bundled one.
    pub fn load_default() -> Self {
        let mut library = Self::new();
        let bundled = crate::data_dir();
        let downloaded = crate::user_data_dir();

        if let Ok(loaded) = Self::load_dir_lenient(&bundled) {
            library.merge(loaded);
        }

        if downloaded != bundled
            && let Ok(loaded) = Self::load_dir_lenient(&downloaded)
        {
            library.merge(loaded);
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
    fn load_dir_lenient(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let path = path.as_ref();
        let mut library = Self::new();

        for entry in fs::read_dir(path).map_err(|error| LoadError::read(path, error))? {
            let entry = entry.map_err(|error| LoadError::read(path, error))?;
            let file = entry.path();

            if !file.is_file()
                || !file
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                continue;
            }

            if let Ok(translation) = Translation::from_file(&file) {
                library.insert(translation);
            }
        }

        Ok(library)
    }

    fn merge(&mut self, other: Self) {
        for translation in other.translations {
            self.insert(translation);
        }
    }

    /// Adds a translation, replacing an existing one with the same abbreviation.
    fn insert(&mut self, translation: Translation) {
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
        let library = Library::load_dir(crate::data_dir()).unwrap();

        assert!(library.translation("KJV").is_some());
        assert!(library.translation("AMH").is_some());
        assert_eq!(library.translations().len(), 2);
    }

    #[test]
    fn translations_are_sorted_by_name() {
        let library = Library::load_dir(crate::data_dir()).unwrap();

        assert_eq!(library.translations()[0].abbreviation, "KJV");
    }

    #[test]
    fn translation_lookup_ignores_case() {
        let library = Library::load_dir(crate::data_dir()).unwrap();

        assert_eq!(
            library
                .translation("kjv")
                .map(|translation| translation.name.as_str()),
            Some("King James Version")
        );
        assert!(library.translation("NIV").is_none());
    }
}
