#[inline]
pub(crate) fn named_placeholder(s: &str) -> String {
  let mut new = String::with_capacity(s.len() + 1);
  new.push(':');
  for char in s.chars() {
    new.push(if char.is_alphanumeric() { char } else { '_' });
  }
  return new;
}
