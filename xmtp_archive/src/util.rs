use sha2::digest::{generic_array::GenericArray, typenum};

pub(crate) trait GenericArrayExt {
    /// Increment the generic array
    fn increment(&mut self);
    /// Decrement the generic array
    fn decrement(&mut self);
}

impl GenericArrayExt for GenericArray<u8, typenum::U12> {
    fn increment(&mut self) {
        let bytes = self.as_mut_slice();

        let mut buf = [0; 16];
        buf[..12].copy_from_slice(bytes);
        let val = u128::from_le_bytes(buf);
        let val = val.wrapping_add(1);

        bytes.copy_from_slice(&val.to_le_bytes()[..12]);
    }

    fn decrement(&mut self) {
        let bytes = self.as_mut_slice();

        let mut buf = [0; 16];
        buf[..12].copy_from_slice(bytes);
        let val = u128::from_le_bytes(buf);
        let val = val.wrapping_sub(1);

        bytes.copy_from_slice(&val.to_le_bytes()[..12]);
    }
}

#[cfg(test)]
mod tests {
    use sha2::digest::{generic_array::GenericArray, typenum};

    use crate::util::GenericArrayExt;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_generic_array_ext() {
        let mut nonce = [0u8; 12];
        nonce[0] = 1;

        let mut array: GenericArray<u8, typenum::U12> = GenericArray::clone_from_slice(&nonce);

        array.increment();

        nonce[0] = 2;
        assert_eq!(&nonce, array.as_slice());

        // Check that it underflows nicely
        for _ in 0..3 {
            array.decrement();
        }

        assert_eq!(&[255; 12], array.as_slice());

        // And overflow
        array.increment();
        assert_eq!(&[0; 12], array.as_slice());
    }
}
