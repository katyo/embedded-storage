use embedded_storage::nor_flash::ErrorType;

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
	async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error>;

	/// The capacity of the peripheral in bytes.
	fn capacity(&self) -> usize;
}

/// NOR flash trait.
pub trait NorFlash: ReadNorFlash {
	/// The minumum number of bytes the storage peripheral can write
	const WRITE_SIZE: usize;

	/// The minumum number of bytes the storage peripheral can erase
	const ERASE_SIZE: usize;

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
	async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error>;

	/// If power is lost during write, the contents of the written words are undefined,
	/// but the rest of the page is guaranteed to be unchanged.
	/// It is not allowed to write to the same word twice.
	///
	/// # Errors
	///
	/// Returns an error if the arguments are not aligned or out of bounds. The implementation
	/// can use the [`check_write`] helper function.
	async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error>;
}

impl<T: ErrorType> ErrorType for &mut T {
	type Error = T::Error;
}

impl<T: ReadNorFlash> ReadNorFlash for &mut T {
	const READ_SIZE: usize = T::READ_SIZE;

	async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
		T::read(self, offset, bytes)
	}

	fn capacity(&self) -> usize {
		T::capacity(self)
	}
}

impl<T: NorFlash> NorFlash for &mut T {
	const WRITE_SIZE: usize = T::WRITE_SIZE;
	const ERASE_SIZE: usize = T::ERASE_SIZE;

	async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
		T::erase(self, from, to)
	}

	async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
		T::write(self, offset, bytes)
	}
}
