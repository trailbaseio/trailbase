use std::borrow::Cow;
use std::error::Error;
use std::str::Utf8Error;

use crate::value::{Value, ValueRef};

/// Enum listing possible errors from [`FromSql`] trait.
#[derive(Debug, thiserror::Error)]
pub enum FromSqlError {
  /// Error when an SQLite value is requested, but the type of the result
  /// cannot be converted to the requested Rust type.
  #[error("InvalidType")]
  InvalidType,

  /// Error when the i64 value returned by SQLite cannot be stored into the
  /// requested type.
  #[error("OutOfRange {0}")]
  OutOfRange(i64),

  /// Error converting a string to UTF-8.
  #[error("Utf8Error {0}")]
  Utf8Error(Utf8Error),

  /// Error when the blob result returned by SQLite cannot be stored into the
  /// requested type due to a size mismatch.
  #[error("InvalidBlobSize")]
  InvalidBlobSize {
    /// The expected size of the blob.
    expected_size: usize,
    /// The actual size of the blob that was returned.
    blob_size: usize,
  },

  /// An error case available for implementors of the [`FromSql`] trait.
  #[error("Other {0}")]
  Other(Box<dyn Error + Send + Sync + 'static>),
}

// impl PartialEq for FromSqlError {
//   fn eq(&self, other: &Self) -> bool {
//     return match (self, other) {
//       (Self::InvalidType, Self::InvalidType) => true,
//       (Self::OutOfRange(n1), Self::OutOfRange(n2)) => n1 == n2,
//       (Self::Utf8Error(u1), Self::Utf8Error(u2)) => u1 == u2,
//       (
//         Self::InvalidBlobSize {
//           expected_size: es1,
//           blob_size: bs1,
//         },
//         Self::InvalidBlobSize {
//           expected_size: es2,
//           blob_size: bs2,
//         },
//       ) => es1 == es2 && bs1 == bs2,
//       (..) => false,
//     };
//   }
// }

/// Result type for implementers of the [`FromSql`] trait.
pub type FromSqlResult<T> = Result<T, FromSqlError>;

/// A trait for types that can be created from a SQLite value.
pub trait FromSql: Sized {
  /// Converts SQLite value into Rust value.
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self>;
}

macro_rules! from_sql_integral(
    ($t:ident) => (
        impl FromSql for $t {
            #[inline]
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                let i = i64::column_result(value)?;
                i.try_into().map_err(|_| FromSqlError::OutOfRange(i))
            }
        }
    );
    (non_zero $nz:ty, $z:ty) => (
        impl FromSql for $nz {
            #[inline]
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                let i = <$z>::column_result(value)?;
                <$nz>::new(i).ok_or(FromSqlError::OutOfRange(0))
            }
        }
    )
);

from_sql_integral!(i8);
from_sql_integral!(i16);
from_sql_integral!(i32);
// from_sql_integral!(i64); // Not needed because the native type is i64.
from_sql_integral!(isize);
from_sql_integral!(u8);
from_sql_integral!(u16);
from_sql_integral!(u32);

from_sql_integral!(non_zero std::num::NonZeroIsize, isize);
from_sql_integral!(non_zero std::num::NonZeroI8, i8);
from_sql_integral!(non_zero std::num::NonZeroI16, i16);
from_sql_integral!(non_zero std::num::NonZeroI32, i32);
from_sql_integral!(non_zero std::num::NonZeroI64, i64);

impl FromSql for i64 {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.as_i64();
  }
}

impl FromSql for f32 {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return match value {
      ValueRef::Integer(i) => Ok(i as Self),
      ValueRef::Real(f) => Ok(f as Self),
      _ => Err(FromSqlError::InvalidType),
    };
  }
}

impl FromSql for f64 {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return match value {
      ValueRef::Integer(i) => Ok(i as Self),
      ValueRef::Real(f) => Ok(f),
      _ => Err(FromSqlError::InvalidType),
    };
  }
}

impl FromSql for bool {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return i64::column_result(value).map(|i| i != 0);
  }
}

impl FromSql for String {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.as_str().map(ToString::to_string);
  }
}

impl FromSql for Box<str> {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.as_str().map(Into::into);
  }
}

impl FromSql for std::rc::Rc<str> {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.as_str().map(Into::into);
  }
}

impl FromSql for std::sync::Arc<str> {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.as_str().map(Into::into);
  }
}

impl FromSql for Vec<u8> {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.as_blob().map(<[u8]>::to_vec);
  }
}

impl FromSql for Box<[u8]> {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.as_blob().map(Box::<[u8]>::from);
  }
}

impl FromSql for std::rc::Rc<[u8]> {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.as_blob().map(std::rc::Rc::<[u8]>::from);
  }
}

impl FromSql for std::sync::Arc<[u8]> {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.as_blob().map(std::sync::Arc::<[u8]>::from);
  }
}

impl<const N: usize> FromSql for [u8; N] {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    let slice = value.as_blob()?;
    return slice.try_into().map_err(|_| FromSqlError::InvalidBlobSize {
      expected_size: N,
      blob_size: slice.len(),
    });
  }
}

impl<T: FromSql> FromSql for Option<T> {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return match value {
      ValueRef::Null => Ok(None),
      _ => FromSql::column_result(value).map(Some),
    };
  }
}

impl<T: ?Sized> FromSql for Cow<'_, T>
where
  T: ToOwned,
  T::Owned: FromSql,
{
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return <T::Owned>::column_result(value).map(Cow::Owned);
  }
}

impl FromSql for Value {
  #[inline]
  fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
    return value.try_into();
  }
}
