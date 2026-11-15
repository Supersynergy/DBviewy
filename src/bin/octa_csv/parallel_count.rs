//! Parallel count for regular files with identifiable CSV record boundaries.
use std::error::Error;
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::fs::FileExt;
use std::thread;

type Failure = Box<dyn Error + Send + Sync>;

struct Range<'a> {
    file: &'a File,
    pos: u64,
    end: u64,
}

impl Read for Range<'_> {
    fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
        let len = (self.end - self.pos).min(dest.len() as u64) as usize;
        if len == 0 {
            return Ok(0);
        }
        let read = self.file.read_at(&mut dest[..len], self.pos)?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.pos += read as u64;
        Ok(read)
    }
}

fn boundary(probe: &[u8]) -> Option<usize> {
    // After an ordinary byte the CSV parser is either in a field or in a
    // quoted field. A single quote followed by a terminator ends the record
    // in both states. A doubled quote or a quote at field start cannot anchor.
    for (i, &byte) in probe.iter().enumerate().skip(1) {
        if byte != b'"' || matches!(probe[i - 1], b',' | b'"' | b'\r' | b'\n') {
            continue;
        }
        let rest = &probe[i + 1..];
        let newline = if rest.starts_with(b"\r\n") {
            2
        } else if rest.starts_with(b"\n") {
            1
        } else {
            continue;
        };
        let next = i + 1 + newline;
        // A new csv::Reader strips a UTF-8 BOM only at its beginning.
        if probe
            .get(next..next + 3)
            .is_some_and(|s| s != b"\xef\xbb\xbf")
        {
            return Some(next);
        }
    }
    None
}

fn count_range(range: Range<'_>) -> Result<(u64, usize), Failure> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(range);
    let mut record = csv::ByteRecord::new();
    let mut rows = 0;
    while reader.read_byte_record(&mut record)? {
        rows += 1;
    }
    Ok((rows, reader.byte_headers()?.len()))
}

pub(super) fn count(file: &File) -> Result<Option<u64>, Failure> {
    let before = file.metadata()?;
    let size = before.len();
    let lanes = thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(16)
        .min((size / (16 << 20)).min(usize::MAX as u64) as usize);
    if !before.is_file() || lanes < 2 {
        return Ok(None);
    }
    let mut offsets = vec![0];
    let mut probe = vec![0; 64 << 10];
    let mut plain_boundaries = false;
    for lane in 1..lanes {
        let at = size / lanes as u64 * lane as u64;
        let n = file.read_at(&mut probe, at)?;
        let sample = &probe[..n];
        let end = if let Some(end) = boundary(sample) {
            end
        } else if !sample.contains(&b'"') {
            let Some(line) = sample.iter().position(|&b| b == b'\n') else {
                return Ok(None);
            };
            if !sample
                .get(line + 1..line + 4)
                .is_some_and(|s| s != b"\xef\xbb\xbf")
            {
                return Ok(None);
            }
            plain_boundaries = true;
            line + 1
        } else {
            return Ok(None);
        };
        offsets.push(at + end as u64);
    }
    if plain_boundaries {
        // A newline in a quoted field is not a boundary. Certify the whole
        // source before using any plain newline; mixed files can stream.
        let mut source = Range {
            file,
            pos: 0,
            end: size,
        };
        loop {
            let n = source.read(&mut probe)?;
            if n == 0 {
                break;
            }
            if probe[..n].contains(&b'"') {
                return Ok(None);
            }
        }
    }
    offsets.push(size);
    let total = thread::scope(|scope| -> Result<u64, Failure> {
        let mut jobs = Vec::new();
        for pair in offsets.windows(2) {
            let range = Range {
                file,
                pos: pair[0],
                end: pair[1],
            };
            jobs.push(thread::Builder::new().spawn_scoped(scope, move || count_range(range))?);
        }
        let mut total = 0;
        let mut width = None;
        for job in jobs {
            let (rows, fields) = job.join().map_err(|_| "CSV count worker stopped")??;
            if rows > 0 && width.get_or_insert(fields) != &fields {
                return Err("inconsistent CSV field count across partitions".into());
            }
            total += rows;
        }
        Ok(total.saturating_sub(1))
    })?;
    let after = file.metadata()?;
    if after.len() != size || before.modified()? != after.modified()? {
        return Err("CSV changed while counting".into());
    }
    Ok(Some(total))
}
