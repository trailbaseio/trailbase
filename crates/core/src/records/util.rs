use compact_str::CompactString;

#[inline]
pub(crate) fn named_placeholder(s: &str) -> CompactString {
  let mut new = CompactString::with_capacity(s.len() + 1);
  new.push(':');
  for char in s.chars() {
    new.push(if char.is_alphanumeric() { char } else { '_' });
  }
  return new;
}
