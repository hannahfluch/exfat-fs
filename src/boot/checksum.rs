#[derive(Copy, Clone, Debug)]
pub struct Checksum {
    inner: u32,
    sector_size_in_bytes: u16,
}

impl Checksum {
    pub fn new(sector_size_in_bytes: u16) -> Checksum {
        Self {
            inner: 0,
            sector_size_in_bytes,
        }
    }
}

impl Checksum {
    /// Updates the checksum according to one entirely empty sector.
    pub fn zero_sector(&mut self) {
        for _ in 0..self.sector_size_in_bytes {
            self.inner = (self.inner & 1) * 0x80000000 + (self.inner >> 1);
        }
    }

    /// Updates the checksum according to a boot sector.
    pub fn boot_sector(&mut self, sector: &[u8]) {
        assert_eq!(sector.len(), self.sector_size_in_bytes as usize);
        for i in 0..self.sector_size_in_bytes {
            if i == 106 || i == 107 || i == 112 {
                continue;
            }

            self.inner =
                (self.inner & 1) * 0x80000000 + (self.inner >> 1) + sector[i as usize] as u32;
        }
    }

    /// Updates the checksum according to a set of extended boot sectors.
    pub fn extended_boot_sector(&mut self, sector: &[u8], amount: u64) {
        assert_eq!(sector.len(), self.sector_size_in_bytes as usize);
        for _ in 0..amount {
            for i in 0..self.sector_size_in_bytes {
                self.inner =
                    (self.inner & 1) * 0x80000000 + (self.inner >> 1) + sector[i as usize] as u32;
            }
        }
    }

    /// Returns a copy of the current state of the checksum in little-endian format.
    pub fn get(&self) -> u32 {
        self.inner.to_le()
    }
}
