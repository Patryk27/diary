#![feature(try_blocks)]

mod cmds;
mod env;
mod utils;

pub use self::cmds::*;
pub use self::env::*;
use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
pub enum Cmd {
    Add(AddCmd),
}

impl Cmd {
    pub fn run(self, env: &mut Env) -> Result<()> {
        match self {
            Cmd::Add(cmd) => cmd.run(env),
        }
    }
}
