pub use crate::wit::wasi::filesystem::preopens::{Descriptor, get_directories};
pub use crate::wit::wasi::filesystem::types::{DescriptorFlags, ErrorCode, OpenFlags, PathFlags};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("MissingRoot")]
  MissingRoot,
  #[error("NotFound")]
  NotFound,
  #[error("InvalidPath")]
  InvalidPath,
  #[error("Open {0}")]
  Open(ErrorCode),
  #[error("Read {0}")]
  Read(ErrorCode),
}

pub fn read_file(path: impl AsRef<Path>) -> Result<Vec<u8>, Error> {
  let path = path.as_ref();
  let segments: Vec<_> = path.iter().collect();

  let (root, _) = get_directories()
    .into_iter()
    .find(|(_, path)| path == "/")
    .ok_or_else(|| Error::MissingRoot)?;

  let mut descriptor: Descriptor = root;
  for (i, segment) in segments.iter().enumerate() {
    let path = segment.to_str().ok_or_else(|| Error::InvalidPath)?;

    // First.
    if i == 0 {
      if path != "/" {
        return Err(Error::InvalidPath);
      }
      continue;
    }

    let last = i == segments.len() - 1;
    descriptor = descriptor
      .open_at(
        PathFlags::empty(),
        path,
        if last {
          OpenFlags::empty()
        } else {
          OpenFlags::DIRECTORY
        },
        DescriptorFlags::READ,
      )
      .map_err(|err| Error::Open(err))?;

    if last {
      const MAX: u64 = 1024 * 1024;

      let mut buffer: Vec<u8> = vec![];
      loop {
        let (bytes, eof) = descriptor
          .read(MAX, buffer.len() as u64)
          .map_err(|err| Error::Read(err))?;

        buffer.extend(bytes);

        if eof {
          break;
        }
      }

      return Ok(buffer);
    }
  }

  return Err(Error::NotFound);
}
