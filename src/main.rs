use anyhow::Result;
use clap::Parser;
use diary::{Cmd, Env};
use std::io;

fn main() -> Result<()> {
    let mut stdout = io::stdout().lock();

    let mut env = Env {
        stdout: &mut stdout,
    };

    Cmd::parse().run(&mut env)
}
