use bytes::{Buf, BufMut, BytesMut};
use mbus::{Frame, ParseError};
use std::io::{Error, ErrorKind};
use tokio_util::codec::{Decoder, Encoder};

pub struct MbusCodec;

impl Decoder for MbusCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match Frame::try_parse(src.chunk()) {
            Ok((bytes_read, frame)) => {
                src.advance(bytes_read);
                Ok(Some(frame))
            }
            Err(ParseError::Incomplete(_)) => Ok(None),
            Err(err) => Err(Error::new(ErrorKind::InvalidData, err)),
        }
    }
}

impl Encoder<Frame> for MbusCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        for byte in item.iter_bytes() {
            dst.put_u8(byte);
        }

        Ok(())
    }
}
