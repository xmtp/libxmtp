# import hex encoding library
import binascii

# Converts a Rust Field5x52 representation to hex
def convert_to_hex(rust_repr):
    '''Converts "FieldElement5x52([1072957777321476, 2123452171292785, 2509825474272706, 2734633513915300, 161761072610823])"
    into hex serialized bytes
    Following this Rust Code but converted to Python:
    pub fn to_bytes(self) -> FieldBytes {
        let mut ret = FieldBytes::default();
        ret[0] = (self.0[4] >> 40) as u8;
        ret[1] = (self.0[4] >> 32) as u8;
        ret[2] = (self.0[4] >> 24) as u8;
        ret[3] = (self.0[4] >> 16) as u8;
        ret[4] = (self.0[4] >> 8) as u8;
        ret[5] = self.0[4] as u8;
        ret[6] = (self.0[3] >> 44) as u8;
        ret[7] = (self.0[3] >> 36) as u8;
        ret[8] = (self.0[3] >> 28) as u8;
        ret[9] = (self.0[3] >> 20) as u8;
        ret[10] = (self.0[3] >> 12) as u8;
        ret[11] = (self.0[3] >> 4) as u8;
        ret[12] = ((self.0[2] >> 48) as u8 & 0xFu8) | ((self.0[3] as u8 & 0xFu8) << 4);
        ret[13] = (self.0[2] >> 40) as u8;
        ret[14] = (self.0[2] >> 32) as u8;
        ret[15] = (self.0[2] >> 24) as u8;
        ret[16] = (self.0[2] >> 16) as u8;
        ret[17] = (self.0[2] >> 8) as u8;
        ret[18] = self.0[2] as u8;
        ret[19] = (self.0[1] >> 44) as u8;
        ret[20] = (self.0[1] >> 36) as u8;
        ret[21] = (self.0[1] >> 28) as u8;
        ret[22] = (self.0[1] >> 20) as u8;
        ret[23] = (self.0[1] >> 12) as u8;
        ret[24] = (self.0[1] >> 4) as u8;
        ret[25] = ((self.0[0] >> 48) as u8 & 0xFu8) | ((self.0[1] as u8 & 0xFu8) << 4);
        ret[26] = (self.0[0] >> 40) as u8;
        ret[27] = (self.0[0] >> 32) as u8;
        ret[28] = (self.0[0] >> 24) as u8;
        ret[29] = (self.0[0] >> 16) as u8;
        ret[30] = (self.0[0] >> 8) as u8;
        ret[31] = self.0[0] as u8;
        ret
    }
    '''
    # Remove the "FieldElement5x52([" and the "])" from the string
    rust_repr = rust_repr[18:-2]
    # Parse out the 5 64-bit numbers
    numbers = list(map(int, rust_repr.split(", ")))
    # Convert the numbers into bytes big endian
    # Prepare a 32 byte array to hold the result
    result = [0] * 32
    # Copy the bytes into the result using the shifting and conversion logic
    # from the Rust code
    #   ret[6] = (self.0[3] >> 44) as u8;
    #   ret[7] = (self.0[3] >> 36) as u8;
    #   ret[8] = (self.0[3] >> 28) as u8;
    #   ret[9] = (self.0[3] >> 20) as u8;
    #   ret[10] = (self.0[3] >> 12) as u8;
    #   ret[11] = (self.0[3] >> 4) as u8;
    #   ret[12] = ((self.0[2] >> 48) as u8 & 0xFu8) | ((self.0[3] as u8 & 0xFu8) << 4);
    #   ret[13] = (self.0[2] >> 40) as u8;
    #   ret[14] = (self.0[2] >> 32) as u8;
    #   ret[15] = (self.0[2] >> 24) as u8;
    #   ret[16] = (self.0[2] >> 16) as u8;
    #   ret[17] = (self.0[2] >> 8) as u8;
    #   ret[18] = self.0[2] as u8;
    #   ret[19] = (self.0[1] >> 44) as u8;
    #   ret[20] = (self.0[1] >> 36) as u8;
    #   ret[21] = (self.0[1] >> 28) as u8;
    #   ret[22] = (self.0[1] >> 20) as u8;
    #   ret[23] = (self.0[1] >> 12) as u8;
    #   ret[24] = (self.0[1] >> 4) as u8;
    #   ret[25] = ((self.0[0] >> 48) as u8 & 0xFu8) | ((self.0[1] as u8 & 0xFu8) << 4);
    #   ret[26] = (self.0[0] >> 40) as u8;
    #   ret[27] = (self.0[0] >> 32) as u8;
    #   ret[28] = (self.0[0] >> 24) as u8;
    #   ret[29] = (self.0[0] >> 16) as u8;
    #   ret[30] = (self.0[0] >> 8) as u8;
    #   ret[31] = self.0[0] as u8;

    result[0] = (numbers[4] >> 40) & 0xFF
    result[1] = (numbers[4] >> 32) & 0xFF
    result[2] = (numbers[4] >> 24) & 0xFF
    result[3] = (numbers[4] >> 16) & 0xFF
    result[4] = (numbers[4] >> 8) & 0xFF
    result[5] = numbers[4] & 0xFF
    result[6] = (numbers[3] >> 44) & 0xFF
    result[7] = (numbers[3] >> 36) & 0xFF
    result[8] = (numbers[3] >> 28) & 0xFF
    result[9] = (numbers[3] >> 20) & 0xFF
    result[10] = (numbers[3] >> 12) & 0xFF
    result[11] = (numbers[3] >> 4) & 0xFF
    result[12] = ((numbers[2] >> 48) & 0xF) | ((numbers[3] & 0xF) << 4)
    result[13] = (numbers[2] >> 40) & 0xFF
    result[14] = (numbers[2] >> 32) & 0xFF
    result[15] = (numbers[2] >> 24) & 0xFF
    result[16] = (numbers[2] >> 16) & 0xFF
    result[17] = (numbers[2] >> 8) & 0xFF
    result[18] = numbers[2] & 0xFF
    result[19] = (numbers[1] >> 44) & 0xFF
    result[20] = (numbers[1] >> 36) & 0xFF
    result[21] = (numbers[1] >> 28) & 0xFF
    result[22] = (numbers[1] >> 20) & 0xFF
    result[23] = (numbers[1] >> 12) & 0xFF
    result[24] = (numbers[1] >> 4) & 0xFF
    result[25] = ((numbers[0] >> 48) & 0xF) | ((numbers[1] & 0xF) << 4)
    result[26] = (numbers[0] >> 40) & 0xFF
    result[27] = (numbers[0] >> 32) & 0xFF
    result[28] = (numbers[0] >> 24) & 0xFF
    result[29] = (numbers[0] >> 16) & 0xFF
    result[30] = (numbers[0] >> 8) & 0xFF
    result[31] = numbers[0] & 0xFF
    print(result)
    # Convert array of ints to bytes
    return binascii.hexlify(bytes(result)).decode('utf-8')

if __name__ == "__main__":
    # Test the conversion
    print("Testing conversion of 1")
    # affine point: AffinePoint { x: FieldElement(FieldElementImpl { value: , magnitude: 1, normalized: true }), y: FieldElement(FieldElementImpl { value: FieldElement5x52([4498957494956921, 1893692404448273, 4320671074044728, 940458188030592, 207791006683003]), magnitude: 1, normalized: true }), infinity: 0 }
    print('x point: ', convert_to_hex('FieldElement5x52([1293581911306459, 534340368401923, 2629461762533819, 1417293729163992, 179486335370645])'))
    print('y point: ', convert_to_hex('FieldElement5x52([4498957494956921, 1893692404448273, 4320671074044728, 940458188030592, 207791006683003])'))
