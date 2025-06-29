use crate::utils::{
    DiaryPath, DiaryRepository, FoundSourceFile, SourceFile, SourceFileType, SourceRepository,
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
}

impl AddCmd {
    pub fn run(self, env: &mut Env) -> Result<()> {
        if self.dry_run {
            writeln!(env.stdout, "{} is active", "--dry-run".yellow())?;
            writeln!(env.stdout)?;
        }

        let srcs = self.scan(env)?;
        let plan = self.plan(env, &srcs)?;
        let stats = self.exec(env, plan)?;

        self.summary(env, stats)?;

        if self.dry_run {
            writeln!(env.stdout)?;
            writeln!(env.stdout, "{} is active", "--dry-run".yellow())?;
        }

        Ok(())
    }

    fn scan(&self, env: &mut Env) -> Result<Vec<SourceFile>> {
        writeln!(env.stdout, "{}", "scanning".green().bold())?;

        let source = SourceRepository::new(&self.source)?;

        let mut files: Vec<_> = source
            .iter()?
            .map(|file| match file? {
                FoundSourceFile::Recognized(file) => {
                    writeln!(env.stdout, "  {} {}", "found".green(), file.path.display())?;

                    Ok(Some(file))
                }

                FoundSourceFile::Unrecognized(path) => {
                    writeln!(
                        env.stdout,
                        "{} {}: unrecognized",
                        "warn".yellow(),
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

    fn plan(&self, env: &mut Env, files: &[SourceFile]) -> Result<Plan> {
        writeln!(env.stdout, "{}", "planning".green().bold())?;

        let mut plan = Plan::default();
        let diary = DiaryRepository::new(&self.diary)?;

        for file in files {
            let steps = match &file.ty {
                SourceFileType::Note { date } => self.plan_note(&diary, file, *date)?,
                SourceFileType::Photo { date, id } | SourceFileType::Video { date, id } => {
                    self.plan_media(&diary, file, *date, id.as_deref())?
                }
            };

            plan.steps.extend(steps);
        }

        writeln!(env.stdout)?;

        Ok(plan)
    }

    fn plan_note(
        &self,
        diary: &DiaryRepository,
        file: &SourceFile,
        file_dt: NaiveDate,
    ) -> Result<Vec<Step>> {
        let dst = DiaryPath::new(file_dt, "index.org");

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

    fn plan_media(
        &self,
        diary: &DiaryRepository,
        file: &SourceFile,
        file_dt: NaiveDateTime,
        file_id: Option<&str>,
    ) -> Result<Vec<Step>> {
        let dst = DiaryPath::new(
            file_dt.date(),
            format!(
                "{}.{}",
                Self::get_media_name(file, file_dt, file_id),
                file.ext
            ),
        );

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

    fn get_media_name(file: &SourceFile, dt: NaiveDateTime, id: Option<&str>) -> String {
        let time = format!(
            "{:02}-{:02}-{:02}",
            dt.time().hour(),
            dt.time().minute(),
            dt.time().second(),
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

    fn exec(&self, env: &mut Env, plan: Plan) -> Result<Stats> {
        writeln!(env.stdout, "{}", "executing".green().bold())?;

        let mut diary = DiaryRepository::new(&self.diary)?;
        let mut stats = Stats::default();
        let step_count = plan.steps.len();

        for (step_idx, step) in plan.steps.into_iter().enumerate() {
            let ctxt = ExecCtxt {
                env,
                stats: &mut stats,
                diary: &mut diary,
                step_idx,
                step_count,
            };

            match step {
                Step::Copy { src, dst } => {
                    self.exec_copy(ctxt, src, dst)?;
                }
                Step::Skip { src, reason } => {
                    self.exec_skip(ctxt, src, reason)?;
                }
                Step::Remove { src, reason } => {
                    self.exec_remove(ctxt, src, reason)?;
                }
            }
        }

        Ok(stats)
    }

    fn exec_copy(&self, ctxt: ExecCtxt, src: PathBuf, dst: DiaryPath) -> Result<()> {
        writeln!(
            ctxt.env.stdout,
            "  {} `{}` to `{}` [{}/{}]",
            "copying".green(),
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

    fn exec_skip(&self, ctxt: ExecCtxt, src: PathBuf, reason: String) -> Result<()> {
        writeln!(
            ctxt.env.stdout,
            "  {} `{}` ({}) [{}/{}]",
            "skipping".green(),
            src.display(),
            reason,
            ctxt.step_idx + 1,
            ctxt.step_count,
        )?;

        ctxt.stats.skipped += 1;

        Ok(())
    }

    fn exec_remove(&self, ctxt: ExecCtxt, src: PathBuf, reason: String) -> Result<()> {
        writeln!(
            ctxt.env.stdout,
            "  {} `{}` ({}) [{}/{}]",
            "removing".green(),
            src.display(),
            reason,
            ctxt.step_idx + 1,
            ctxt.step_count,
        )?;

        if !self.dry_run {
            fs::remove_file(&src).with_context(|| format!("couldn't remove: {}", src.display()))?;
        }

        ctxt.stats.removed += 1;

        Ok(())
    }

    fn summary(&self, env: &mut Env, stats: Stats) -> Result<()> {
        writeln!(env.stdout)?;
        writeln!(env.stdout, "{}", "summary".green().bold())?;

        let mut print_files_stats = |files: usize, verb: &str| -> Result<()> {
            if files > 0 {
                writeln!(
                    env.stdout,
                    "  {} {} file{}",
                    verb,
                    files,
                    if files > 1 { "s" } else { "" },
                )?;
            }

            Ok(())
        };

        print_files_stats(stats.skipped, "skipped")?;
        print_files_stats(stats.copied, "copied")?;
        print_files_stats(stats.removed, "removed")?;

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
    Copy { src: PathBuf, dst: DiaryPath },
    Skip { src: PathBuf, reason: String },
    Remove { src: PathBuf, reason: String },
}

impl Step {
    fn copy_and_remove(src: PathBuf, dst: DiaryPath, remove: bool) -> impl Iterator<Item = Self> {
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

struct ExecCtxt<'a, 'b> {
    env: &'a mut Env<'b>,
    stats: &'a mut Stats,
    diary: &'a mut DiaryRepository,
    step_idx: usize,
    step_count: usize,
}
