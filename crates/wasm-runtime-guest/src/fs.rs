pub use crate::wit::wasi::filesystem::preopens::{Descriptor, get_directories};
pub use crate::wit::wasi::filesystem::types::{DescriptorFlags, OpenFlags, PathFlags};
use std::path::Path;

pub fn read_file(path: impl AsRef<Path>) -> Result<Vec<u8>, String> {
  let path = path.as_ref();
  let segments: Vec<_> = path.iter().collect();

  let (root, _) = get_directories()
    .into_iter()
    .find(|(_, path)| path == "/")
    .expect("root");

  let mut descriptor: Descriptor = root;
  for (i, segment) in segments.iter().enumerate() {
    println!("Path segment: {segment:?}");
    let path = segment
      .to_str()
      .ok_or_else(|| format!("invalid path segment: {segment:?}"))?;

    // First.
    if i == 0 {
      if path != "/" {
        return Err("Only absolute paths".to_string());
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
      .map_err(|err| err.to_string())?;

    if last {
      const MAX: u64 = 1024 * 1024;

      let mut buffer: Vec<u8> = vec![];
      loop {
        let (bytes, eof) = descriptor
          .read(MAX, buffer.len() as u64)
          .map_err(|err| err.to_string())?;

        buffer.extend(bytes);

        if eof {
          break;
        }
      }

      return Ok(buffer);
    }
  }

  return Err("not found".to_string());
}
