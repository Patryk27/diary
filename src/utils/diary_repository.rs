use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, NaiveDate};
use itertools::Itertools;
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

    pub fn dir(&self, date: NaiveDate) -> PathBuf {
        self.dir
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}", date.month()))
            .join(format!("{:02}", date.day()))
    }

    pub fn file(&self, id: &DiaryFileId) -> PathBuf {
        self.dir(id.date).join(&id.name)
    }

    pub fn add(&mut self, src: impl AsRef<Path>, dst: &DiaryFileId) -> Result<()> {
        let src = src.as_ref();
        let dir = self.dir(dst.date);
        let dst_path = self.file(dst);

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

    pub fn has(&self, id: &DiaryFileId) -> Result<bool> {
        Ok(self.file(id).try_exists()?)
    }

    pub fn find_by_date(&self, date: NaiveDate) -> Result<Vec<DiaryFileId>> {
        let dir = self.dir(date);

        if !dir.try_exists()? {
            return Ok(Default::default());
        }

        fs::read_dir(&dir)?
            .map(|entry| {
                let entry = entry?;

                if entry.file_type()?.is_dir() {
                    return Ok(None);
                }

                let name = entry
                    .file_name()
                    .to_str()
                    .context("file has non-unicode name")?
                    .to_owned();

                Ok(Some(DiaryFileId { date, name }))
            })
            .flatten_ok()
            .collect()
    }
}

#[derive(Debug)]
pub struct DiaryFileId {
    pub date: NaiveDate,
    pub name: String,
}

impl DiaryFileId {
    pub fn new(date: NaiveDate, name: impl AsRef<str>) -> Self {
        Self {
            date,
            name: name.as_ref().to_string(),
        }
    }
}

impl fmt::Display for DiaryFileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "diary:{:04}/{:02}/{:02}/{}",
            self.date.year(),
            self.date.month(),
            self.date.day(),
            self.name
        )
    }
}
