use crate::utils::{
    DiaryFileId, DiaryRepository, FoundSourceFile, SourceFile, SourceFileType, SourceRepository,
};
use crate::Env;
use anyhow::{Context, Result};
use chrono::{NaiveDate, NaiveDateTime, Timelike};
use clap::Parser;
use colored::Colorize;
use itertools::Itertools;
use std::path::PathBuf;
use std::{fs, iter};

#[derive(Debug, Parser)]
pub struct AddCmd {
    #[clap(long)]
    diary: PathBuf,

    #[clap(long)]
    source: PathBuf,

    #[clap(long)]
    on: Option<NaiveDate>,

    #[clap(long)]
    #[clap(conflicts_with = "on")]
    from: Option<NaiveDate>,

    #[clap(long)]
    #[clap(conflicts_with = "on")]
    #[clap(requires = "from")]
    to: Option<NaiveDate>,

    #[clap(long)]
    remove: bool,

    #[clap(long)]
    dry_run: bool,

    #[clap(long)]
    verbose: bool,
}

impl AddCmd {
    pub fn run(self, env: &mut Env) -> Result<()> {
        let srcs = self.scan(env)?;
        let plan = self.process(env, &srcs)?;
        let stats = self.execute(env, plan)?;

        self.summary(env, stats)?;

        Ok(())
    }

    fn scan(&self, env: &mut Env) -> Result<Vec<SourceFile>> {
        writeln!(env.stdout, "{}", "Scanning".green().bold())?;

        let source = SourceRepository::new(&self.source)?;

        let mut files: Vec<_> = source
            .iter()?
            .map(|file| match file? {
                FoundSourceFile::Recognized(file) => {
                    if self.verbose {
                        writeln!(env.stdout, "  {} {}", "Found".green(), file.path.display())?;
                    }

                    Ok(Some(file))
                }

                FoundSourceFile::Unrecognized(path) => {
                    writeln!(
                        env.stdout,
                        "! {} {}: unrecognized",
                        "Warn".yellow(),
                        path.display()
                    )?;

                    Ok(None)
                }
            })
            .flatten_ok()
            .filter_ok(|file| {
                let date = file.ty.date();

                let on = self.on.is_none_or(|on| date == on);
                let from = self.from.is_none_or(|from| date >= from);
                let to = self.to.is_none_or(|to| date <= to);

                on && from && to
            })
            .collect::<Result<_>>()?;

        files.sort_by_key(|file| file.path.clone());

        writeln!(env.stdout)?;

        Ok(files)
    }

    fn process(&self, env: &mut Env, files: &[SourceFile]) -> Result<Plan> {
        writeln!(env.stdout, "{}", "Processing".green().bold())?;

        let mut plan = Plan::default();
        let diary = DiaryRepository::new(&self.diary)?;

        for file in files {
            let steps = match &file.ty {
                SourceFileType::Note { date } => self.process_note(&diary, file, *date)?,

                SourceFileType::Photo { date, id } => {
                    self.process_photo(&diary, file, *date, id.as_deref())?
                }

                SourceFileType::Video { date, id } => {
                    self.process_video(&diary, files, file, *date, id.as_deref())?
                }
            };

            plan.steps.extend(steps);
        }

        writeln!(env.stdout)?;

        Ok(plan)
    }

    fn process_note(
        &self,
        diary: &DiaryRepository,
        file: &SourceFile,
        file_dt: NaiveDate,
    ) -> Result<Vec<Step>> {
        let dst = DiaryFileId::new(file_dt, "index.org");

        if diary.has(&dst)? {
            Ok(vec![Step::skip_or_remove(
                file.path.clone(),
                "already in the diary",
                self.remove,
            )])
        } else {
            Ok(Step::copy_and_remove(file.path.clone(), dst, self.remove).collect())
        }
    }

    fn process_photo(
        &self,
        diary: &DiaryRepository,
        file: &SourceFile,
        file_dt: NaiveDateTime,
        file_id: Option<&str>,
    ) -> Result<Vec<Step>> {
        let dst = DiaryFileId::new(
            file_dt.date(),
            format!(
                "{}.{}",
                Self::get_media_name(file, file_dt, file_id),
                file.ext
            ),
        );

        if diary.has(&dst)? {
            return Ok(vec![Step::skip_or_remove(
                file.path.clone(),
                "already in the diary",
                self.remove,
            )]);
        }

        if let Some(file_id) = file_id {
            let already_exists = diary
                .find_by_date(file_dt.date())?
                .into_iter()
                .any(|entry| {
                    ["jpg", "png", "heic"]
                        .iter()
                        .any(|ext| entry.name.ends_with(&format!(" {}.{}", file_id, ext)))
                });

            if already_exists {
                return Ok(vec![Step::skip_or_remove(
                    file.path.clone(),
                    "already in the diary - under a different timestamp, though!",
                    self.remove,
                )]);
            }
        }

        Ok(Step::copy_and_remove(file.path.clone(), dst, self.remove).collect())
    }

    fn process_video(
        &self,
        diary: &DiaryRepository,
        files: &[SourceFile],
        file: &SourceFile,
        file_dt: NaiveDateTime,
        file_id: Option<&str>,
    ) -> Result<Vec<Step>> {
        let name = Self::get_media_name(file, file_dt, file_id);
        let mk = |ext: &str| DiaryFileId::new(file_dt.date(), format!("{}.{}", name, ext));

        let dst = mk("mp4");
        let dst_jpg = mk("jpg");
        let dst_png = mk("png");
        let dst_heic = mk("heic");

        if diary.has(&dst)? {
            return Ok(vec![Step::skip_or_remove(
                file.path.clone(),
                "already in the diary",
                self.remove,
            )]);
        }

        let has_photo = diary.has(&dst_jpg)? || diary.has(&dst_png)? || diary.has(&dst_heic)?;

        let will_have_photo = file_id.is_some()
            && files.iter().any(|src| {
                if let SourceFileType::Photo { id: id2, .. } = &src.ty {
                    file_id == id2.as_deref()
                } else {
                    false
                }
            });

        if has_photo || will_have_photo {
            return Ok(vec![Step::skip_or_remove(
                file.path.clone(),
                "already in the diary as a photo",
                self.remove,
            )]);
        }

        if let Some(file_id) = file_id {
            let already_exists = diary
                .find_by_date(file_dt.date())?
                .into_iter()
                .any(|entry| {
                    ["mp4", "jpg", "png", "heic"]
                        .iter()
                        .any(|ext| entry.name.ends_with(&format!(" {}.{}", file_id, ext)))
                });

            if already_exists {
                return Ok(vec![Step::skip_or_remove(
                    file.path.clone(),
                    "already in the diary - under a different timestamp, though!",
                    self.remove,
                )]);
            }
        }

        Ok(Step::copy_and_remove(file.path.clone(), dst, self.remove).collect())
    }

    fn get_media_name(file: &SourceFile, dt: NaiveDateTime, id: Option<&str>) -> String {
        let time = format!(
            "{:02}-{:02}-{:02}",
            dt.time().hour(),
            dt.time().minute(),
            dt.time().second()
        );

        if let Some(id) = id {
            return format!("{} {}", time, id);
        }

        if file.stem.starts_with("Screenshot") {
            return format!("{} screenshot", time);
        }

        if file.stem.starts_with("Screencast") || file.stem.starts_with("Screen Recording") {
            return format!("{} screencast", time);
        }

        if file.stem.starts_with("Recording") {
            return format!("{} recording", time);
        }

        file.stem.to_owned()
    }

    fn execute(&self, env: &mut Env, plan: Plan) -> Result<Stats> {
        writeln!(env.stdout, "{}", "Executing".green().bold())?;

        let mut diary = DiaryRepository::new(&self.diary)?;
        let mut stats = Stats::default();
        let step_count = plan.steps.len();

        for (step_idx, step) in plan.steps.into_iter().enumerate() {
            let ctxt = ExecutionCtxt {
                env,
                stats: &mut stats,
                diary: &mut diary,
                step_idx,
                step_count,
            };

            match step {
                Step::Copy { src, dst } => {
                    self.execute_copy(ctxt, src, dst)?;
                }
                Step::Skip { src, reason } => {
                    self.execute_skip(ctxt, src, reason)?;
                }
                Step::Remove { src, reason } => {
                    self.execute_remove(ctxt, src, reason)?;
                }
            }
        }

        Ok(stats)
    }

    fn execute_copy(&self, ctxt: ExecutionCtxt, src: PathBuf, dst: DiaryFileId) -> Result<()> {
        let action = if self.dry_run {
            "Would copy"
        } else {
            "Copying"
        };

        writeln!(
            ctxt.env.stdout,
            "  {} {} to {} | {}/{}",
            action.green(),
            src.display(),
            dst,
            ctxt.step_idx + 1,
            ctxt.step_count,
        )?;

        if !self.dry_run {
            ctxt.diary.add(&src, &dst)?;
        }

        ctxt.stats.copied += 1;

        Ok(())
    }

    fn execute_skip(&self, ctxt: ExecutionCtxt, src: PathBuf, reason: String) -> Result<()> {
        let action = if self.dry_run {
            "Would skip"
        } else {
            "Skipping"
        };

        writeln!(
            ctxt.env.stdout,
            "  {} {} ({}) | {}/{}",
            action.green(),
            src.display(),
            reason,
            ctxt.step_idx + 1,
            ctxt.step_count,
        )?;

        ctxt.stats.skipped += 1;

        Ok(())
    }

    fn execute_remove(&self, ctxt: ExecutionCtxt, src: PathBuf, reason: String) -> Result<()> {
        let action = if self.dry_run {
            "Would remove"
        } else {
            "Removing"
        };

        writeln!(
            ctxt.env.stdout,
            "  {} {} ({}) | {}/{}",
            action.green(),
            src.display(),
            reason,
            ctxt.step_idx + 1,
            ctxt.step_count,
        )?;

        if !self.dry_run {
            fs::remove_file(&src).with_context(|| format!("Couldn't remove: {}", src.display()))?;
        }

        ctxt.stats.removed += 1;

        Ok(())
    }

    fn summary(&self, env: &mut Env, stats: Stats) -> Result<()> {
        writeln!(env.stdout)?;
        writeln!(env.stdout, "{}", "Summary".green().bold())?;

        let mut print_files_stats = |files: usize, verb: &str, verb_dry_run: &str| -> Result<()> {
            if files > 0 {
                writeln!(
                    env.stdout,
                    "  {} {} file{}",
                    if self.dry_run { verb_dry_run } else { verb },
                    files,
                    if files > 1 { "s" } else { "" },
                )?;
            }

            Ok(())
        };

        print_files_stats(stats.skipped, "Skipped", "Would skip")?;
        print_files_stats(stats.copied, "Copied", "Would copy")?;
        print_files_stats(stats.removed, "Removed", "Would remove")?;

        Ok(())
    }
}

#[derive(Default)]
struct Stats {
    skipped: usize,
    copied: usize,
    removed: usize,
}

#[derive(Default, Debug)]
struct Plan {
    steps: Vec<Step>,
}

#[derive(Debug)]
enum Step {
    Copy { src: PathBuf, dst: DiaryFileId },
    Skip { src: PathBuf, reason: String },
    Remove { src: PathBuf, reason: String },
}

impl Step {
    fn copy_and_remove(src: PathBuf, dst: DiaryFileId, remove: bool) -> impl Iterator<Item = Self> {
        let add = Step::Copy {
            src: src.clone(),
            dst,
        };

        let remove = remove.then(|| Step::Remove {
            src,
            reason: "just added into the diary".into(),
        });

        iter::once(add).chain(remove)
    }

    fn skip_or_remove(src: PathBuf, reason: impl AsRef<str>, remove: bool) -> Self {
        let reason = reason.as_ref().into();

        if remove {
            Step::Remove { src, reason }
        } else {
            Step::Skip { src, reason }
        }
    }
}

struct ExecutionCtxt<'a, 'b> {
    env: &'a mut Env<'b>,
    stats: &'a mut Stats,
    diary: &'a mut DiaryRepository,
    step_idx: usize,
    step_count: usize,
}
