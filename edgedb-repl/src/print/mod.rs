use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io;
#[cfg(test)] use std::convert::Infallible;

use async_std::stream::{Stream, StreamExt};
use atty;
use snafu::{Snafu, ResultExt, AsErrorSource};
use term_size;
use uuid::Uuid;

mod format;
mod buffer;
mod stream;
mod formatter;
pub mod style;
#[cfg(test)] mod tests;

use format::FormatExt;
use formatter::ColorfulExt;
use buffer::{Exception, WrapErr, UnwrapExc, Delim};
use stream::Output;


#[derive(Snafu, Debug)]
pub enum PrintError<S: AsErrorSource + Error, P: AsErrorSource + Error> {
    #[snafu(display("error fetching element"))]
    StreamErr { source: S },
    #[snafu(display("error printing element"))]
    PrintErr { source: P },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub colors: Option<bool>,
    pub indent: usize,
    pub max_width: Option<usize>,
    pub implicit_properties: bool,
    pub type_names: Option<HashMap<Uuid, String>>,
}


pub(in crate::print) struct Printer<'a, T> {
    // config
    colors: bool,
    indent: usize,
    max_width: usize,
    implicit_properties: bool,
    type_names: &'a Option<HashMap<Uuid, String>>,

    // state
    buffer: String,
    stream: T,
    delim: Delim,
    flow: bool,
    committed: usize,
    committed_indent: usize,
    committed_column: usize,
    column: usize,
    cur_indent: usize,
}

struct Stdout {}

async fn format_rows_buf<S, I, E, O>(prn: &mut Printer<'_, O>, rows: &mut S,
    row_buf: &mut Vec<I>)
    -> Result<(), Exception<PrintError<E, O::Error>>>
    where S: Stream<Item=Result<I, E>> + Send + Unpin,
          I: FormatExt,
          E: fmt::Debug + Error + 'static,
          O: Output,
          O::Error: fmt::Debug + Error + 'static,
{
    let branch = prn.open_block("{".clear()).wrap_err(PrintErr)?;
    debug_assert!(branch);
    while let Some(v) = rows.next().await.transpose().wrap_err(StreamErr)? {
        row_buf.push(v);
        let v = row_buf.last().unwrap();
        v.format(prn).wrap_err(PrintErr)?;
        prn.comma().wrap_err(PrintErr)?;
        // Buffer rows up to one visual line.
        // After line is reached we get Exception::DisableFlow
    }
    prn.close_block("}".clear(), true).wrap_err(PrintErr)?;
    Ok(())
}

async fn format_rows<S, I, E, O>(prn: &mut Printer<'_, O>,
    buffered_rows: Vec<I>, rows: &mut S)
    -> Result<(), Exception<PrintError<E, O::Error>>>
    where S: Stream<Item=Result<I, E>> + Send + Unpin,
          I: FormatExt,
          E: fmt::Debug + Error + 'static,
          O: Output,
          O::Error: fmt::Debug + Error + 'static,
{
    prn.reopen_block().wrap_err(PrintErr)?;
    for v in buffered_rows {
        v.format(prn).wrap_err(PrintErr)?;
        prn.comma().wrap_err(PrintErr)?;
    }
    while let Some(v) = rows.next().await.transpose().wrap_err(StreamErr)? {
        v.format(prn).wrap_err(PrintErr)?;
        prn.comma().wrap_err(PrintErr)?;
    }
    prn.close_block("}".clear(), true).wrap_err(PrintErr)?;
    Ok(())
}

pub async fn print_to_stdout<S, I, E>(mut rows: S, config: &Config)
    -> Result<(), PrintError<E, io::Error>>
    where S: Stream<Item=Result<I, E>> + Send + Unpin,
          I: FormatExt,
          E: fmt::Debug + Error + 'static,
{
    let w = config.max_width.unwrap_or_else(|| {
        term_size::dimensions_stdout().map(|(w, _h)| w).unwrap_or(80)
    });
    let mut prn = Printer {
        colors: config.colors
            .unwrap_or_else(|| atty::is(atty::Stream::Stdout)),
        indent: config.indent,
        max_width: w,
        implicit_properties: config.implicit_properties,
        type_names: &config.type_names,

        buffer: String::with_capacity(8192),
        stream: Stdout {},
        delim: Delim::None,
        flow: false,
        committed: 0,
        committed_indent: 0,
        committed_column: 0,
        column: 0,
        cur_indent: 0,
    };
    let mut row_buf = Vec::new();
    match format_rows_buf(&mut prn, &mut rows, &mut row_buf).await {
        Ok(()) => {},
        Err(Exception::DisableFlow) => {
            // debug_assert!(t.0 == 0);
            format_rows(&mut prn, row_buf, &mut rows).await.unwrap_exc()?;
        }
        Err(Exception::Error(e)) => return Err(e),
    };
    prn.end().unwrap_exc().context(PrintErr)?;
    Ok(())
}

#[cfg(test)]
fn test_format_rows<I: FormatExt>(prn: &mut Printer<&mut String>, items: &[I],
    reopen: bool)
    -> buffer::Result<Infallible>
{
    if reopen {
        prn.reopen_block()?;
    } else {
        let cp = prn.open_block("{".clear())?;
        debug_assert!(cp);
    }
    for v in items {
        v.format(prn)?;
        prn.comma()?;
    }
    prn.close_block("}".clear(), true)?;
    Ok(())
}

#[cfg(test)]
pub fn test_format<I: FormatExt>(items: &[I], max_width: usize)
    -> Result<String, Infallible>
{
    let mut out = String::new();
    let mut prn = Printer {
        colors: false,
        indent: 2,
        max_width: max_width,
        implicit_properties: false,
        type_names: &None,

        buffer: String::with_capacity(8192),
        stream: &mut out,
        delim: Delim::None,
        flow: false,
        committed: 0,
        committed_indent: 0,
        committed_column: 0,
        column: 0,
        cur_indent: 0,
    };
    match test_format_rows(&mut prn, &items, false) {
        Ok(()) => {},
        Err(Exception::DisableFlow) => {
            test_format_rows(&mut prn, &items, true).unwrap_exc()?;
        }
        Err(Exception::Error(e)) => return Err(e),
    };
    prn.end().unwrap_exc()?;
    Ok(out)
}
