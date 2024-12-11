use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use glob::glob;
use itertools::Itertools;
use std::cmp;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

#[derive(Debug)]
pub struct SourceRepository {
    dir: PathBuf,
}

impl SourceRepository {
    pub fn new(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();

        if !dir.try_exists()? {
            return Err(anyhow!("Source directory not found: {}", dir.display()));
        }

        Ok(Self {
            dir: dir.to_owned(),
        })
    }

    pub fn iter(&self) -> Result<impl Iterator<Item = Result<FoundSourceFile>>> {
        let files = glob(&format!("{}/**/*", self.dir.display()))?
            .filter_ok(|entry| entry.is_file())
            .map(|entry| {
                let path = entry?.to_path_buf();

                let Some(stem) = path.file_stem() else {
                    return Ok(FoundSourceFile::Unrecognized(path));
                };

                let Some(ext) = path.extension() else {
                    return Ok(FoundSourceFile::Unrecognized(path));
                };

                let file: Result<_> = try {
                    let stem = stem
                        .to_str()
                        .context("File has non-Unicode stem")?
                        .to_owned();

                    let ext = ext
                        .to_str()
                        .context("File has non-Unicode extension")?
                        .to_lowercase();

                    let ty = SourceFileType::new(&path, &stem, &ext)?;

                    ty.map(|ty| SourceFile {
                        path: path.clone(),
                        stem,
                        ext,
                        ty,
                    })
                };

                let file =
                    file.with_context(|| format!("Couldn't identify file: {}", path.display()))?;

                if let Some(file) = file {
                    Ok(FoundSourceFile::Recognized(file))
                } else {
                    Ok(FoundSourceFile::Unrecognized(path))
                }
            });

        Ok(files)
    }
}

#[derive(Debug)]
pub enum FoundSourceFile {
    Recognized(SourceFile),
    Unrecognized(PathBuf),
}

#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub stem: String,
    pub ext: String,
    pub ty: SourceFileType,
}

#[derive(Debug)]
pub enum SourceFileType {
    Note {
        date: NaiveDate,
    },
    Photo {
        date: NaiveDateTime,
        id: Option<String>,
    },
    Video {
        date: NaiveDateTime,
        id: Option<String>,
    },
}

impl SourceFileType {
    fn new(path: &Path, stem: &str, ext: &str) -> Result<Option<Self>> {
        let created_or_modified_at = || -> Result<_> {
            let metadata = path.metadata()?;

            let date = match (metadata.created(), metadata.modified()) {
                (Ok(created_at), Ok(modified_at)) => cmp::min(created_at, modified_at),
                (Ok(created_at), Err(_)) => created_at,
                (Err(_), Ok(modified_at)) => modified_at,
                (Err(_), Err(_)) => {
                    return Err(anyhow!("Cannot determine file timestamp"));
                }
            };

            Ok(DateTime::<Local>::from(date).naive_local())
        };

        match ext {
            "org" => {
                let mut stem = stem.split('-');

                let year = stem
                    .next()
                    .context("Invalid name: missing year")?
                    .parse()
                    .context("Invalid name: invalid year")?;

                let month = stem
                    .next()
                    .context("Invalid name: missing month")?
                    .parse()
                    .context("Invalid name: invalid month")?;

                let day = stem
                    .next()
                    .context("Invalid name: missing day")?
                    .parse()
                    .context("Invalid name: invalid day")?;

                Ok(Some(Self::Note {
                    date: NaiveDate::from_ymd_opt(year, month, day)
                        .context("Invalid name: invalid date")?,
                }))
            }

            "jpg" | "png" | "webp" | "heic" | "mov" | "mp4" | "webm" => {
                enum Kind {
                    Photo,
                    Video,
                }

                let kind = match ext {
                    "jpg" | "png" | "webp" | "heic" => Kind::Photo,
                    "mov" | "mp4" | "webm" => Kind::Video,
                    _ => unreachable!(),
                };

                let mut date = None;
                let mut id = None;

                if let Some((new_date, new_time, new_id)) = stem.split('_').collect_tuple() {
                    let new_date = new_date.split('-').collect_tuple();
                    let new_time = new_time.split('-').collect_tuple();

                    if let (Some((year, month, day)), Some((hour, min, sec))) = (new_date, new_time)
                    {
                        let year = year.parse()?;
                        let month = month.parse()?;
                        let day = day.parse()?;

                        let hour = hour.parse()?;
                        let min = min.parse()?;
                        let sec = sec.parse()?;

                        date = Some(NaiveDateTime::new(
                            NaiveDate::from_ymd_opt(year, month, day).unwrap(),
                            NaiveTime::from_hms_opt(hour, min, sec).unwrap(),
                        ));

                        id = Some(new_id.to_string());
                    }
                }

                let tag = match kind {
                    Kind::Photo => "-DateTimeOriginal",
                    Kind::Video => "-MediaCreateDate",
                };

                let date = if let Some(date) = date {
                    date
                } else if let Some(date) = extract_media_datetime(path, tag)? {
                    date
                } else {
                    created_or_modified_at()?
                };

                let id = id.or_else(|| stem.strip_prefix("IMG_").map(|id| id.to_owned()));

                Ok(Some(match kind {
                    Kind::Photo => Self::Photo { date, id },
                    Kind::Video => Self::Video { date, id },
                }))
            }

            _ => Ok(None),
        }
    }

    pub fn date(&self) -> NaiveDate {
        match self {
            Self::Note { date } => *date,
            Self::Photo { date, .. } | Self::Video { date, .. } => date.date(),
        }
    }
}

fn extract_media_datetime(path: &Path, tag: &str) -> Result<Option<NaiveDateTime>> {
    let out = Command::new("exiftool")
        .arg("-s")
        .arg("-T")
        .arg(tag)
        .arg(path)
        .output()
        .context("Couldn't launch exiftool")?
        .stdout;

    let out = String::from_utf8_lossy(&out);
    let out = out.trim();

    if out == "-" || out == "0000:00:00 00:00:00" {
        Ok(None)
    } else {
        parse_exiftool_date(out)
            .map(Some)
            .with_context(|| format!("Couldn't parse exiftool's response: {}", out))
    }
}

fn parse_exiftool_date(s: &str) -> Option<NaiveDateTime> {
    fn parse<T>(s: impl AsRef<str>) -> Option<T>
    where
        T: FromStr,
    {
        s.as_ref().parse().ok()
    }

    if s.is_empty() {
        return None;
    }

    let (d, t) = s.split(' ').collect_tuple()?;
    let t = t.split_once('-').map(|(t, _)| t).unwrap_or(t);
    let (d_y, d_m, d_d) = d.split(':').collect_tuple()?;
    let (t_h, t_m, t_s) = t.split(':').collect_tuple()?;

    let t_s = if let Some(ms_idx) = t_s.find('.') {
        &t_s[0..ms_idx]
    } else {
        t_s
    };

    let t_s = if let Some(sep_idx) = t_s.find('-') {
        &t_s[0..sep_idx]
    } else {
        t_s
    };

    let dt = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(parse(d_y)?, parse(d_m)?, parse(d_d)?)?,
        NaiveTime::from_hms_opt(parse(t_h)?, parse(t_m)?, parse(t_s)?)?,
    );

    Some(dt + Local.offset_from_utc_date(&dt.date()))
}

#[cfg(test)]
mod tests {
    use std::env;
    use test_case::test_case;

    #[test_case("2016:04:23 20:19:55", "2016-04-23 20:19:55")]
    #[test_case("2016:04:23 20:19:55.1234", "2016-04-23 20:19:55")]
    #[test_case("2016:04:23 20:19:55-20:19", "2016-04-23 20:19:55")]
    fn parse_exiftool_date(given: &str, expected: &str) {
        env::set_var("TZ", "UTC");

        let actual = super::parse_exiftool_date(given).unwrap().to_string();

        assert_eq!(expected, actual);
    }
}
