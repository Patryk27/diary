use std::io::Write;

pub struct Env<'a> {
    pub stdout: &'a mut dyn Write,
}
