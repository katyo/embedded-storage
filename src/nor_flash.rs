use crate::{iter::IterableByOverlaps, ReadStorage, Region, Storage};

/// NOR flash errors.
///
/// NOR flash implementations must use an error type implementing this trait. This permits generic
/// code to extract a generic error kind.
pub trait NorFlashError: core::fmt::Debug {
	/// Convert a specific NOR flash error into a generic error kind.
	fn kind(&self) -> NorFlashErrorKind;
}

impl NorFlashError for core::convert::Infallible {
	fn kind(&self) -> NorFlashErrorKind {
		match *self {}
	}
}

/// A trait that NorFlash implementations can use to share an error type.
pub trait ErrorType {
	/// Errors returned by this NOR flash.
	type Error: NorFlashError;
}

/// NOR flash error kinds.
///
/// NOR flash implementations must map their error to those generic error kinds through the
/// [`NorFlashError`] trait.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[non_exhaustive]
pub enum NorFlashErrorKind {
	/// The arguments are not properly aligned.
	NotAligned,

	/// The arguments are out of bounds.
	OutOfBounds,

	/// The cell already was written or cannot be written properly with provided value
	DirtyWrite,

	/// Error specific to the implementation.
	Other,
}

impl NorFlashError for NorFlashErrorKind {
	fn kind(&self) -> NorFlashErrorKind {
		*self
	}
}

impl core::fmt::Display for NorFlashErrorKind {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::NotAligned => write!(f, "Arguments are not properly aligned"),
			Self::OutOfBounds => write!(f, "Arguments are out of bounds"),
			Self::DirtyWrite => write!(f, "Dirty write operation"),
			Self::Other => write!(f, "An implementation specific error occurred"),
		}
	}
}

/// Read only NOR flash trait.
pub trait ReadNorFlash: ErrorType {
	/// The minumum number of bytes the storage peripheral can read
	const READ_SIZE: usize;

	/// Read a slice of data from the storage peripheral, starting the read
	/// operation at the given address offset, and reading `bytes.len()` bytes.
	///
	/// # Errors
	///
	/// Returns an error if the arguments are not aligned or out of bounds. The implementation
	/// can use the [`check_read`] helper function.
	fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error>;

	/// The capacity of the peripheral in bytes.
	fn capacity(&self) -> usize;
}

/// Return whether a read operation is within bounds.
pub fn check_read<T: ReadNorFlash>(
	flash: &T,
	offset: u32,
	length: usize,
) -> Result<(), NorFlashErrorKind> {
	check_slice(flash, T::READ_SIZE, offset, length)
}

/// NOR flash trait.
pub trait NorFlash: ReadNorFlash {
	/// The minumum number of bytes the storage peripheral can write
	const WRITE_SIZE: usize;

	/// The minumum number of bytes the storage peripheral can erase
	const ERASE_SIZE: usize;

	/// The content of erased storage
	///
	/// Usually is `0xff` for NOR flash
	const ERASE_BYTE: u8 = 0xff;

	/// Erase the given storage range, clearing all data within `[from..to]`.
	/// The given range will contain all 1s afterwards.
	///
	/// If power is lost during erase, contents of the page are undefined.
	///
	/// # Errors
	///
	/// Returns an error if the arguments are not aligned or out of bounds (the case where `to >
	/// from` is considered out of bounds). The implementation can use the [`check_erase`]
	/// helper function.
	fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error>;

	/// If power is lost during write, the contents of the written words are undefined,
	/// but the rest of the page is guaranteed to be unchanged.
	/// It is not allowed to write to the same word twice.
	///
	/// # Errors
	///
	/// Returns an error if the arguments are not aligned or out of bounds. The implementation
	/// can use the [`check_write`] helper function.
	fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error>;
}

/// Return whether an erase operation is aligned and within bounds.
pub fn check_erase<T: NorFlash>(flash: &T, from: u32, to: u32) -> Result<(), NorFlashErrorKind> {
	let (from, to) = (from as usize, to as usize);
	if from > to || to > flash.capacity() {
		return Err(NorFlashErrorKind::OutOfBounds);
	}
	if from % T::ERASE_SIZE != 0 || to % T::ERASE_SIZE != 0 {
		return Err(NorFlashErrorKind::NotAligned);
	}
	Ok(())
}

/// Return whether a write operation is aligned and within bounds.
pub fn check_write<T: NorFlash>(
	flash: &T,
	offset: u32,
	length: usize,
) -> Result<(), NorFlashErrorKind> {
	check_slice(flash, T::WRITE_SIZE, offset, length)
}

fn check_slice<T: ReadNorFlash>(
	flash: &T,
	align: usize,
	offset: u32,
	length: usize,
) -> Result<(), NorFlashErrorKind> {
	let offset = offset as usize;
	if length > flash.capacity() || offset > flash.capacity() - length {
		return Err(NorFlashErrorKind::OutOfBounds);
	}
	if offset % align != 0 || length % align != 0 {
		return Err(NorFlashErrorKind::NotAligned);
	}
	Ok(())
}

impl<T: ErrorType> ErrorType for &mut T {
	type Error = T::Error;
}

impl<T: ReadNorFlash> ReadNorFlash for &mut T {
	const READ_SIZE: usize = T::READ_SIZE;

	fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
		T::read(self, offset, bytes)
	}

	fn capacity(&self) -> usize {
		T::capacity(self)
	}
}

impl<T: NorFlash> NorFlash for &mut T {
	const WRITE_SIZE: usize = T::WRITE_SIZE;
	const ERASE_SIZE: usize = T::ERASE_SIZE;
	const ERASE_BYTE: u8 = T::ERASE_BYTE;

	fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
		T::erase(self, from, to)
	}

	fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
		T::write(self, offset, bytes)
	}
}

/// Marker trait for NorFlash relaxing the restrictions on `write`.
///
/// Writes to the same word twice are now allowed. The result is the logical AND of the
/// previous data and the written data. That is, it is only possible to change 1 bits to 0 bits.
///
/// If power is lost during write:
/// - Bits that were 1 on flash and are written to 1 are guaranteed to stay as 1
/// - Bits that were 1 on flash and are written to 0 are undefined
/// - Bits that were 0 on flash are guaranteed to stay as 0
/// - Rest of the bits in the page are guaranteed to be unchanged
pub trait MultiwriteNorFlash: NorFlash {}

struct Page {
	pub start: u32,
	pub size: usize,
}

impl Page {
	fn new(index: u32, size: usize) -> Self {
		Self {
			start: index * size as u32,
			size,
		}
	}

	/// The end address of the page
	const fn end(&self) -> u32 {
		self.start + self.size as u32
	}
}

impl Region for Page {
	/// Checks if an address offset is contained within the page
	fn contains(&self, address: u32) -> bool {
		(self.start <= address) && (self.end() > address)
	}
}

///
pub struct RmwNorFlashStorage<'a, S> {
	storage: S,
	merge_buffer: &'a mut [u8],
}

impl<'a, S> RmwNorFlashStorage<'a, S>
where
	S: NorFlash,
{
	/// Instantiate a new generic `Storage` from a `NorFlash` peripheral
	///
	/// **NOTE** This will panic if the provided merge buffer,
	/// is smaller than the erase size of the flash peripheral
	pub fn new(nor_flash: S, merge_buffer: &'a mut [u8]) -> Self {
		if merge_buffer.len() < S::ERASE_SIZE {
			panic!("Merge buffer is too small");
		}

		Self {
			storage: nor_flash,
			merge_buffer,
		}
	}
}

impl<'a, S> ReadStorage for RmwNorFlashStorage<'a, S>
where
	S: ReadNorFlash,
{
	type Error = S::Error;

	fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
		// Nothing special to be done for reads
		self.storage.read(offset, bytes)
	}

	fn capacity(&self) -> usize {
		self.storage.capacity()
	}
}

impl<'a, S> Storage for RmwNorFlashStorage<'a, S>
where
	S: NorFlash,
{
	fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
		// Perform read/modify/write operations on the byte slice.
		let last_page = self.storage.capacity() / S::ERASE_SIZE;

		// `data` is the part of `bytes` contained within `page`,
		// and `addr` in the address offset of `page` + any offset into the page as requested by `address`
		for (data, page, addr) in (0..last_page as u32)
			.map(move |i| Page::new(i, S::ERASE_SIZE))
			.overlaps(bytes, offset)
		{
			let offset_into_page = addr.saturating_sub(page.start) as usize;

			self.storage
				.read(page.start, &mut self.merge_buffer[..S::ERASE_SIZE])?;

			// If we cannot write multiple times to the same page, we will have to erase it
			self.storage.erase(page.start, page.end())?;
			self.merge_buffer[..S::ERASE_SIZE]
				.iter_mut()
				.skip(offset_into_page)
				.zip(data)
				.for_each(|(byte, input)| *byte = *input);
			self.storage
				.write(page.start, &self.merge_buffer[..S::ERASE_SIZE])?;
		}
		Ok(())
	}
}

///
pub struct RmwMultiwriteNorFlashStorage<'a, S> {
	storage: S,
	merge_buffer: &'a mut [u8],
}

impl<'a, S> RmwMultiwriteNorFlashStorage<'a, S>
where
	S: MultiwriteNorFlash,
{
	/// Instantiate a new generic `Storage` from a `NorFlash` peripheral
	///
	/// **NOTE** This will panic if the provided merge buffer,
	/// is smaller than the erase size of the flash peripheral
	pub fn new(nor_flash: S, merge_buffer: &'a mut [u8]) -> Self {
		if merge_buffer.len() < S::ERASE_SIZE {
			panic!("Merge buffer is too small");
		}

		Self {
			storage: nor_flash,
			merge_buffer,
		}
	}
}

impl<'a, S> ReadStorage for RmwMultiwriteNorFlashStorage<'a, S>
where
	S: ReadNorFlash,
{
	type Error = S::Error;

	fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
		// Nothing special to be done for reads
		self.storage.read(offset, bytes)
	}

	fn capacity(&self) -> usize {
		self.storage.capacity()
	}
}

impl<'a, S> Storage for RmwMultiwriteNorFlashStorage<'a, S>
where
	S: MultiwriteNorFlash,
{
	fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
		// Perform read/modify/write operations on the byte slice.
		let last_page = self.storage.capacity() / S::ERASE_SIZE;

		// `data` is the part of `bytes` contained within `page`,
		// and `addr` in the address offset of `page` + any offset into the page as requested by `address`
		for (data, page, addr) in (0..last_page as u32)
			.map(move |i| Page::new(i, S::ERASE_SIZE))
			.overlaps(bytes, offset)
		{
			let offset_into_page = addr.saturating_sub(page.start) as usize;

			self.storage
				.read(page.start, &mut self.merge_buffer[..S::ERASE_SIZE])?;

			let rhs = &self.merge_buffer[offset_into_page..S::ERASE_SIZE];
			let is_subset = data.iter().zip(rhs.iter()).all(|(a, b)| *a & *b == *a);

			// Check if we can write the data block directly, under the limitations imposed by NorFlash:
			// - We can only change 1's to 0's
			if is_subset {
				// Use `merge_buffer` as allocation for padding `data` to `WRITE_SIZE`
				let offset = addr as usize % S::WRITE_SIZE;
				let aligned_end = data.len() % S::WRITE_SIZE + offset + data.len();
				self.merge_buffer[..aligned_end].fill(S::ERASE_BYTE);
				self.merge_buffer[offset..offset + data.len()].copy_from_slice(data);
				self.storage
					.write(addr - offset as u32, &self.merge_buffer[..aligned_end])?;
			} else {
				self.storage.erase(page.start, page.end())?;
				self.merge_buffer[..S::ERASE_SIZE]
					.iter_mut()
					.skip(offset_into_page)
					.zip(data)
					.for_each(|(byte, input)| *byte = *input);
				self.storage
					.write(page.start, &self.merge_buffer[..S::ERASE_SIZE])?;
			}
		}
		Ok(())
	}
}

/// A wrapper for NOR flash storage to collect usage statistics
#[derive(Clone, Copy, Debug)]
pub struct NorFlashStats<S> {
	storage: S,
	/// Number of read operations
	pub reads: usize,
	/// Amount read chunks
	pub read: usize,
	/// Number of write operations
	pub writes: usize,
	/// Amount written chunks
	pub written: usize,
	/// Number of erase operations
	pub erases: usize,
	/// Amount of erased sectors
	pub erased: usize,
}

impl<S> From<S> for NorFlashStats<S> {
	fn from(storage: S) -> Self {
		Self {
			storage,
			reads: 0,
			read: 0,
			writes: 0,
			written: 0,
			erases: 0,
			erased: 0,
		}
	}
}

impl<S> NorFlashStats<S> {
	/// Unwrap to get wrapped storage instance
	pub fn into_inner(self) -> S {
		self.storage
	}
}

impl<S> core::ops::Deref for NorFlashStats<S> {
	type Target = S;

	fn deref(&self) -> &Self::Target {
		&self.storage
	}
}

impl<S> core::ops::DerefMut for NorFlashStats<S> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.storage
	}
}

impl<S: ErrorType> ErrorType for NorFlashStats<S> {
	type Error = S::Error;
}

impl<S: ReadNorFlash> ReadNorFlash for NorFlashStats<S> {
	const READ_SIZE: usize = S::READ_SIZE;

	fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), S::Error> {
		let res = self.storage.read(offset, bytes);
		if res.is_ok() {
			self.reads += 1;
			self.read += bytes.len() / S::READ_SIZE;
		}
		res
	}

	fn capacity(&self) -> usize {
		self.storage.capacity()
	}
}

impl<S: NorFlash> NorFlash for NorFlashStats<S> {
	const WRITE_SIZE: usize = S::WRITE_SIZE;
	const ERASE_SIZE: usize = S::ERASE_SIZE;
	const ERASE_BYTE: u8 = S::ERASE_BYTE;

	fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), S::Error> {
		let res = self.storage.write(offset, bytes);
		if res.is_ok() {
			self.writes += 1;
			self.written += bytes.len() / S::WRITE_SIZE;
		}
		res
	}

	fn erase(&mut self, from: u32, to: u32) -> Result<(), S::Error> {
		let res = self.storage.erase(from, to);
		if res.is_ok() {
			self.erases += 1;
			self.erased += (to - from) as usize / S::ERASE_SIZE;
		}
		res
	}
}

/// Simple RAM-backed flash storage implementation for tests
#[derive(Clone, Copy, Debug)]
pub struct MockFlash<
	const CAPACITY: usize,
	const READ_SIZE: usize = 1,
	const WRITE_SIZE: usize = 1,
	const ERASE_SIZE: usize = { 1 << 10 },
	const ERASE_BYTE: u8 = 0xff,
	const MULTI_WRITE: bool = false,
> {
	data: [u8; CAPACITY],
}

impl<
		const CAPACITY: usize,
		const READ_SIZE: usize,
		const WRITE_SIZE: usize,
		const ERASE_SIZE: usize,
		const ERASE_BYTE: u8,
		const MULTI_WRITE: bool,
	> Default for MockFlash<CAPACITY, READ_SIZE, WRITE_SIZE, ERASE_SIZE, ERASE_BYTE, MULTI_WRITE>
{
	fn default() -> Self {
		Self {
			data: [ERASE_BYTE; CAPACITY],
		}
	}
}

impl<
		const CAPACITY: usize,
		const READ_SIZE: usize,
		const WRITE_SIZE: usize,
		const ERASE_SIZE: usize,
		const ERASE_BYTE: u8,
		const MULTI_WRITE: bool,
	> core::ops::Deref
	for MockFlash<CAPACITY, READ_SIZE, WRITE_SIZE, ERASE_SIZE, ERASE_BYTE, MULTI_WRITE>
{
	type Target = [u8; CAPACITY];

	fn deref(&self) -> &Self::Target {
		&self.data
	}
}

impl<
		const CAPACITY: usize,
		const READ_SIZE: usize,
		const WRITE_SIZE: usize,
		const ERASE_SIZE: usize,
		const ERASE_BYTE: u8,
		const MULTI_WRITE: bool,
	> core::ops::DerefMut
	for MockFlash<CAPACITY, READ_SIZE, WRITE_SIZE, ERASE_SIZE, ERASE_BYTE, MULTI_WRITE>
{
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.data
	}
}

impl<
		const CAPACITY: usize,
		const READ_SIZE: usize,
		const WRITE_SIZE: usize,
		const ERASE_SIZE: usize,
		const ERASE_BYTE: u8,
		const MULTI_WRITE: bool,
	> ErrorType for MockFlash<CAPACITY, READ_SIZE, WRITE_SIZE, ERASE_SIZE, ERASE_BYTE, MULTI_WRITE>
{
	type Error = NorFlashErrorKind;
}

impl<
		const CAPACITY: usize,
		const READ_SIZE: usize,
		const WRITE_SIZE: usize,
		const ERASE_SIZE: usize,
		const ERASE_BYTE: u8,
		const MULTI_WRITE: bool,
	> ReadNorFlash for MockFlash<CAPACITY, READ_SIZE, WRITE_SIZE, ERASE_SIZE, ERASE_BYTE, MULTI_WRITE>
{
	const READ_SIZE: usize = READ_SIZE;

	fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
		check_read(self, offset, bytes.len())?;
		bytes.copy_from_slice(&self.data[offset as usize..][..bytes.len()]);
		Ok(())
	}

	fn capacity(&self) -> usize {
		CAPACITY
	}
}

impl<
		const CAPACITY: usize,
		const READ_SIZE: usize,
		const WRITE_SIZE: usize,
		const ERASE_SIZE: usize,
		const ERASE_BYTE: u8,
		const MULTI_WRITE: bool,
	> NorFlash for MockFlash<CAPACITY, READ_SIZE, WRITE_SIZE, ERASE_SIZE, ERASE_BYTE, MULTI_WRITE>
{
	const WRITE_SIZE: usize = WRITE_SIZE;
	const ERASE_SIZE: usize = ERASE_SIZE;
	const ERASE_BYTE: u8 = ERASE_BYTE;

	fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
		check_write(self, offset, bytes.len())?;
		for (dst, src) in self.data[offset as usize..].iter_mut().zip(bytes) {
			if !MULTI_WRITE && *dst != ERASE_BYTE {
				return Err(NorFlashErrorKind::DirtyWrite);
			}
			*dst &= *src;
			if *src != *dst {
				return Err(NorFlashErrorKind::DirtyWrite);
			}
		}
		Ok(())
	}

	fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
		check_erase(self, from, to)?;
		self.data[from as usize..to as usize].fill(ERASE_BYTE);
		Ok(())
	}
}

#[cfg(test)]
mod test {
	use super::*;

	const TEST_SIZE: usize = 64;
	const TEST_WORD: usize = 4;
	const TEST_PAGE: usize = 16;
	type TestFlash = MockFlash<TEST_SIZE, TEST_WORD, TEST_WORD, TEST_PAGE>;

	const fn gen_test_data<const N: usize>() -> [u8; N] {
		let mut data = [0u8; N];
		let mut i = 0;

		while i < N {
			data[i] = i as u8;
			i += 1;
		}

		data
	}

	const TEST_DATA: [u8; 64] = gen_test_data();

	fn gen_ranges(aligned: Option<bool>) -> impl Iterator<Item = (usize, usize)> {
		(0..TEST_SIZE).flat_map(move |off| {
			(0..=TEST_SIZE - off)
				.filter(move |len| {
					aligned
						.map(|aligned| aligned == (off % TEST_WORD == 0 && len % TEST_WORD == 0))
						.unwrap_or(true)
				})
				.map(move |len| (off, len))
		})
	}

	#[test]
	fn aligned_test_ranges() {
		let mut ranges = gen_ranges(true.into());

		assert_eq!(ranges.next(), Some((0, 0)));
		assert_eq!(ranges.next(), Some((0, 4)));
		assert_eq!(ranges.next(), Some((0, 8)));
		for _ in 0..13 {
			ranges.next();
		}
		assert_eq!(ranges.next(), Some((0, 64)));
		assert_eq!(ranges.next(), Some((4, 0)));
		assert_eq!(ranges.next(), Some((4, 4)));
		for _ in 0..13 {
			ranges.next();
		}
		assert_eq!(ranges.next(), Some((4, 60)));
		assert_eq!(ranges.next(), Some((8, 0)));
		for _ in 0..13 {
			ranges.next();
		}
		assert_eq!(ranges.next(), Some((8, 56)));
		assert_eq!(ranges.next(), Some((12, 0)));
		for _ in 0..12 {
			ranges.next();
		}
		assert_eq!(ranges.next(), Some((12, 52)));
		assert_eq!(ranges.next(), Some((16, 0)));
		for _ in 0..11 {
			ranges.next();
		}
		assert_eq!(ranges.next(), Some((16, 48)));
		assert_eq!(ranges.next(), Some((20, 0)));
	}

	#[test]
	fn not_aligned_test_ranges() {
		let mut ranges = gen_ranges(false.into());

		assert_eq!(ranges.next(), Some((0, 1)));
		assert_eq!(ranges.next(), Some((0, 2)));
		assert_eq!(ranges.next(), Some((0, 3)));
		assert_eq!(ranges.next(), Some((0, 5)));
		for _ in 0..43 {
			ranges.next();
		}
		assert_eq!(ranges.next(), Some((0, 63)));
		assert_eq!(ranges.next(), Some((1, 0)));
	}

	#[test]
	fn aligned_read_raw() {
		let mut flash = TestFlash::default();
		flash[..TEST_DATA.len()].copy_from_slice(&TEST_DATA);
		let mut buffer = [0; TEST_SIZE];

		for (off, len) in gen_ranges(true.into()) {
			assert_eq!(flash.read(off as u32, &mut buffer[..len]), Ok(()));
			assert_eq!(buffer[..len], TEST_DATA[off..][..len]);
		}
	}

	#[test]
	fn not_aligned_read_raw() {
		let mut flash = TestFlash::default();
		let mut buffer = [0; TEST_SIZE];

		for (off, len) in gen_ranges(false.into()) {
			assert_eq!(
				flash.read(off as u32, &mut buffer[..len]),
				Err(NorFlashErrorKind::NotAligned)
			);
		}
	}

	#[test]
	fn aligned_read_rmw() {
		let mut flash = TestFlash::default();
		flash[..TEST_DATA.len()].copy_from_slice(&TEST_DATA);
		let mut buffer = [0; TEST_SIZE];

		let mut flash_buffer = [0; TEST_PAGE];
		let mut flash = RmwNorFlashStorage::new(&mut flash, &mut flash_buffer);

		for (off, len) in gen_ranges(true.into()) {
			assert_eq!(flash.read(off as u32, &mut buffer[..len]), Ok(()));
			assert_eq!(buffer[..len], TEST_DATA[off..][..len]);
		}
	}

	#[test]
	fn not_aligned_read_rmw() {
		let mut flash = TestFlash::default();
		flash[..TEST_DATA.len()].copy_from_slice(&TEST_DATA);
		let mut buffer = [0; TEST_SIZE];

		let mut flash_buffer = [0; TEST_PAGE];
		let mut flash = RmwNorFlashStorage::new(&mut flash, &mut flash_buffer);

		for (off, len) in gen_ranges(false.into()) {
			assert_eq!(flash.read(off as u32, &mut buffer[..len]), Ok(()));
			assert_eq!(buffer[..len], TEST_DATA[off..][..len]);
		}
	}

	#[test]
	fn aligned_write_raw() {
		let mut flash = TestFlash::default();

		for (off, len) in gen_ranges(true.into()) {
			assert_eq!(flash.erase(0, TEST_SIZE as u32), Ok(()));
			assert_eq!(flash.write(off as u32, &TEST_DATA[..len]), Ok(()));
			assert_eq!(flash[off..][..len], TEST_DATA[..len]);
		}
	}

	#[test]
	fn not_aligned_write_raw() {
		let mut flash = TestFlash::default();

		for (off, len) in gen_ranges(false.into()) {
			assert_eq!(
				flash.write(off as u32, &TEST_DATA[..len]),
				Err(NorFlashErrorKind::NotAligned)
			);
		}
	}

	#[test]
	fn not_aligned_erase_raw() {
		let mut flash = TestFlash::default();

		for (off, len) in [
			(1usize, TEST_PAGE),
			(0, TEST_PAGE - 1),
			(TEST_PAGE, TEST_PAGE + 1),
		] {
			assert_eq!(
				flash.erase(off as u32, (off + len) as u32),
				Err(NorFlashErrorKind::NotAligned)
			);
		}
	}

	#[test]
	fn aligned_write_rmw() {
		let mut flash = TestFlash::default();
		let mut flash_buffer = [0u8; TEST_PAGE];

		for (off, len) in gen_ranges(true.into()) {
			{
				let mut flash = RmwNorFlashStorage::new(&mut flash, &mut flash_buffer);
				assert_eq!(flash.write(off as u32, &TEST_DATA[..len]), Ok(()));
			}
			assert_eq!(flash[off..][..len], TEST_DATA[..len]);
		}
	}

	#[test]
	fn not_aligned_write_rmw() {
		let mut flash = TestFlash::default();
		let mut flash_buffer = [0u8; TEST_PAGE];

		for (off, len) in gen_ranges(false.into()) {
			{
				let mut flash = RmwNorFlashStorage::new(&mut flash, &mut flash_buffer);
				assert_eq!(flash.write(off as u32, &TEST_DATA[..len]), Ok(()));
			}
			assert_eq!(flash[off..][..len], TEST_DATA[..len]);
		}
	}
}
