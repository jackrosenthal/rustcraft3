//! JASS: the scripting language of Warcraft III maps, Blizzard.j, and the AI scripts.

pub mod ast;
pub mod bytecode;
pub mod compile;
pub mod lexer;
pub mod parser;
pub mod softfloat;
pub mod value;
pub mod vm;

pub use compile::{CompileError, Module};
pub use value::{BaseType, FuncId, Value};
pub use vm::{Host, NativeResult, Outcome, Thread, Vm};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{file}: {err}")]
    Syntax { file: String, err: lexer::SyntaxError },
    #[error("{file}: {err}")]
    Compile { file: String, err: CompileError },
}

pub fn parse(src: &str) -> Result<ast::Program, lexer::SyntaxError> {
    let toks = lexer::tokenize(src)?;
    let mut prog = ast::Program::default();
    parser::Parser::new(&toks).program(&mut prog)?;
    Ok(prog)
}

/// Parses and links source files in order into one module.
pub fn compile<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<Module, Error> {
    let mut m = Module::default();
    for (name, src) in files {
        let prog = parse(src).map_err(|err| Error::Syntax { file: name.into(), err })?;
        m.add(&prog).map_err(|err| Error::Compile { file: name.into(), err })?;
    }
    Ok(m)
}
