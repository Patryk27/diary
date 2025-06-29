use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, NaiveDate};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

#[derive(Debug)]
pub struct DiaryRepository {
    dir: PathBuf,
}

impl DiaryRepository {
    pub fn new(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();

        if !dir.try_exists()? {
            return Err(anyhow!("diary directory not found: {}", dir.display()));
        }

        Ok(Self {
            dir: dir.to_owned(),
        })
    }

    fn dir(&self, date: NaiveDate) -> PathBuf {
        self.dir
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}", date.month()))
            .join(format!("{:02}", date.day()))
    }

    fn resolve(&self, path: &DiaryPath) -> PathBuf {
        self.dir(path.date).join(&path.file)
    }

    pub fn add(&mut self, src: impl AsRef<Path>, dst: &DiaryPath) -> Result<()> {
        let src = src.as_ref();
        let dir = self.dir(dst.date);
        let dst_path = self.resolve(dst);

        if dst_path.try_exists()? {
            return Err(anyhow!(
                "cannot add `{}` into diary, because it would overwrite `{}`",
                src.display(),
                dst,
            ));
        }

        if !dir.try_exists()? {
            fs::create_dir_all(&dir)
                .with_context(|| format!("couldn't create directory: {}", dir.display()))?;
        }

        fs::copy(src, &dst_path).with_context(|| {
            format!(
                "couldn't copy `{}` to `{}`",
                src.display(),
                dst_path.display()
            )
        })?;

        Ok(())
    }

    pub fn has(&self, path: &DiaryPath) -> Result<bool> {
        Ok(self.resolve(path).try_exists()?)
    }
}

#[derive(Debug)]
pub struct DiaryPath {
    pub date: NaiveDate,
    pub file: String,
}

impl DiaryPath {
    pub fn new(date: NaiveDate, file: impl AsRef<str>) -> Self {
        Self {
            date,
            file: file.as_ref().to_string(),
        }
    }
}

impl fmt::Display for DiaryPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "diary:{:04}/{:02}/{:02}/{}",
            self.date.year(),
            self.date.month(),
            self.date.day(),
            self.file,
        )
    }
}
