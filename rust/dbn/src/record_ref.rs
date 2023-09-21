//! The [`RecordRef`] struct for non-owning references to DBN records.

use std::{marker::PhantomData, mem, ptr::NonNull};

use crate::{
    enums::RType,
    record::{HasRType, RecordHeader},
    RecordEnum, RecordRefEnum,
};

/// A wrapper around a non-owning immutable reference to a DBN record. This wrapper
/// allows for mixing of record types and schemas, and runtime record polymorphism.
#[derive(Copy, Clone, Debug)]
pub struct RecordRef<'a> {
    ptr: NonNull<RecordHeader>,
    /// Associates the object with the lifetime of the memory pointed to by `ptr`.
    _marker: PhantomData<&'a RecordHeader>,
}

// Safety: RecordRef exhibits immutable reference semantics similar to &T.
// It should be safe to both send it across threads or access it simultaneously
// (since the data is immutable).
unsafe impl Send for RecordRef<'_> {}
unsafe impl Sync for RecordRef<'_> {}

impl<'a> RecordRef<'a> {
    /// Constructs a new reference to the DBN record in `buffer`.
    ///
    /// # Safety
    /// `buffer` should begin with a [`RecordHeader`] and contain a type implementing
    /// [`HasRType`].
    pub unsafe fn new(buffer: &'a [u8]) -> Self {
        debug_assert!(buffer.len() >= mem::size_of::<RecordHeader>());

        // Safety: casting to `*mut` to use `NonNull`, but `ptr` is still treated internally
        // as an immutable reference
        let raw_ptr = buffer.as_ptr() as *mut RecordHeader;

        // Check if alignment of pointer
        debug_assert_eq!(
            raw_ptr.align_offset(std::mem::align_of::<RecordHeader>()),
            0
        );
        let ptr = NonNull::new_unchecked(raw_ptr.cast::<RecordHeader>());
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Constructs a new reference to the DBN record.
    ///
    /// # Safety
    /// `header` must point to a valid DBN record.
    pub unsafe fn unchecked_from_header(header: *const RecordHeader) -> Self {
        Self {
            // `NonNull` requires `mut` but it is never mutated
            ptr: NonNull::new_unchecked(header.cast_mut()),
            _marker: PhantomData,
        }
    }

    /// Returns a reference to the [`RecordHeader`] of the referenced record.
    pub fn header(&self) -> &RecordHeader {
        // Safety: assumes `ptr` passes to a `RecordHeader`.
        unsafe { self.ptr.as_ref() }
    }

    /// Returns `true` if the object points to a record of type `T`.
    pub fn has<T: HasRType>(&self) -> bool {
        T::has_rtype(self.header().rtype)
    }

    /// Returns the size of the record in bytes.
    pub fn record_size(&self) -> usize {
        self.header().record_size()
    }

    /// Tries to convert the raw `rtype` into an enum which is useful for exhaustive
    /// pattern matching.
    ///
    /// # Errors
    /// This function returns an error if the `rtype` field does not
    /// contain a valid, known [`RType`].
    pub fn rtype(&self) -> crate::Result<RType> {
        self.header().rtype()
    }

    /// Returns a reference to the underlying record of type `T` or `None` if it points
    /// to another record type.
    ///
    /// Note: for safety, this method calls [`has::<T>()`](Self::has). To avoid a
    /// duplicate check, use [`get_unchecked()`](Self::get_unchecked).
    ///
    /// # Panics
    /// This function will panic if the rtype indicates it's of type `T` but the encoded
    ///  length of the record is less than the size of `T`.
    pub fn get<T: HasRType>(&self) -> Option<&'a T> {
        if self.has::<T>() {
            assert!(
                self.record_size() >= mem::size_of::<T>(),
                "Malformed record. Expected length of at least {} bytes, found {} bytes",
                mem::size_of::<T>(),
                self.record_size()
            );
            // Safety: checked `rtype` in call to `has()`. Assumes the initial data based to
            // `RecordRef` is indeed a record.
            Some(unsafe { self.ptr.cast::<T>().as_ref() })
        } else {
            None
        }
    }

    /// Returns a native Rust enum with a variant for each record type. This allows for
    /// pattern `match`ing.
    ///
    /// # Errors
    /// This function returns a conversion error if the rtype does not correspond with
    /// any known DBN record type.
    pub fn as_enum(&self) -> crate::Result<RecordRefEnum> {
        RecordRefEnum::try_from(*self)
    }

    /// Returns a reference to the underlying record of type `T` without checking if
    /// this object references a record of type `T`.
    ///
    /// For a safe alternative, see [`get()`](Self::get).
    ///
    /// # Safety
    /// The caller needs to validate this object points to a `T`.
    pub unsafe fn get_unchecked<T: HasRType>(&self) -> &'a T {
        debug_assert!(self.has::<T>());
        debug_assert!(self.record_size() >= mem::size_of::<T>());
        self.ptr.cast::<T>().as_ref()
    }
}

impl<'a, R> From<&'a R> for RecordRef<'a>
where
    R: HasRType,
{
    /// Constructs a new reference to a DBN record.
    fn from(rec: &'a R) -> Self {
        Self {
            // Safe: `R` must be a record because it implements `HasRType`. Casting to `mut`
            // is required for `NonNull`, but it is never mutated.
            ptr: unsafe {
                NonNull::new_unchecked((rec.header() as *const RecordHeader).cast_mut())
            },
            _marker: PhantomData,
        }
    }
}

impl<'a> AsRef<[u8]> for RecordRef<'a> {
    fn as_ref(&self) -> &[u8] {
        // # Safety
        // Assumes the encoded record length is correct.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr() as *const u8, self.record_size()) }
    }
}

impl<'a> From<&'a RecordEnum> for RecordRef<'a> {
    fn from(rec_enum: &'a RecordEnum) -> Self {
        match rec_enum {
            RecordEnum::Mbo(rec) => Self::from(rec),
            RecordEnum::Trade(rec) => Self::from(rec),
            RecordEnum::Mbp1(rec) => Self::from(rec),
            RecordEnum::Mbp10(rec) => Self::from(rec),
            RecordEnum::Ohlcv(rec) => Self::from(rec),
            RecordEnum::Status(rec) => Self::from(rec),
            RecordEnum::InstrumentDef(rec) => Self::from(rec),
            RecordEnum::Imbalance(rec) => Self::from(rec),
            RecordEnum::Stat(rec) => Self::from(rec),
            RecordEnum::Error(rec) => Self::from(rec),
            RecordEnum::SymbolMapping(rec) => Self::from(rec),
            RecordEnum::System(rec) => Self::from(rec),
        }
    }
}

impl<'a> From<RecordRefEnum<'a>> for RecordRef<'a> {
    fn from(rec_enum: RecordRefEnum<'a>) -> Self {
        match rec_enum {
            RecordRefEnum::Mbo(rec) => Self::from(rec),
            RecordRefEnum::Trade(rec) => Self::from(rec),
            RecordRefEnum::Mbp1(rec) => Self::from(rec),
            RecordRefEnum::Mbp10(rec) => Self::from(rec),
            RecordRefEnum::Ohlcv(rec) => Self::from(rec),
            RecordRefEnum::Status(rec) => Self::from(rec),
            RecordRefEnum::InstrumentDef(rec) => Self::from(rec),
            RecordRefEnum::Imbalance(rec) => Self::from(rec),
            RecordRefEnum::Stat(rec) => Self::from(rec),
            RecordRefEnum::Error(rec) => Self::from(rec),
            RecordRefEnum::SymbolMapping(rec) => Self::from(rec),
            RecordRefEnum::System(rec) => Self::from(rec),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_char;

    use crate::{
        enums::rtype,
        record::{ErrorMsg, InstrumentDefMsg, MboMsg, Mbp10Msg, Mbp1Msg, OhlcvMsg, TradeMsg},
    };

    use super::*;

    const SOURCE_RECORD: MboMsg = MboMsg {
        hd: RecordHeader::new::<MboMsg>(rtype::MBO, 1, 1, 0),
        order_id: 17,
        price: 0,
        size: 32,
        flags: 0,
        channel_id: 1,
        action: 'A' as c_char,
        side: 'B' as c_char,
        ts_recv: 0,
        ts_in_delta: 160,
        sequence: 1067,
    };

    #[test]
    fn test_header() {
        let target = unsafe { RecordRef::new(SOURCE_RECORD.as_ref()) };
        assert_eq!(*target.header(), SOURCE_RECORD.hd);
    }

    #[test]
    fn test_has_and_get() {
        let target = RecordRef::from(&SOURCE_RECORD);
        assert!(!target.has::<Mbp1Msg>());
        assert!(!target.has::<Mbp10Msg>());
        assert!(!target.has::<TradeMsg>());
        assert!(!target.has::<ErrorMsg>());
        assert!(!target.has::<OhlcvMsg>());
        assert!(!target.has::<InstrumentDefMsg>());
        assert!(target.has::<MboMsg>());
        assert_eq!(*unsafe { target.get_unchecked::<MboMsg>() }, SOURCE_RECORD);
    }

    #[test]
    fn test_as_ref() {
        let target = RecordRef::from(&SOURCE_RECORD);
        let byte_slice = target.as_ref();
        assert_eq!(SOURCE_RECORD.record_size(), byte_slice.len());
        assert_eq!(target.record_size(), byte_slice.len());
    }

    #[should_panic]
    #[test]
    fn test_get_too_short() {
        let mut src = SOURCE_RECORD;
        src.hd.length -= 1;
        let target = unsafe { RecordRef::new(src.as_ref()) };
        // panic due to unexpected length
        target.get::<MboMsg>();
    }
}
