use std::borrow::Cow;

pub(crate) fn cow_to_string(cow: Cow<'static, [u8]>) -> String {
  match cow {
    Cow::Borrowed(x) => String::from_utf8_lossy(x).to_string(),
    Cow::Owned(x) => String::from_utf8_lossy(&x).to_string(),
  }
}
