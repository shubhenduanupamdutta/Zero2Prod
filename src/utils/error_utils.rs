use std::{error, fmt};

pub fn error_chain_fmt(e: &impl error::Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "{}", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "\nCaused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}
