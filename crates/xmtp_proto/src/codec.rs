//! Transparent codec which just pipes bytes of encoded proto to a writer
use std::{io::Write, marker::PhantomData};

use prost::bytes::{Buf, BufMut, Bytes};
use tonic::{
    Status,
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
};

#[derive(Debug)]
pub struct TransparentEncoder(PhantomData<Bytes>);

impl Encoder for TransparentEncoder {
    type Error = Status;
    type Item = Bytes;

    fn encode(&mut self, item: Self::Item, buf: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        buf.writer()
            .write_all(&item)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct TransparentDecoder(PhantomData<Bytes>);

impl Decoder for TransparentDecoder {
    type Error = Status;
    type Item = Bytes;

    fn decode(&mut self, buf: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        let len = buf.remaining();
        Ok(Some(buf.copy_to_bytes(len)))
    }
}

#[derive(Debug, Clone)]
pub struct TransparentCodec(PhantomData<(Bytes, Bytes)>);

impl Default for TransparentCodec {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl Codec for TransparentCodec {
    type Decode = Bytes;
    type Decoder = TransparentDecoder;
    type Encode = Bytes;
    type Encoder = TransparentEncoder;

    fn encoder(&mut self) -> Self::Encoder {
        TransparentEncoder(PhantomData)
    }

    fn decoder(&mut self) -> Self::Decoder {
        TransparentDecoder(PhantomData)
    }
}
