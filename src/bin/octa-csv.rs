//! Raw RFC-style CSV access for agents that need cell bytes without type inference.
#[cfg(unix)]
#[path = "octa_csv/parallel_count.rs"]
mod parallel_count;

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::ExitCode;

const HELP: &str = "octa-csv slice FILE START LENGTH\nocta-csv count FILE\nocta-csv verify-index FILE\nOptional first flag: --indexed (use a matching qsv/xsv .idx file).\n\nComma-separated CSV with a header. START is zero-based. Use - for stdin.";

fn number(value: &OsString) -> Result<usize, String> {
    value
        .to_str()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| "expected a non-negative integer".into())
}

fn index_word(index: &mut File, offset: u64) -> io::Result<u64> {
    index.seek(SeekFrom::Start(offset))?;
    let mut bytes = [0; 8];
    index.read_exact(&mut bytes)?;
    Ok(u64::from_be_bytes(bytes))
}

// qsv and xsv store a big-endian u64 offset per record (including the header),
// followed by the number of offsets. The caller explicitly selects this index.
fn open_index(
    path: &Path,
    data: &std::fs::Metadata,
) -> Result<(File, std::fs::Metadata, u64), Box<dyn std::error::Error>> {
    let mut sidecar = path.as_os_str().to_owned();
    sidecar.push(".idx");
    let mut index = File::open(sidecar)?;
    let meta = index.metadata()?;
    if meta.modified()? < data.modified()? {
        return Err("CSV changed after indexing; rebuild its index".into());
    }
    let bytes = meta.len();
    if bytes < 8 || bytes % 8 != 0 {
        return Err("invalid CSV index length".into());
    }
    let records = index_word(&mut index, bytes - 8)?;
    if records != bytes / 8 - 1 || records > data.len() {
        return Err("CSV index does not match its source".into());
    }
    Ok((index, meta, records))
}

fn verify_index(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let data = file.metadata()?;
    let (mut index, before, records) = open_index(path, &data)?;
    index.rewind()?;
    let mut offsets = io::BufReader::new(index);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(file);
    let mut record = csv::ByteRecord::new();
    let mut seen = 0u64;
    while reader.read_byte_record(&mut record)? {
        if seen >= records {
            return Err("CSV contains more records than its index".into());
        }
        let mut word = [0; 8];
        offsets.read_exact(&mut word)?;
        if Some(u64::from_be_bytes(word)) != record.position().map(csv::Position::byte) {
            return Err(
                format!("CSV index offset mismatch at record {seen}; rebuild its index").into(),
            );
        }
        seen += 1;
    }
    if seen != records {
        return Err("CSV ended before its indexed record count".into());
    }
    for (snapshot, current) in [
        (data, reader.get_ref().metadata()?),
        (before, offsets.get_ref().metadata()?),
    ] {
        if current.len() != snapshot.len() || current.modified()? != snapshot.modified()? {
            return Err("CSV or index changed during verification".into());
        }
    }
    writeln!(io::stdout().lock(), "{}", seen.saturating_sub(1))?;
    Ok(())
}

fn indexed(path: &Path, range: Option<(usize, usize)>) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let data = file.metadata()?;
    let (mut index, _, records) = open_index(path, &data)?;
    let count = records.saturating_sub(1);
    let mut out = io::stdout().lock();
    let Some((start, len)) = range else {
        writeln!(out, "{count}")?;
        return Ok(());
    };
    let start = u64::try_from(start)?;
    let take = count.saturating_sub(start).min(u64::try_from(len)?);
    let mut offset = None;
    if take > 0 {
        let pos = index_word(&mut index, (start + 1) * 8)?;
        if pos == 0 || pos >= data.len() {
            return Err("row offset outside CSV body".into());
        }
        file.seek(SeekFrom::Start(pos - 1))?;
        let mut window = [0; 4];
        let available = (data.len() - pos).min(3) as usize + 1;
        file.read_exact(&mut window[..available])?;
        if !matches!(window[0], b'\r' | b'\n') {
            return Err(
                "indexed row does not follow a record terminator; rebuild its index".into(),
            );
        }
        file.rewind()?;
        // Seeking resets BOM detection in csv-core. A BOM inside a field
        // is data, so keep the original parser state in this rare case.
        if window[1..available] == *b"\xef\xbb\xbf" {
            return stream(file, range);
        }
        offset = Some(pos);
    }
    let mut reader = csv::Reader::from_reader(file);
    let mut writer = csv::Writer::from_writer(out);
    let header = reader.byte_headers()?;
    if !header.is_empty() {
        writer.write_byte_record(header)?;
    }
    if let Some(offset) = offset {
        if offset < reader.position().byte() {
            return Err("row offset outside CSV body".into());
        }
        let mut position = csv::Position::new();
        position.set_byte(offset).set_record(start + 1);
        reader.seek(position)?;
        let mut record = csv::ByteRecord::new();
        for _ in 0..take {
            if !reader.read_byte_record(&mut record)? {
                return Err("CSV ended before its indexed row count".into());
            }
            writer.write_byte_record(&record)?;
        }
    }
    writer.flush()?;
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if matches!(args.as_slice(), [arg] if arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    if let [command, path] = args.as_slice()
        && command == "verify-index"
    {
        return verify_index(Path::new(path));
    }
    let (use_index, args) = match args.split_first() {
        Some((first, rest)) if first == "--indexed" => (true, rest),
        _ => (false, args.as_slice()),
    };
    let (path, range) = match args {
        [cmd, file] if cmd == "count" => (file, None),
        [cmd, file, start, len] if cmd == "slice" => (file, Some((number(start)?, number(len)?))),
        _ => return Err(HELP.into()),
    };
    if use_index {
        if path == "-" {
            return Err("indexed reads require a file".into());
        }
        return indexed(Path::new(path), range);
    }
    let input: Box<dyn io::Read> = if path == "-" {
        Box::new(io::stdin().lock())
    } else {
        let file = File::open(Path::new(path))?;
        #[cfg(unix)]
        if range.is_none()
            && let Some(rows) = parallel_count::count(&file).map_err(|e| e.to_string())?
        {
            writeln!(io::stdout().lock(), "{rows}")?;
            return Ok(());
        }
        Box::new(file)
    };
    stream(input, range)
}

fn stream(
    input: impl Read,
    range: Option<(usize, usize)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = csv::Reader::from_reader(input);
    let mut record = csv::ByteRecord::new();
    let mut out = io::stdout().lock();
    if let Some((start, len)) = range {
        let mut writer = csv::Writer::from_writer(out);
        let header = reader.byte_headers()?;
        if !header.is_empty() {
            writer.write_byte_record(header)?;
        }
        if len > 0 {
            for _ in 0..start {
                if !reader.read_byte_record(&mut record)? {
                    writer.flush()?;
                    return Ok(());
                }
            }
            for _ in 0..len {
                if !reader.read_byte_record(&mut record)? {
                    break;
                }
                writer.write_byte_record(&record)?;
            }
        }
        writer.flush()?;
    } else {
        let mut count = 0u64;
        while reader.read_byte_record(&mut record)? {
            count += 1;
        }
        writeln!(out, "{count}")?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
